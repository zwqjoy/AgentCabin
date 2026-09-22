import test from "node:test";
import assert from "node:assert/strict";
import zlib from "node:zlib";
import { existsSync } from "node:fs";
import {
  ComputerUseV2Session,
  VisualGroundingBackend,
  MODE_SEMANTIC,
  MODE_VISUAL,
  MODE_FUSED,
  computePerceptualDiff,
  defaultOcrProvider,
  resolveOcrHelper,
} from "./computer_use_v2_runtime.mjs";

function envelope(parsed) {
  return { success: true, status: "success", stdout: JSON.stringify(parsed) };
}

test("Milestone D: VisualGroundingBackend selectively triggers on empty tree, canvas, or explicit visual/fused mode", () => {
  const backend = new VisualGroundingBackend();

  // 1. Interactive semantic tree does not trigger in default semantic mode
  const richElements = [
    { role: "button", title: "OK", rect: { x: 10, y: 10, width: 80, height: 30 } },
    { role: "input", title: "Search", rect: { x: 10, y: 50, width: 200, height: 30 } },
  ];
  assert.equal(backend.shouldTrigger(richElements, MODE_SEMANTIC), false);

  // 2. Explicit mode 'visual' or 'fused' triggers even if elements exist
  assert.equal(backend.shouldTrigger(richElements, MODE_VISUAL), true);
  assert.equal(backend.shouldTrigger(richElements, MODE_FUSED), true);

  // 3. Empty elements triggers
  assert.equal(backend.shouldTrigger([], MODE_SEMANTIC), true);
  assert.equal(backend.shouldTrigger(undefined, MODE_SEMANTIC), true);

  // 4. Picture-only or Canvas UI without interactive controls triggers
  const canvasElements = [
    { role: "canvas", title: "Game Canvas", rect: { x: 0, y: 0, width: 800, height: 600 } },
    { role: "image", title: "Splash Art", rect: { x: 10, y: 10, width: 100, height: 100 } },
  ];
  assert.equal(backend.shouldTrigger(canvasElements, MODE_SEMANTIC), true);
});

test("Milestone D: VisualGrounding grounds visual detector and OCR elements with evidence: { visual: true }", async () => {
  const mockDetector = async () => [
    { role: "button", label: "Submit", rect: { x: 100, y: 200, width: 80, height: 40 } },
  ];
  const mockOcr = async () => [
    { text: "Total: $42.00", rect: { x: 100, y: 260, width: 120, height: 25 } },
  ];

  const backend = new VisualGroundingBackend({ detector: mockDetector, ocr: mockOcr });
  const fakeScreenshot = { data: "base64-fake-image-png-data" };
  const grounded = await backend.ground(fakeScreenshot);

  assert.equal(grounded.length, 2);
  const textEl = grounded.find((e) => e.title === "Total: $42.00");
  assert.ok(textEl);
  assert.equal(textEl.evidence?.visual, true);
  assert.equal(textEl.evidence?.ocr, true);

  const btnEl = grounded.find((e) => e.title === "Submit");
  assert.ok(btnEl);
  assert.equal(btnEl.evidence?.visual, true);
});

test("Milestone D: Session observeUi automatically enriches empty/canvas observation with visual controls", async () => {
  const mockDetector = async () => [
    { role: "button", label: "Play Game", rect: { x: 150, y: 300, width: 100, height: 50 } },
  ];
  const visualBackend = new VisualGroundingBackend({ detector: mockDetector });

  const invokeBackend = async () => envelope({
    structuredContent: {
      pid: 999,
      window_id: 1,
      elements: [{ role: "canvas", title: "HTML5 WebGL" }],
    },
    content: [{ type: "image", data: "fake-screenshot-data" }],
  });

  const session = new ComputerUseV2Session(invokeBackend, { visualBackend });
  const observed = await session.observeUi("call:1");

  assert.equal(observed.details.ok, true);
  const elements = observed.details.elements;
  assert.ok(elements.length >= 2);

  const visualBtn = elements.find((e) => e.title === "Play Game");
  assert.ok(visualBtn);
  assert.match(visualBtn.ref, /^@e\d+$/);
  assert.equal(visualBtn.evidence?.visual, true);
});

