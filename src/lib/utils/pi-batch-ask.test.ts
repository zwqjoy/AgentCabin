import { describe, expect, it } from "vitest";
import { encodePiBatchAskResponse, parsePiBatchAskEnvelope } from "./pi-batch-ask";

describe("Pi batch ask envelope", () => {
  it("parses the ask_question batch payload instead of treating it as one text field", () => {
    const message = JSON.stringify({
      __piDeckBatchAsk: 1,
      title: "Release preferences",
      review: true,
      questions: [
        {
          id: "fixed_point",
          type: "input",
          question: "What is the fixed point to diff HEAD against?",
          placeholder: "e.g. origin/dev, main, HEAD~5",
        },
        {
          id: "issue_tracker",
          type: "select",
          question: "How do you want to handle spec sourcing?",
          options: [
            {
              label: "Skip issue tracker, use docs/specs",
              value: "Skip issue tracker, use docs/specs",
              description: "Proceed using docs/ as the spec source",
            },
          ],
          allowOther: true,
        },
      ],
    });

    const envelope = parsePiBatchAskEnvelope(message);

    expect(envelope).not.toBeNull();
    expect(envelope?.title).toBe("Release preferences");
    expect(envelope?.review).toBe(true);
    expect(envelope?.questions).toHaveLength(2);
    expect(envelope?.questions[0]).toMatchObject({ id: "fixed_point", type: "input" });
    expect(envelope?.questions[1].options?.[0]).toMatchObject({
      label: "Skip issue tracker, use docs/specs",
      value: "Skip issue tracker, use docs/specs",
    });
  });

  it("encodes answers as the value expected by ctx.ui.input", () => {
    expect(
      encodePiBatchAskResponse([
        { id: "fixed_point", type: "input", value: "HEAD~5" },
        { id: "issue_tracker", type: "select", value: "Skip issue tracker, use docs/specs" },
      ]),
    ).toBe(
      JSON.stringify({
        answers: [
          { id: "fixed_point", type: "input", value: "HEAD~5" },
          { id: "issue_tracker", type: "select", value: "Skip issue tracker, use docs/specs" },
        ],
      }),
    );
  });

  it("preserves multi-select questions for the shared elicitation UI", () => {
    const envelope = parsePiBatchAskEnvelope(
      JSON.stringify({
        __piDeckBatchAsk: 1,
        review: false,
        questions: [
          {
            id: "formats",
            type: "select",
            question: "Which formats should be supported?",
            multi_select: true,
            options: [
              { label: "CSV", value: "csv" },
              { label: "XLSX", value: "xlsx" },
            ],
          },
        ],
      }),
    );

    expect(envelope?.questions[0]).toMatchObject({
      id: "formats",
      multiSelect: true,
    });
  });

  it("preserves question header chips when present", () => {
    const envelope = parsePiBatchAskEnvelope(
      JSON.stringify({
        __piDeckBatchAsk: 1,
        review: false,
        questions: [
          {
            id: "doc_type",
            header: "文档类型",
            type: "select",
            question: "您希望我为您创建什么类型的文档？",
            options: [{ label: "Word 文档", value: "docx" }],
          },
        ],
      }),
    );

    expect(envelope?.questions[0]).toMatchObject({
      id: "doc_type",
      header: "文档类型",
    });
  });
});
