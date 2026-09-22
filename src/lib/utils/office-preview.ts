import ExcelJS from "exceljs";
import JSZip from "jszip";
import mammoth from "mammoth";

/** Keep Office parsing bounded so a malformed or unexpectedly large file cannot freeze the UI. */
export const MAX_OFFICE_PREVIEW_BYTES = 25 * 1024 * 1024;
export const MAX_XLSX_PREVIEW_ROWS = 500;
export const MAX_XLSX_PREVIEW_COLUMNS = 50;
export const OFFICE_PREVIEW_TOO_LARGE = "office_preview_too_large";

export interface DocxOfficePreview {
  kind: "docx";
  html: string;
  messages: string[];
}

export interface XlsxSheetPreview {
  name: string;
  rows: string[][];
  rowCount: number;
  columnCount: number;
  truncated: boolean;
}

export interface XlsxOfficePreview {
  kind: "xlsx";
  sheets: XlsxSheetPreview[];
}

export interface PptxSlidePreview {
  number: number;
  paragraphs: string[];
}

export interface PptxOfficePreview {
  kind: "pptx";
  slides: PptxSlidePreview[];
}

export type OfficePreview = DocxOfficePreview | XlsxOfficePreview | PptxOfficePreview;

/** Decode the Rust base64 IPC payload without relying on Node's Buffer API. */
export function base64ToBytes(base64: string): Uint8Array {
  const comma = base64.indexOf(",");
  const payload = (comma >= 0 ? base64.slice(comma + 1) : base64).replace(/\s/g, "");
  if (!payload) return new Uint8Array();

  const binary = atob(payload);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function assertPreviewSize(bytes: Uint8Array): void {
  if (bytes.byteLength > MAX_OFFICE_PREVIEW_BYTES) {
    throw new Error(OFFICE_PREVIEW_TOO_LARGE);
  }
}

function arrayBufferFor(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

/** Convert an ExcelJS cell value into a safe, human-readable string. */
export function formatOfficeCellValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (value instanceof Date) return value.toISOString();
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }

  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (Array.isArray(record.richText)) {
      return record.richText
        .map((part) =>
          part && typeof part === "object" ? String((part as { text?: unknown }).text ?? "") : "",
        )
        .join("");
    }
    if (typeof record.hyperlink === "string") {
      return String(record.text ?? record.hyperlink);
    }
    if (typeof record.formula === "string") {
      return record.result === null || record.result === undefined
        ? `=${record.formula}`
        : formatOfficeCellValue(record.result);
    }
    if (typeof record.error === "string") return `#${record.error}`;
    if (typeof record.text === "string") return record.text;
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }

  return String(value);
}

async function parseDocx(bytes: Uint8Array): Promise<DocxOfficePreview> {
  // Mammoth exposes different input names in its Node and browser builds.
  // Tauri runs the browser build, while Vitest resolves the Node build; keep
  // both paths explicit so previews behave the same in desktop and tests.
  const runtimeBuffer = (
    globalThis as typeof globalThis & {
      Buffer?: { from: (value: Uint8Array) => unknown };
    }
  ).Buffer;
  const input = runtimeBuffer
    ? ({ buffer: runtimeBuffer.from(bytes) } as Parameters<typeof mammoth.convertToHtml>[0])
    : ({ arrayBuffer: arrayBufferFor(bytes) } as Parameters<typeof mammoth.convertToHtml>[0]);
  const result = await mammoth.convertToHtml(input);
  const html = result.value.trim();
  if (!html) throw new Error("Document appears to be empty");

  return {
    kind: "docx",
    html,
    messages: (result.messages ?? []).map((message) => String(message.message ?? message)),
  };
}

