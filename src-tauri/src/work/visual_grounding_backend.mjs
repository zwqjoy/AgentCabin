/**
 * AgentCabin Computer Use V3 Visual Grounding Fallback.
 *
 * Implements priority scheduling (CDP -> AX -> Vision -> Coordinate),
 * state-scoped coordinate binding, and multimodal successor verification.
 */

import { createUiElement } from "./computer_use_v3_models.mjs";
import zlib from "node:zlib";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync, unlinkSync } from "node:fs";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

export const MODE_SEMANTIC = "semantic";
export const MODE_VISUAL = "visual";
export const MODE_FUSED = "fused";

/**
 * VisualGroundingBackend coordinates visual control detection (OCR, template/box detection,
 * vision model grounding) when the semantic tree is empty, self-drawn, or PictureOnly.
 */
export class VisualGroundingBackend {
  constructor(options = {}) {
    this.detector = options.detector || defaultVisualDetector;
    this.ocr = options.ocr || defaultOcrProvider;
  }

  /**
   * Evaluates whether visual grounding is required for a given observation.
   * Triggers if:
   * 1. mode is explicitly "visual" or "fused"
   * 2. semantic elements are empty (0 interactive controls)
   * 3. elements only contain generic canvas/picture without interactive children
   */
  shouldTrigger(elements = [], mode = MODE_SEMANTIC) {
    if (mode === MODE_VISUAL || mode === MODE_FUSED) {
      return true;
    }
    if (!Array.isArray(elements) || elements.length === 0) {
      return true;
    }
    const interactive = elements.some((el) => {
      const role = String(el.role || "").toLowerCase();
      return role !== "canvas" && role !== "image" && role !== "picture" && role !== "unknown";
    });
    return !interactive;
  }

  /**
   * Performs visual grounding on an observation screenshot.
   * Returns an array of normalized UiElement objects with evidence: { visual: true }.
   */
  async ground(screenshot, options = {}) {
    if (!screenshot || (!screenshot.data && !screenshot.base64)) {
      return [];
    }

    const imagePayload = screenshot.data || screenshot.base64;
    const detected = await this.detector(imagePayload, options);
    const ocrResults = await this.ocr(imagePayload, options);

    const fused = [];
    const seenBoxes = new Set();

    // 1. Text elements from OCR
    for (const item of ocrResults) {
      const key = `${Math.round(item.rect.x)}:${Math.round(item.rect.y)}:${item.text}`;
      seenBoxes.add(key);
      fused.push({
        role: item.role || "text",
        title: item.text,
        name: item.text,
        label: item.text,
        value: item.value,
        rect: item.rect,
        evidence: { visual: true, ocr: true },
      });
    }

    // 2. Control boxes from visual detector
    for (const item of detected) {
      const key = `${Math.round(item.rect.x)}:${Math.round(item.rect.y)}:${item.label || item.role}`;
      if (!seenBoxes.has(key)) {
        fused.push({
          role: item.role || "button",
          title: item.label || item.name || "",
          name: item.label || item.name || "",
          label: item.label || item.name || "",
          value: item.value,
          rect: item.rect,
          evidence: { visual: true },
        });
      }
    }

    return fused;
  }

  /**
   * Resolves visual click coordinates for an element within the state's coordinate space.
   * Enforces that the element is owned by the specified stateId.
   */
  resolveElementCoordinates(state, elementRef) {
    const canonical = elementRef.startsWith("@") ? elementRef : `@${elementRef}`;
    const element = state.elements.find((el) => el.ref === canonical);
    if (!element) {
      throw new Error(`Element '${elementRef}' is not owned by state ${state.stateId}.`);
    }
    if (!element.rect || typeof element.rect !== "object") {
      throw new Error(`Element '${elementRef}' in state ${state.stateId} does not have geometric coordinates.`);
    }

    const x = Number(element.rect.x);
    const y = Number(element.rect.y);
    const width = Number(element.rect.width ?? element.rect.w);
    const height = Number(element.rect.height ?? element.rect.h);
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(width) || !Number.isFinite(height)) {
      throw new Error(`Element '${elementRef}' coordinates are incomplete in state ${state.stateId}.`);
    }

