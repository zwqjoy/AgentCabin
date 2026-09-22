import { describe, expect, it } from "vitest";
import { createOfficeDocument, editOfficeDocument, parseEditedOfficePreview } from "../office-edit";

describe("office-edit", () => {
  it("creates an XLSX workbook and edits a cell", async () => {
    const created = await createOfficeDocument("xlsx", "Sales plan");
    const edited = await editOfficeDocument(created, "xlsx", {
      kind: "xlsx_cells",
      edits: [{ sheet: "Sheet1", row: 2, column: 1, value: "Q3" }],
    });

    const preview = await parseEditedOfficePreview(edited, "xlsx");
    expect(preview.kind).toBe("xlsx");
    if (preview.kind !== "xlsx") return;
    expect(preview.sheets[0]?.rows[1]?.[0]).toBe("Q3");
  });

  it("creates a DOCX package and replaces text without executing content", async () => {
    const created = await createOfficeDocument("docx", "Draft report");
    const edited = await editOfficeDocument(created, "docx", {
      kind: "text",
      edit: { find: "Draft report", replace: "Final report" },
    });

    const preview = await parseEditedOfficePreview(edited, "docx");
    expect(preview.kind).toBe("docx");
    if (preview.kind !== "docx") return;
    expect(preview.html).toContain("Final report");
    expect(preview.html).not.toContain("Draft report");
  });

  it("creates a PPTX package and replaces slide text", async () => {
    const created = await createOfficeDocument("pptx", "Quarterly review");
    const edited = await editOfficeDocument(created, "pptx", {
      kind: "text",
      edit: { find: "Quarterly review", replace: "Annual review" },
    });

    const preview = await parseEditedOfficePreview(edited, "pptx");
    expect(preview.kind).toBe("pptx");
    if (preview.kind !== "pptx") return;
    expect(preview.slides[0]?.paragraphs).toContain("Annual review");
  });

  it("rejects text editing an XLSX document", async () => {
    const created = await createOfficeDocument("xlsx", "Sheet");
    await expect(
      editOfficeDocument(created, "xlsx", {
        kind: "text",
        edit: { find: "Sheet", replace: "Table" },
      }),
    ).rejects.toThrow("office_editor_format_mismatch");
  });
});