async function parseXlsx(bytes: Uint8Array): Promise<XlsxOfficePreview> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(arrayBufferFor(bytes));

  const sheets: XlsxSheetPreview[] = [];
  workbook.eachSheet((worksheet) => {
    const rowCount = Math.max(worksheet.actualRowCount || 0, 0);
    const columnCount = Math.max(worksheet.actualColumnCount || 0, 0);
    const displayedRows = Math.min(rowCount, MAX_XLSX_PREVIEW_ROWS);
    const displayedColumns = Math.min(columnCount, MAX_XLSX_PREVIEW_COLUMNS);
    const rows: string[][] = [];

    for (let rowNumber = 1; rowNumber <= displayedRows; rowNumber += 1) {
      const row = worksheet.getRow(rowNumber);
      const values: string[] = [];
      for (let columnNumber = 1; columnNumber <= displayedColumns; columnNumber += 1) {
        values.push(formatOfficeCellValue(row.getCell(columnNumber).value));
      }
      rows.push(values);
    }

    sheets.push({
      name: worksheet.name,
      rows,
      rowCount,
      columnCount,
      truncated: rowCount > displayedRows || columnCount > displayedColumns,
    });
  });

  if (sheets.length === 0) throw new Error("Spreadsheet appears to be empty");
  return { kind: "xlsx", sheets };
}

function decodeXmlEntities(value: string): string {
  return value
    .replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1")
    .replace(/&#x([0-9a-f]+);/gi, (_match, code: string) => {
      try {
        return String.fromCodePoint(parseInt(code, 16));
      } catch {
        return "";
      }
    })
    .replace(/&#(\d+);/g, (_match, code: string) => {
      try {
        return String.fromCodePoint(Number(code));
      } catch {
        return "";
      }
    })
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

/** Extract visible text from one PresentationML slide without executing any document content. */
export function extractPptxParagraphs(xml: string): string[] {
  const paragraphs: string[] = [];
  const paragraphPattern = /<a:p(?:\s[^>]*)?>([\s\S]*?)<\/a:p>/gi;
  let paragraphMatch: RegExpExecArray | null;

  while ((paragraphMatch = paragraphPattern.exec(xml)) !== null) {
    const textParts = [...paragraphMatch[1].matchAll(/<a:t(?:\s[^>]*)?>([\s\S]*?)<\/a:t>/gi)].map(
      (match) => decodeXmlEntities(match[1]),
    );
    const text = textParts.join("").replace(/\s+/g, " ").trim();
    if (text) paragraphs.push(text);
  }

  if (paragraphs.length > 0) return paragraphs;
  return [...xml.matchAll(/<a:t(?:\s[^>]*)?>([\s\S]*?)<\/a:t>/gi)]
    .map((match) => decodeXmlEntities(match[1]).replace(/\s+/g, " ").trim())
    .filter(Boolean);
}

async function parsePptx(bytes: Uint8Array): Promise<PptxOfficePreview> {
  const archive = await JSZip.loadAsync(arrayBufferFor(bytes));
  const slideNames = Object.keys(archive.files)
    .filter((name) => /^ppt\/slides\/slide\d+\.xml$/i.test(name))
    .sort((left, right) => {
      const leftNumber = Number(left.match(/slide(\d+)\.xml$/i)?.[1] ?? 0);
      const rightNumber = Number(right.match(/slide(\d+)\.xml$/i)?.[1] ?? 0);
      return leftNumber - rightNumber;
    });

  if (slideNames.length === 0) throw new Error("Presentation appears to be empty");

  const slides = await Promise.all(
    slideNames.map(async (name, index) => {
      const entry = archive.file(name);
      if (!entry) throw new Error(`Unable to read slide ${index + 1}`);
      return {
        number: index + 1,
        paragraphs: extractPptxParagraphs(await entry.async("string")),
      };
    }),
  );

  return { kind: "pptx", slides };
}

export async function parseOfficePreview(
  base64: string,
  extension: string,
): Promise<OfficePreview> {
  const bytes = base64ToBytes(base64);
  assertPreviewSize(bytes);
  const normalizedExtension = extension.toLowerCase();

  if (normalizedExtension === "docx") return parseDocx(bytes);
  if (normalizedExtension === "xlsx") return parseXlsx(bytes);
  if (normalizedExtension === "pptx") return parsePptx(bytes);
  throw new Error(`Unsupported Office preview format: .${normalizedExtension}`);
}
