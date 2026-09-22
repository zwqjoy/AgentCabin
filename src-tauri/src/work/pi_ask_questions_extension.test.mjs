import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

const TYPEBOX_STUB = `
export const Type = {
  Array: () => ({}),
  Boolean: () => ({}),
  Object: () => ({}),
  Optional: (value) => value,
  String: () => ({}),
};
`;

async function loadExtension(tempRoot) {
  const extensionDir = path.join(tempRoot, "extensions");
  const typeboxDir = path.join(tempRoot, "node_modules", "typebox");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.writeFileSync(path.join(tempRoot, "package.json"), '{"type":"module"}\n', "utf8");
  fs.writeFileSync(
    path.join(tempRoot, "node_modules", "typebox", "package.json"),
    '{"type":"module","exports":"./index.js"}\n',
    "utf8",
  );
  fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB, "utf8");
  fs.copyFileSync(
    new URL("./pi_ask_questions_extension.mjs", import.meta.url),
    path.join(extensionDir, "agentcabin-ask-questions.mjs"),
  );

  const module = await import(
    `${pathToFileURL(path.join(extensionDir, "agentcabin-ask-questions.mjs"))}?test=${Date.now()}-${Math.random()}`,
  );
  return module.default;
}

test("Code built-in ask_questions renders the shared questionnaire envelope and returns answers", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-code-ask-questions-"));
  const previousChild = process.env.PI_SUBAGENT_CHILD;
  delete process.env.PI_SUBAGENT_CHILD;

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({ registerTool(tool) { tools.set(tool.name, tool); } });

    assert.equal(tools.size, 1);
    const askQuestions = tools.get("ask_questions");
    assert.ok(askQuestions);

    let envelope;
    const result = await askQuestions.execute(
      "question-call-1",
      {
        title: "Choose a direction",
        questions: [
          {
            id: "direction",
            question: "Which direction should we take?",
            options: [
              { label: "Fast", value: "fast", description: "Ship the smallest useful change." },
              "Thorough",
            ],
          },
        ],
      },
      undefined,
      undefined,
      {
        hasUI: true,
        ui: {
          async input(message, placeholder) {
            envelope = JSON.parse(message);
            assert.equal(placeholder, "");
            return JSON.stringify({
              answers: [{ id: "direction", type: "select", value: "fast" }],
            });
          },
        },
      },
    );

    assert.equal(envelope.__piDeckBatchAsk, 1);
    assert.equal(envelope.title, "Choose a direction");
    assert.equal(envelope.review, false);
    assert.deepEqual(envelope.questions, [
      {
        id: "direction",
        type: "select",
        question: "Which direction should we take?",
        options: [
          { label: "Fast", value: "fast", description: "Ship the smallest useful change." },
          { label: "Thorough", value: "Thorough" },
        ],
        allowOther: true,
      },
    ]);
    assert.equal(result.details.ok, true);
    assert.deepEqual(result.details.answers, [
      { id: "direction", type: "select", value: "fast" },
    ]);
    assert.match(result.content[0].text, /fast/);
  } finally {
    if (previousChild === undefined) delete process.env.PI_SUBAGENT_CHILD;
    else process.env.PI_SUBAGENT_CHILD = previousChild;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Code built-in ask_questions is not registered in a Pi child process", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-code-child-ask-questions-"));
  const previousChild = process.env.PI_SUBAGENT_CHILD;
  process.env.PI_SUBAGENT_CHILD = "1";

  try {
    const extension = await loadExtension(temp);
    const tools = [];
    extension({ registerTool(tool) { tools.push(tool); } });
    assert.deepEqual(tools, []);
  } finally {
    if (previousChild === undefined) delete process.env.PI_SUBAGENT_CHILD;
    else process.env.PI_SUBAGENT_CHILD = previousChild;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
