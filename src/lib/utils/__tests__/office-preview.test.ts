import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  convertToHtml: vi.fn(),
  xlsxLoad: vi.fn(),
  eachSheet: vi.fn(),
  zipLoadAsync: vi.fn(),
}));

vi.mock("mammoth", () => ({
  default: { convertToHtml: mocks.convertToHtml },
}));

vi.mock("exceljs", () => ({
  default: {
    Workbook: function Workbook() {
      return { xlsx: { load: mocks.xlsxLoad }, eachSheet: mocks.eachSheet };
    },
  },
}));

vi.mock("jszip", () => ({
  default: { loadAsync: mocks.zipLoadAsync },
}));

import {
  extractPptxParagraphs,
  formatOfficeCellValue,
  parseOfficePreview,
} from "$lib/utils/office-preview";

function encode(value: string): string {
  return btoa(value);
}

describe("office preview parsing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("extracts text from DOCX through Mammoth", async () => {
    mocks.convertToHtml.mockResolvedValueOnce({
      value: "<h1>Quarterly report</h1><p>Revenue grew.</p>",
      messages: [{ message: "An image was skipped" }],
    });

    const preview = await parseOfficePreview(encode("docx-bytes"), "docx");

    expect(preview).toEqual({
      kind: "docx",
      html: "<h1>Quarterly report</h1><p>Revenue grew.</p>",
      messages: ["An image was skipped"],
    });
    expect(mocks.convertToHtml).toHaveBeenCalledOnce();
  });

  it("limits XLSX preview rows and formats formulas and rich text", async () => {
    mocks.xlsxLoad.mockResolvedValueOnce(undefined);
    mocks.eachSheet.mockImplementationOnce((callback: (sheet: unknown) => void) => {
      callback({
        name: "Summary",
        actualRowCount: 3,
        actualColumnCount: 3,
        getRow: (rowNumber: number) => ({
          getCell: (columnNumber: number) => ({
            value:
              rowNumber === 1
                ? ["Name", "Total", "Notes"][columnNumber - 1]
                : rowNumber === 2 && columnNumber === 2
                  ? { formula: "SUM(B3:B3)", result: 42 }
                  : rowNumber === 2 && columnNumber === 3
                    ? { richText: [{ text: "approved" }] }
                    : rowNumber === 2
                      ? "Alice"
                      : rowNumber === 3 && columnNumber === 1
                        ? "Bob"
                        : rowNumber === 3 && columnNumber === 2
                          ? 18
                          : "",
          }),
        }),
      });
    });

    const preview = await parseOfficePreview(encode("xlsx-bytes"), "xlsx");

    expect(preview.kind).toBe("xlsx");
    if (preview.kind !== "xlsx") return;
    expect(preview.sheets[0]).toMatchObject({
      name: "Summary",
      rowCount: 3,
      columnCount: 3,
      truncated: false,
      rows: [
        ["Name", "Total", "Notes"],
        ["Alice", "42", "approved"],
        ["Bob", "18", ""],
      ],
    });
  });

  it("extracts ordered slide paragraphs from PPTX XML", async () => {
    const xml = `
      <p:sld><a:p><a:r><a:t>Q3 outlook</a:t></a:r></a:p>
      <a:p><a:r><a:t>Revenue &amp; margin</a:t></a:r></a:p></p:sld>
    `;
    expect(extractPptxParagraphs(xml)).toEqual(["Q3 outlook", "Revenue & margin"]);

    mocks.zipLoadAsync.mockResolvedValueOnce({
      files: {
        "ppt/slides/slide2.xml": {},
        "ppt/slides/slide1.xml": {},
      },
      file: (name: string) => ({
        async: async () =>
          name.endsWith("slide1.xml")
            ? "<a:p><a:r><a:t>First</a:t></a:r></a:p>"
            : "<a:p><a:r><a:t>Second</a:t></a:r></a:p>",
      }),
    });

    const preview = await parseOfficePreview(encode("pptx-bytes"), "pptx");
    expect(preview).toEqual({
      kind: "pptx",
      slides: [
        { number: 1, paragraphs: ["First"] },
        { number: 2, paragraphs: ["Second"] },
      ],
    });
  });

  it("renders common Excel cell value shapes predictably", () => {
    expect(formatOfficeCellValue({ formula: "A1+B1", result: 3 })).toBe("3");
    expect(formatOfficeCellValue({ formula: "A1+B1" })).toBe("=A1+B1");
    expect(formatOfficeCellValue({ hyperlink: "https://example.com", text: "Open" })).toBe("Open");
    expect(formatOfficeCellValue({ error: "DIV/0!" })).toBe("#DIV/0!");
  });
});