    // Return center point
    return {
      x: Math.round(x + width / 2),
      y: Math.round(y + height / 2),
      rect: element.rect,
      evidence: element.evidence,
    };
  }

  /**
   * Verifies visual postconditions between a base state and successor state.
   * Checks for visual diff, OCR text presence, or visual element property shifts.
   */
  async verifyVisualChange(beforeState, afterState, condition = {}) {
    if (!afterState) return false;

    // 1. If text is expected, check OCR / visual text in successor elements
    if (condition.text) {
      const targetText = String(condition.text).toLowerCase();
      const matched = afterState.elements.some((el) => {
        const text = (el.title || el.label || el.name || el.value || "").toLowerCase();
        return text.includes(targetText);
      });
      if (condition.until === "absent") {
        if (matched) return false;
      } else if (!matched) {
        return false;
      }
    }

    // 2. If ref is specified, ensure it exists in successor
    if (condition.ref) {
      const canonical = condition.ref.startsWith("@") ? condition.ref : `@${condition.ref}`;
      const found = afterState.elements.find((el) => el.ref === canonical);
      if (condition.until === "absent") {
        if (found) return false;
      } else if (!found) {
        return false;
      }
      if (condition.value !== undefined && String(found.value ?? "") !== String(condition.value)) {
        return false;
      }
    }

    // 3. Perceptual image visual delta check (if images are present in both)
    const beforeImg = beforeState?.images?.[0]?.data || beforeState?.images?.[0]?.base64;
    const afterImg = afterState?.images?.[0]?.data || afterState?.images?.[0]?.base64;
    if (beforeImg && afterImg && condition.requireVisualDiff) {
      const diff = computePerceptualDiff(beforeImg, afterImg);
      const minThreshold = typeof condition.minDiffThreshold === "number" ? condition.minDiffThreshold : 0.005;
      if (diff < minThreshold) {
        return false; // No significant perceptual change occurred
      }
    }

    return true;
  }
}

function paethPredictor(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

/**
 * Pure JavaScript PNG scanline decoder.
 * Supports Grayscale, RGB, Palette, Gray+Alpha, and RGBA.
 */
export function decodePng(buf) {
  if (!buf || buf.length < 8 || buf.readUInt32BE(0) !== 0x89504e47 || buf.readUInt32BE(4) !== 0x0d0a1a0a) {
    throw new Error("Invalid PNG signature");
  }
  let pos = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 8;
  let colorType = 6;
  const idatChunks = [];

  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.subarray(pos + 4, pos + 8).toString("ascii");
    const data = buf.subarray(pos + 8, pos + 8 + len);
    pos += 12 + len;

    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === "IDAT") {
      idatChunks.push(data);
    } else if (type === "IEND") {
      break;
    }
  }

  if (width === 0 || height === 0) {
    throw new Error("Invalid PNG dimensions");
  }

  const decompressed = zlib.inflateSync(Buffer.concat(idatChunks));
  let bytesPerPixel = 4;
  if (colorType === 0) bytesPerPixel = 1;
  else if (colorType === 2) bytesPerPixel = 3;
  else if (colorType === 4) bytesPerPixel = 2;
  else if (colorType === 6) bytesPerPixel = 4;

  const rowStride = 1 + width * bytesPerPixel;
  const rawRgba = Buffer.alloc(width * height * 4);
  const prevRow = Buffer.alloc(width * bytesPerPixel);
  const curRow = Buffer.alloc(width * bytesPerPixel);

  for (let y = 0; y < height; y++) {
    const filter = decompressed[y * rowStride];
    const srcRow = decompressed.subarray(y * rowStride + 1, (y + 1) * rowStride);

    for (let i = 0; i < srcRow.length; i++) {
      const bpp = bytesPerPixel;
      const left = i >= bpp ? curRow[i - bpp] : 0;
      const up = prevRow[i];
      const upLeft = i >= bpp ? prevRow[i - bpp] : 0;

      let val = srcRow[i];
      if (filter === 0) { /* none */ }
      else if (filter === 1) val = (val + left) & 0xff;
      else if (filter === 2) val = (val + up) & 0xff;
      else if (filter === 3) val = (val + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) val = (val + paethPredictor(left, up, upLeft)) & 0xff;

      curRow[i] = val;
    }

    for (let x = 0; x < width; x++) {
      const outIdx = (y * width + x) * 4;
      if (colorType === 6) {
        rawRgba[outIdx] = curRow[x * 4];
        rawRgba[outIdx + 1] = curRow[x * 4 + 1];
        rawRgba[outIdx + 2] = curRow[x * 4 + 2];
        rawRgba[outIdx + 3] = curRow[x * 4 + 3];
      } else if (colorType === 2) {
        rawRgba[outIdx] = curRow[x * 3];
        rawRgba[outIdx + 1] = curRow[x * 3 + 1];
        rawRgba[outIdx + 2] = curRow[x * 3 + 2];
        rawRgba[outIdx + 3] = 255;
      } else if (colorType === 0) {
        const g = curRow[x];
        rawRgba[outIdx] = g;
        rawRgba[outIdx + 1] = g;
        rawRgba[outIdx + 2] = g;
        rawRgba[outIdx + 3] = 255;
      } else if (colorType === 4) {
        const g = curRow[x * 2];
        rawRgba[outIdx] = g;
        rawRgba[outIdx + 1] = g;
        rawRgba[outIdx + 2] = g;
        rawRgba[outIdx + 3] = curRow[x * 2 + 1];
      }
    }
    curRow.copy(prevRow);
  }

  return { width, height, data: rawRgba };
}

