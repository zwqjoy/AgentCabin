export const PI_BATCH_ASK_ENVELOPE_KEY = "__piDeckBatchAsk";

export type PiBatchQuestionType = "select" | "confirm" | "input" | "editor";

export interface PiBatchOption {
  label: string;
  value: string;
  description?: string;
}

export interface PiBatchQuestion {
  id: string;
  type: PiBatchQuestionType;
  question: string;
  header?: string;
  options?: PiBatchOption[];
  multiSelect?: boolean;
  allowOther?: boolean;
  placeholder?: string;
  prefill?: string;
}

export interface PiBatchAskEnvelope {
  title?: string;
  review: boolean;
  questions: PiBatchQuestion[];
}

export interface PiBatchAnswer {
  id: string;
  type: string;
  value: string | string[] | boolean | null;
  label?: string;
  wasCustom?: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringOrUndefined(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function normalizeQuestionType(value: unknown): PiBatchQuestionType {
  return value === "select" || value === "confirm" || value === "editor" ? value : "input";
}

function normalizeOptions(value: unknown): PiBatchOption[] | undefined {
  if (!Array.isArray(value)) return undefined;

  const options = value.flatMap((option): PiBatchOption[] => {
    if (typeof option === "string") return [{ label: option, value: option }];
    if (!isRecord(option) || typeof option.label !== "string") return [];
    return [
      {
        label: option.label,
        value: typeof option.value === "string" ? option.value : option.label,
        description: stringOrUndefined(option.description),
      },
    ];
  });

  return options.length > 0 ? options : undefined;
}

/** Parse the JSON title used by pi-deck-ask-question's batch input envelope. */
export function parsePiBatchAskEnvelope(message: string | undefined): PiBatchAskEnvelope | null {
  if (!message?.trim()) return null;

  let raw: unknown;
  try {
    raw = JSON.parse(message.trim());
  } catch {
    return null;
  }
  if (!isRecord(raw) || raw[PI_BATCH_ASK_ENVELOPE_KEY] !== 1 || !Array.isArray(raw.questions)) {
    return null;
  }

  const questions = raw.questions.flatMap((item, index): PiBatchQuestion[] => {
    if (!isRecord(item)) return [];
    const id = typeof item.id === "string" && item.id.trim() ? item.id : `q${index + 1}`;
    const type = normalizeQuestionType(item.type);
    const question = typeof item.question === "string" ? item.question : "";
    const options = type === "select" ? normalizeOptions(item.options) : undefined;
    return [
      {
        id,
        type,
        question,
        header: stringOrUndefined(item.header),
        options,
        multiSelect: item.multiSelect === true || item.multi_select === true,
        allowOther: type === "select" ? item.allowOther !== false : undefined,
        placeholder: stringOrUndefined(item.placeholder),
        prefill: stringOrUndefined(item.prefill),
      },
    ];
  });

  return questions.length > 0
    ? {
        ...(stringOrUndefined(raw.title) ? { title: stringOrUndefined(raw.title) } : {}),
        review: raw.review === true,
        questions,
      }
    : null;
}

/** Encode the tabbed form result as the string returned by ctx.ui.input. */
export function encodePiBatchAskResponse(answers: PiBatchAnswer[], cancelled = false): string {
  return JSON.stringify({ ...(cancelled ? { cancelled: true } : {}), answers });
}