test("Milestone D: Visual click resolves center coordinates and executes without native element_token", async () => {
  let executedAction;
  const invokeBackend = async (_callId, tool, intent, params) => {
    if (intent === "observe") {
      return envelope({
        structuredContent: {
          pid: 500,
          window_id: 1,
          elements: [], // empty -> triggers visual
        },
        content: [{ type: "image", data: "test-img" }],
      });
    }
    if (intent === "act_batch") {
      executedAction = params.actions[0];
      return envelope({
        structuredContent: {
          execution: { steps: [{ outcome: "worked" }] },
          observation: { pid: 500, window_id: 1, elements: [] },
        },
      });
    }
    return envelope({});
  };

  const mockDetector = async () => [
    { role: "button", label: "Start", rect: { x: 100, y: 200, width: 80, height: 40 } },
  ];
  const visualBackend = new VisualGroundingBackend({ detector: mockDetector });

  const session = new ComputerUseV2Session(invokeBackend, { visualBackend });
  const observed = await session.observeUi("call:obs");
  const visualBtn = observed.details.elements.find((e) => e.title === "Start");
  assert.ok(visualBtn);

  const actResult = await session.actUi("call:act", {
    stateId: observed.details.stateId,
    actions: [{ action: "click", ref: visualBtn.ref }],
  });

  assert.equal(actResult.details.ok, true);
  assert.ok(executedAction);
  assert.equal(executedAction.tool, "desktop_click");
  assert.equal(executedAction.params.element_token, undefined); // No native AX token for visual element
  assert.equal(executedAction.params.x, 140); // 100 + 80/2
  assert.equal(executedAction.params.y, 220); // 200 + 40/2
});

test("Milestone D: State-scoped coordinate security rejects cross-state coordinate reuse", async () => {
  const invokeBackend = async () => envelope({
    structuredContent: { pid: 10, window_id: 1, elements: [] },
  });

  const session = new ComputerUseV2Session(invokeBackend);
  const obs1 = await session.observeUi("call:obs1");

  // Attempting to act with a mismatched action stateId throws
  await assert.rejects(
    async () => {
      session.legacyAction(session.state(obs1.details.stateId), {
        action: "click",
        stateId: "mismatched-state-id-9999",
        x: 100,
        y: 100,
      });
    },
    /Cross-state coordinate reuse is prohibited/
  );
});

test("Milestone D: Multimodal successor verification validates visual changes and diffs", async () => {
  let observeCount = 0;
  const invokeBackend = async (_callId, _tool, intent) => {
    if (intent === "observe" || intent === "act_batch") {
      observeCount++;
      return envelope({
        structuredContent: {
          pid: 300,
          window_id: 1,
          elements: [],
          observation: {
            pid: 300,
            window_id: 1,
            elements: [],
          },
        },
        content: [{ type: "image", data: observeCount === 1 ? "img-before-hash" : "img-after-hash" }],
      });
    }
    return envelope({});
  };

  const mockOcr = async (img) => {
    if (img === "img-after-hash") {
      return [{ text: "Success Notification", rect: { x: 50, y: 50, width: 200, height: 30 } }];
    }
    return [];
  };

  const visualBackend = new VisualGroundingBackend({ ocr: mockOcr });
  const session = new ComputerUseV2Session(invokeBackend, { visualBackend });

  const initial = await session.observeUi("call:init");
  assert.equal(initial.details.elements.length, 0);

  const act = await session.actUi("call:act", {
    stateId: initial.details.stateId,
    actions: [{ action: "click", x: 100, y: 100 }],
    expect: { text: "Success Notification", requireVisualDiff: true },
  });

  assert.equal(act.details.ok, true);
  assert.equal(act.details.execution.outcome, "worked");
  assert.equal(act.details.execution.verification.status, "verified");
});