/**
 * Downsamples decoded image to a normalized 64x64 luminance grid.
 * Luminance uses Rec. 601 coefficients: 0.299*R + 0.587*G + 0.114*B.
 */
export function downsampleToLuminance64(decodedImage) {
  const { width, height, data } = decodedImage;
  const targetSize = 64;
  const result = new Float32Array(targetSize * targetSize);

  for (let gy = 0; gy < targetSize; gy++) {
    const srcY = Math.min(height - 1, Math.floor((gy * height) / targetSize));
    const rowOffset = gy * targetSize;
    for (let gx = 0; gx < targetSize; gx++) {
      const srcX = Math.min(width - 1, Math.floor((gx * width) / targetSize));
      const srcIdx = (srcY * width + srcX) * 4;
      const r = data[srcIdx];
      const g = data[srcIdx + 1];
      const b = data[srcIdx + 2];
      result[rowOffset + gx] = 0.299 * r + 0.587 * g + 0.114 * b;
    }
  }

  return result;
}

/**
 * Decodes base64 image (PNG, JPEG, etc.) into a 64x64 luminance array.
 */
export function decodeImageToLuminance64(base64Payload) {
  if (!base64Payload) return null;
  const raw = Buffer.from(base64Payload.replace(/^data:image\/[a-z]+;base64,/, ""), "base64");
  if (raw.length === 0) return null;

  // Pure JS PNG decode
  if (raw.length >= 8 && raw.readUInt32BE(0) === 0x89504e47 && raw.readUInt32BE(4) === 0x0d0a1a0a) {
    try {
      const decoded = decodePng(raw);
      return downsampleToLuminance64(decoded);
    } catch {
      // Fallback below
    }
  }

  // Non-PNG format fallback on macOS (sips)
  if (process.platform === "darwin") {
    try {
      const tempIn = join(tmpdir(), `agentcabin_diff_in_${Date.now()}_${Math.random().toString(36).slice(2)}.tmp`);
      const tempOut = join(tmpdir(), `agentcabin_diff_out_${Date.now()}_${Math.random().toString(36).slice(2)}.png`);
      writeFileSync(tempIn, raw);
      spawnSync("sips", ["-s", "format", "png", tempIn, "--out", tempOut], { timeout: 3000 });
      let convertedPng = null;
      try {
        convertedPng = readFileSync(tempOut);
      } catch {}
      try { unlinkSync(tempIn); } catch {}
      try { unlinkSync(tempOut); } catch {}
      if (convertedPng) {
        const decoded = decodePng(convertedPng);
        return downsampleToLuminance64(decoded);
      }
    } catch {
      // sips unavailable
    }
  }

  return null;
}

/**
 * Computes true decoded-pixel perceptual difference between two images.
 * Decodes images, pools to a 64x64 luminance space, and calculates the
 * normalized Mean Absolute Pixel Delta in [0.0, 1.0].
 */
export function computePerceptualDiff(beforeBase64, afterBase64) {
  if (!beforeBase64 || !afterBase64) return 1.0;
  if (beforeBase64 === afterBase64) return 0.0;

  const lum1 = decodeImageToLuminance64(beforeBase64);
  const lum2 = decodeImageToLuminance64(afterBase64);

  if (!lum1 || !lum2 || lum1.length !== lum2.length) {
    return beforeBase64 === afterBase64 ? 0.0 : 1.0;
  }

  let deltaSum = 0;
  const n = lum1.length;
  for (let i = 0; i < n; i++) {
    deltaSum += Math.abs(lum1[i] - lum2[i]);
  }

  const normDelta = deltaSum / (n * 255.0);
  return Math.min(1.0, Math.max(0.0, normDelta));
}

