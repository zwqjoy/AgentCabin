import { Type } from "typebox";

const AskQuestionOption = Type.Object({
  label: Type.String({ description: "Visible option label." }),
  value: Type.Optional(Type.String({ description: "Stable returned value." })),
  description: Type.Optional(Type.String({ description: "Optional explanation." })),
});

const AskQuestionsParameters = Type.Object({
  title: Type.Optional(Type.String({ description: "Short title shown above the questions." })),
  questions: Type.Array(Type.Object({
    id: Type.Optional(Type.String({ description: "Stable answer key." })),
    header: Type.Optional(Type.String({ description: "Compact question label." })),
    question: Type.String({ description: "Question for the root user." }),
    options: Type.Optional(Type.Array(AskQuestionOption)),
    multi_select: Type.Optional(Type.Boolean()),
    allow_other: Type.Optional(Type.Boolean()),
    placeholder: Type.Optional(Type.String()),
  })),
});

const BATCH_ENVELOPE_KEY = "__piDeckBatchAsk";

function textResult(text, details = {}) {
  return { content: [{ type: "text", text }], details };
}

function formatAnswers(answers) {
  try {
    return JSON.stringify(answers ?? [], null, 2);
  } catch {
    return String(answers ?? "");
  }
}

function fail(message, details = {}) {
  return textResult(message, { ok: false, error: message, ...details });
}

function normalizeQuestions(raw) {
  if (!Array.isArray(raw) || raw.length === 0) return [];
  return raw.map((question, index) => {
    const options = Array.isArray(question?.options)
      ? question.options.flatMap((option) => {
          if (typeof option === "string") return [{ label: option, value: option }];
          if (!option || typeof option !== "object" || typeof option.label !== "string") return [];
          return [{
            label: option.label,
            value: typeof option.value === "string" ? option.value : option.label,
            ...(typeof option.description === "string" ? { description: option.description } : {}),
          }];
        })
      : [];
    return {
      id: typeof question?.id === "string" && question.id.trim() ? question.id.trim() : `q${index + 1}`,
      type: options.length > 0 ? "select" : "input",
      question: String(question?.question || "").trim(),
      ...(options.length > 0 ? { options } : {}),
      ...(options.length > 0 ? { allowOther: question?.allow_other !== false } : {}),
      ...(question?.multi_select === true ? { multiSelect: true } : {}),
      ...(typeof question?.placeholder === "string" ? { placeholder: question.placeholder } : {}),
    };
  });
}

function parseAnswers(raw) {
  try {
    const parsed = JSON.parse(String(raw || ""));
    if (!parsed || typeof parsed !== "object" || parsed.cancelled === true) return null;
    return Array.isArray(parsed.answers) ? parsed.answers : null;
  } catch {
    return null;
  }
}

/**
 * AgentCabin-owned Code Pi questionnaire. The UI envelope is shared with the
 * Work elicitation renderer, while answers stay local to the Code session.
 */
export default function agentCabinAskQuestions(pi) {
  // Pi child processes must not gain a root-only interaction capability.
  if (process.env.PI_SUBAGENT_CHILD === "1" || typeof pi?.registerTool !== "function") return;

  pi.registerTool({
    name: "ask_questions",
    label: "ask_questions",
    description: "Ask the root user one or more structured questions and wait for answers.",
    parameters: AskQuestionsParameters,
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const questions = normalizeQuestions(params?.questions);
      if (questions.length === 0) return fail("ask_questions requires at least one question.");
      if (questions.some((question) => !question.question)) {
        return fail("Every ask_questions item must have a non-empty question.");
      }
      if (!ctx?.hasUI || typeof ctx.ui?.input !== "function") {
        return fail("ask_questions requires an interactive root Code session.");
      }

      const envelope = JSON.stringify({
        [BATCH_ENVELOPE_KEY]: 1,
        ...(typeof params?.title === "string" && params.title.trim()
          ? { title: params.title.trim() }
          : {}),
        review: questions.length > 1,
        questions,
      });
      const raw = await ctx.ui.input(envelope, "");
      const answers = parseAnswers(raw);
      if (!answers) return fail("用户取消了问题，未继续执行。", { cancelled: true });

      return textResult(`用户已回答问题，具体答案如下：\n${formatAnswers(answers)}\n请基于这些答案继续执行。`, {
        ok: true,
        answers,
      });
    },
  });
}
