import ExcelJS from "exceljs";
import JSZip from "jszip";
import {
  base64ToBytes,
  MAX_OFFICE_PREVIEW_BYTES,
  type OfficePreview,
  parseOfficePreview,
} from "./office-preview";

export type OfficeDocumentExtension = "docx" | "xlsx" | "pptx";

export interface XlsxCellEdit {
  sheet: string;
  row: number;
  column: number;
  value: string;
}

export interface OfficeTextEdit {
  find: string;
  replace: string;
  replaceAll?: boolean;
}

export type OfficeEditOperation =
  | { kind: "xlsx_cells"; edits: XlsxCellEdit[] }
  | { kind: "text"; edit: OfficeTextEdit };

export const MAX_OFFICE_EDITOR_ROWS = 100;
export const MAX_OFFICE_EDITOR_COLUMNS = 20;

function arrayBufferFor(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function bytesFor(value: ArrayBuffer | Uint8Array): Uint8Array {
  return value instanceof Uint8Array ? value : new Uint8Array(value);
}

export function bytesToBase64(value: ArrayBuffer | Uint8Array): string {
  const bytes = bytesFor(value);
  let encoded = "";
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    encoded += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return btoa(encoded);
}

function assertOfficeSize(bytes: Uint8Array): void {
  if (bytes.byteLength > MAX_OFFICE_PREVIEW_BYTES) {
    throw new Error("office_document_too_large");
  }
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

function decodeXmlEntities(value: string): string {
  return value
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

function replaceXmlNodeText(
  xml: string,
  nodeName: "w:t" | "a:t",
  edit: OfficeTextEdit,
): { xml: string; matches: number } {
  const pattern = new RegExp(`(<${nodeName}\\b[^>]*>)([\\s\\S]*?)(</${nodeName}>)`, "gi");
  let matches = 0;
  const replaced = xml.replace(pattern, (_match, open: string, body: string, close: string) => {
    if (!edit.replaceAll && matches > 0) return `${open}${body}${close}`;
    const decoded = decodeXmlEntities(body);
    if (!decoded.includes(edit.find)) return `${open}${body}${close}`;
    const next = edit.replaceAll
      ? decoded.split(edit.find).join(edit.replace)
      : decoded.replace(edit.find, edit.replace);
    matches += edit.replaceAll ? decoded.split(edit.find).length - 1 : 1;
    return `${open}${escapeXml(next)}${close}`;
  });
  return { xml: replaced, matches };
}

function docxContentTypes(): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
  <Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>`;
}

function createDocxPackage(title: string): JSZip {
  const zip = new JSZip();
  const safeTitle = escapeXml(title || "新建文档");
  zip.file("[Content_Types].xml", docxContentTypes());
  zip.file(
    "_rels/.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>`,
  );
  zip.file(
    "word/document.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t xml:space="preserve">${safeTitle}</w:t></w:r></w:p>
    <w:p><w:r><w:t xml:space="preserve">在这里开始编辑。</w:t></w:r></w:p>
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/>
    </w:sectPr>
  </w:body>
</w:document>`,
  );
  zip.file(
    "word/_rels/document.xml.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>`,
  );
  zip.file(
    "word/styles.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
  <w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
</w:styles>`,
  );
  zip.file(
    "docProps/core.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>${safeTitle}</dc:title><dc:creator>AgentCabin</dc:creator></cp:coreProperties>`,
  );
  zip.file(
    "docProps/app.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>AgentCabin</Application></Properties>`,
  );
  return zip;
}

function pptxContentTypes(): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
  <Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
  <Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>`;
}

function createPptxPackage(title: string): JSZip {
  const zip = new JSZip();
  const safeTitle = escapeXml(title || "新建演示文稿");
  zip.file("[Content_Types].xml", pptxContentTypes());
  zip.file(
    "_rels/.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>`,
  );
  zip.file(
    "ppt/presentation.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
  <p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst>
  <p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>
  <p:notesSz cx="6858000" cy="9144000"/>
  <p:defaultTextStyle><a:defPPr/><a:lvl1pPr marL="0" algn="l" rtl="0" eaLn="0" latinLn="0" hanging="0"><a:defRPr lang="zh-CN"/></a:lvl1pPr></p:defaultTextStyle>
</p:presentation>`,
  );
  zip.file(
    "ppt/_rels/presentation.xml.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>`,
  );
  zip.file(
    "ppt/slides/slide1.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
    <p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" dirty="0"/><a:t>${safeTitle}</a:t></a:r><a:endParaRPr lang="zh-CN"/></a:p></p:txBody></p:sp>
  </p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>`,
  );
  zip.file(
    "ppt/slides/_rels/slide1.xml.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>`,
  );
  zip.file(
    "ppt/slideLayouts/slideLayout1.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="title" preserve="1"><p:cSld name="Title Slide"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>`,
  );
  zip.file(
    "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>`,
  );
  zip.file(
    "ppt/slideMasters/slideMaster1.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld name="Master"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles/><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/></p:sldMaster>`,
  );
  zip.file(
    "ppt/slideMasters/_rels/slideMaster1.xml.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>`,
  );
  zip.file(
    "ppt/theme/theme1.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="AgentCabin"><a:themeElements><a:clrScheme name="AgentCabin"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F9FAFB"/></a:lt2><a:accent1><a:srgbClr val="2563EB"/></a:accent1><a:accent2><a:srgbClr val="0EA5E9"/></a:accent2><a:accent3><a:srgbClr val="10B981"/></a:accent3><a:accent4><a:srgbClr val="F59E0B"/></a:accent4><a:accent5><a:srgbClr val="8B5CF6"/></a:accent5><a:accent6><a:srgbClr val="EC4899"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="AgentCabin"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="AgentCabin"><a:fillStyleLst/><a:lnStyleLst/><a:effectStyleLst/><a:bgFillStyleLst/></a:fmtScheme></a:themeElements></a:theme>`,
  );
  zip.file(
    "docProps/core.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>${safeTitle}</dc:title><dc:creator>AgentCabin</dc:creator></cp:coreProperties>`,
  );
  zip.file(
    "docProps/app.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>AgentCabin</Application><Slides>1</Slides></Properties>`,
  );
  return zip;
}