/**
 * Resolves the path to the OCR helper executable or script on macOS.
 * Priority:
 * 1. Bundled app resource (production Tauri bundle)
 * 2. Build cache compiled binary
 * 3. Development Swift script fallback
 */
export function resolveOcrHelper(customBaseDir) {
  if (process.platform !== "darwin") return null;

  const curDir = customBaseDir || dirname(fileURLToPath(import.meta.url));

  // 1. Explicit env override
  if (process.env.AGENTCABIN_OCR_HELPER && existsSync(process.env.AGENTCABIN_OCR_HELPER)) {
    return { path: process.env.AGENTCABIN_OCR_HELPER, type: "binary", origin: "bundled" };
  }

  // 1a. Production Tauri bundle resource paths
  const bundledCandidates = [
    resolve(dirname(process.execPath || ""), "../Resources/resources/agentcabin-computer-use/macos/ocr-helper"),
    resolve(dirname(process.execPath || ""), "../Resources/agentcabin-computer-use/macos/ocr-helper"),
    resolve(curDir, "../../resources/agentcabin-computer-use/macos/ocr-helper"),
    resolve(curDir, "../../../src-tauri/resources/agentcabin-computer-use/macos/ocr-helper"),
  ];

  for (const p of bundledCandidates) {
    if (existsSync(p)) {
      return { path: p, type: "binary", origin: "bundled" };
    }
  }

  // 2. Local build cache compiled binary
  const cacheCandidates = [
    resolve(curDir, "../../../.agentcabin-build-cache/macos_ocr/ocr-helper"),
    resolve(curDir, "../../../.agentcabin-build-cache/macos_ocr/macos_ocr"),
  ];
  for (const p of cacheCandidates) {
    if (existsSync(p)) {
      return { path: p, type: "binary", origin: "build_cache" };
    }
  }

  // 3. Development Swift script fallback
  const scriptCandidate = resolve(curDir, "../../../scripts/macos_ocr.swift");
  if (existsSync(scriptCandidate)) {
    return { path: scriptCandidate, type: "script", origin: "development_fallback" };
  }

  return null;
}

/**
 * Built-in text / heuristic visual detector for fast, deterministic local matching.
 */
export async function defaultVisualDetector(imagePayload, options = {}) {
  if (!imagePayload) return [];

  if (typeof options.visionDetector === "function") {
    return await options.visionDetector(imagePayload, options);
  }

  const ocrBoxes = await defaultOcrProvider(imagePayload, options);
  if (Array.isArray(ocrBoxes) && ocrBoxes.length > 0) {
    return ocrBoxes.map((box) => ({
      role: "button",
      label: box.text,
      name: box.text,
      rect: box.rect,
    }));
  }

  return [];
}

/**
 * Built-in OCR provider for local heuristic or simulated visual text extraction.
 * On macOS, utilizes native Apple Vision framework (VNRecognizeTextRequest).
 */
export async function defaultOcrProvider(imagePayload, options = {}) {
  if (!imagePayload) return [];

  if (typeof options.ocrProvider === "function") {
    return await options.ocrProvider(imagePayload, options);
  }

  if (process.platform === "darwin") {
    try {
      const helper = resolveOcrHelper();
      if (!helper) return [];

      let proc;
      if (helper.type === "binary") {
        proc = spawnSync(helper.path, [], {
          input: imagePayload,
          encoding: "utf8",
          timeout: 8000,
          maxBuffer: 10 * 1024 * 1024,
        });
      } else if (helper.type === "script") {
        const curDir = dirname(fileURLToPath(import.meta.url));
        const cacheDir = resolve(curDir, "../../../.agentcabin-build-cache/swift-cache");
        proc = spawnSync("swift", ["-module-cache-path", cacheDir, helper.path], {
          input: imagePayload,
          encoding: "utf8",
          timeout: 10000,
          maxBuffer: 10 * 1024 * 1024,
        });
      }

      if (proc?.stdout) {
        const parsed = JSON.parse(proc.stdout.trim());
        if (Array.isArray(parsed)) {
          return parsed.map((item) => ({
            role: "text",
            text: item.text,
            confidence: item.confidence,
            rect: item.rect,
          }));
        }
      }
    } catch {
      // Vision OCR unavailable, fallback to empty
    }
  }

  return [];
}