test("Milestone D: computePerceptualDiff calculates deterministic perceptual delta", () => {
  const imgA = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=", "base64").toString("base64");
  const imgSame = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=", "base64").toString("base64");
  const imgDiff = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64").toString("base64");

  assert.equal(computePerceptualDiff(imgA, imgSame), 0.0, "identical images must yield 0 diff");
  const diff = computePerceptualDiff(imgA, imgDiff);
  assert.ok(diff > 0.01, `distinct images must yield significant delta (${diff})`);
});

test("Milestone D: defaultOcrProvider executes successfully on live macOS", async () => {
  if (process.platform !== "darwin") return;

  // Tiny 1x1 test payload to verify invocation pipeline
  const payload = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
  const boxes = await defaultOcrProvider(payload);
  assert.ok(Array.isArray(boxes), "OCR output must be an array");
});

test("Regression 6: OCR production helper path resolution", () => {
  if (process.platform !== "darwin") return;

  const helper = resolveOcrHelper();
  assert.ok(helper, "OCR helper must be resolved on macOS");
  assert.ok(helper.path, "OCR helper path must be present");
  assert.equal(typeof helper.type, "string");
  assert.equal(existsSync(helper.path), true, `Resolved helper path must exist on disk: ${helper.path}`);
  assert.equal(helper.origin, "bundled", "Bundled production helper must take precedence");
  assert.equal(helper.type, "binary", "Production helper must be a compiled binary executable");
});

test("Regression 7: decoded image perceptual diff correctness", () => {
  function makeTestPng(width, height, pixelFn, level = 6) {
    const header = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(width, 0);
    ihdr.writeUInt32BE(height, 4);
    ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;

    function chunk(type, data) {
      const l = Buffer.alloc(4); l.writeUInt32BE(data.length, 0);
      return Buffer.concat([l, Buffer.from(type, "ascii"), data, Buffer.alloc(4)]);
    }

    const stride = 1 + width * 4;
    const raw = Buffer.alloc(height * stride);
    for (let y = 0; y < height; y++) {
      raw[y * stride] = 0; // Filter None
      for (let x = 0; x < width; x++) {
        const [r, g, b, a] = pixelFn(x, y);
        const idx = y * stride + 1 + x * 4;
        raw[idx] = r; raw[idx+1] = g; raw[idx+2] = b; raw[idx+3] = a;
      }
    }

    const idat = zlib.deflateSync(raw, { level });
    return Buffer.concat([header, chunk("IHDR", ihdr), chunk("IDAT", idat), chunk("IEND", Buffer.alloc(0))]).toString("base64");
  }

  // 1. Identical images: diff = 0
  const imgBase = makeTestPng(64, 64, () => [240, 240, 240, 255], 1);
  assert.equal(computePerceptualDiff(imgBase, imgBase), 0.0, "Identical image must yield exactly 0 diff");

  // 2. Same image with different compression levels (level 1 vs level 9): diff < 0.005
  const imgSameDiffCompression = makeTestPng(64, 64, () => [240, 240, 240, 255], 9);
  const compDiff = computePerceptualDiff(imgBase, imgSameDiffCompression);
  assert.ok(compDiff < 0.005, `Different compression of same pixels must yield diff < 0.005 (got ${compDiff})`);
  assert.equal(compDiff, 0.0, "Identical decoded pixels must yield exactly 0 diff regardless of compression");

  // 3. Cursor blink (3 pixels changed out of 4096): diff < 0.005 (below default threshold)
  const imgCursor = makeTestPng(64, 64, (x, y) => (x === 10 && y >= 10 && y <= 12) ? [0, 0, 0, 255] : [240, 240, 240, 255]);
  const cursorDiff = computePerceptualDiff(imgBase, imgCursor);
  assert.ok(cursorDiff < 0.005, `Cursor blink must be below threshold 0.005 (got ${cursorDiff})`);

  // 4. Modal appearance (25x25 dark rect): diff > 0.05 (well above threshold)
  const imgModal = makeTestPng(64, 64, (x, y) => (x >= 20 && x <= 45 && y >= 20 && y <= 45) ? [30, 30, 30, 255] : [240, 240, 240, 255]);
  const modalDiff = computePerceptualDiff(imgBase, imgModal);
  assert.ok(modalDiff > 0.05, `Modal appearance must be above threshold 0.05 (got ${modalDiff})`);
});