export async function createOfficeDocument(
  extension: OfficeDocumentExtension,
  title: string,
): Promise<string> {
  const normalizedTitle = title.trim() || (extension === "pptx" ? "新建演示文稿" : "新建文档");
  if (extension === "xlsx") {
    const workbook = new ExcelJS.Workbook();
    const worksheet = workbook.addWorksheet("Sheet1");
    worksheet.getCell("A1").value = normalizedTitle;
    worksheet.getCell("A2").value = "在这里开始编辑";
    worksheet.getColumn(1).width = Math.max(normalizedTitle.length + 4, 18);
    return bytesToBase64(bytesFor(await workbook.xlsx.writeBuffer()));
  }

  const zip =
    extension === "docx" ? createDocxPackage(normalizedTitle) : createPptxPackage(normalizedTitle);
  return bytesToBase64(await zip.generateAsync({ type: "uint8array", compression: "DEFLATE" }));
}

async function applyXlsxEdits(bytes: Uint8Array, edits: XlsxCellEdit[]): Promise<string> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(arrayBufferFor(bytes));
  for (const edit of edits) {
    if (!Number.isInteger(edit.row) || edit.row < 1 || edit.row > MAX_OFFICE_EDITOR_ROWS) {
      throw new Error("office_editor_row_limit");
    }
    if (
      !Number.isInteger(edit.column) ||
      edit.column < 1 ||
      edit.column > MAX_OFFICE_EDITOR_COLUMNS
    ) {
      throw new Error("office_editor_column_limit");
    }
    const worksheet = workbook.getWorksheet(edit.sheet);
    if (!worksheet) throw new Error(`找不到工作表：${edit.sheet}`);
    const value = edit.value;
    const cell = worksheet.getCell(edit.row, edit.column);
    if (!value) {
      cell.value = null;
    } else if (value.startsWith("=")) {
      cell.value = { formula: value.slice(1) };
    } else {
      cell.value = value;
    }
  }
  return bytesToBase64(bytesFor(await workbook.xlsx.writeBuffer()));
}

async function applyTextEdit(
  bytes: Uint8Array,
  extension: "docx" | "pptx",
  edit: OfficeTextEdit,
): Promise<string> {
  if (!edit.find.trim()) throw new Error("office_editor_find_required");
  const zip = await JSZip.loadAsync(arrayBufferFor(bytes));
  const nodeName = extension === "docx" ? "w:t" : "a:t";
  const fileNames = Object.keys(zip.files).filter((name) =>
    extension === "docx"
      ? /^word\/.*\.xml$/i.test(name)
      : /^ppt\/slides\/slide\d+\.xml$/i.test(name),
  );
  let matches = 0;
  for (const fileName of fileNames) {
    const file = zip.file(fileName);
    if (!file) continue;
    const original = await file.async("string");
    const replaced = replaceXmlNodeText(original, nodeName, edit);
    if (replaced.matches > 0) {
      zip.file(fileName, replaced.xml);
      matches += replaced.matches;
      if (!edit.replaceAll) break;
    }
  }
  if (matches === 0) throw new Error("office_editor_text_not_found");
  return bytesToBase64(await zip.generateAsync({ type: "uint8array", compression: "DEFLATE" }));
}

export async function editOfficeDocument(
  base64: string,
  extension: OfficeDocumentExtension,
  operation: OfficeEditOperation,
): Promise<string> {
  const bytes = base64ToBytes(base64);
  assertOfficeSize(bytes);
  if (operation.kind === "xlsx_cells") {
    if (extension !== "xlsx") throw new Error("office_editor_format_mismatch");
    return applyXlsxEdits(bytes, operation.edits);
  }
  if (extension === "xlsx") throw new Error("office_editor_format_mismatch");
  return applyTextEdit(bytes, extension, operation.edit);
}

export async function parseEditedOfficePreview(
  base64: string,
  extension: OfficeDocumentExtension,
): Promise<OfficePreview> {
  return parseOfficePreview(base64, extension);
}
