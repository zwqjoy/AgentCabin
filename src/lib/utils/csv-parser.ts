/**
 * Lightweight, robust CSV and TSV parser.
 * Handles commas, tabs, semicolons, quoted strings with escaped quotes,
 * multiline cells, and whitespace normalization.
 */

export interface ParsedCsv {
  headers: string[];
  rows: string[][];
  delimiter: string;
  totalRows: number;
  totalCols: number;
}

export function detectDelimiter(text: string): string {
  const firstLine = text.split(/\r\n|\n|\r/)[0] || "";
  const tabCount = (firstLine.match(/\t/g) || []).length;
  const semicolonCount = (firstLine.match(/;/g) || []).length;
  const commaCount = (firstLine.match(/,/g) || []).length;

  if (tabCount > commaCount && tabCount >= semicolonCount) return "\t";
  if (semicolonCount > commaCount && semicolonCount > tabCount) return ";";
  return ",";
}

export function parseCsv(text: string, customDelimiter?: string): ParsedCsv {
  if (!text || !text.trim()) {
    return { headers: [], rows: [], delimiter: ",", totalRows: 0, totalCols: 0 };
  }

  const delimiter = customDelimiter || detectDelimiter(text);
  const rows: string[][] = [];
  let currentRow: string[] = [];
  let currentCell = "";
  let inQuotes = false;
  let i = 0;
  const len = text.length;

  while (i < len) {
    const char = text[i];
    const nextChar = text[i + 1];

    if (inQuotes) {
      if (char === '"') {
        if (nextChar === '"') {
          currentCell += '"';
          i += 2;
          continue;
        } else {
          inQuotes = false;
          i++;
          continue;
        }
      } else {
        currentCell += char;
        i++;
        continue;
      }
    } else {
      if (char === '"') {
        inQuotes = true;
        i++;
        continue;
      }

      if (char === delimiter) {
        currentRow.push(currentCell.trim());
        currentCell = "";
        i++;
        continue;
      }

      if (char === "\r" || char === "\n") {
        if (char === "\r" && nextChar === "\n") {
          i += 2;
        } else {
          i++;
        }
        currentRow.push(currentCell.trim());
        currentCell = "";
        if (currentRow.some((c) => c.length > 0)) {
          rows.push(currentRow);
        }
        currentRow = [];
        continue;
      }

      currentCell += char;
      i++;
    }
  }

  if (currentCell.length > 0 || currentRow.length > 0) {
    currentRow.push(currentCell.trim());
    if (currentRow.some((c) => c.length > 0)) {
      rows.push(currentRow);
    }
  }

  if (rows.length === 0) {
    return { headers: [], rows: [], delimiter, totalRows: 0, totalCols: 0 };
  }

  const headers = rows[0];
  const dataRows = rows.slice(1);
  const maxCols = Math.max(headers.length, ...dataRows.map((r) => r.length));

  // Normalize column count
  const normalizedHeaders = [...headers];
  while (normalizedHeaders.length < maxCols) {
    normalizedHeaders.push(`Col ${normalizedHeaders.length + 1}`);
  }

  const normalizedRows = dataRows.map((row) => {
    const r = [...row];
    while (r.length < maxCols) {
      r.push("");
    }
    return r;
  });

  return {
    headers: normalizedHeaders,
    rows: normalizedRows,
    delimiter,
    totalRows: normalizedRows.length,
    totalCols: maxCols,
  };
}

export function sortCsvRows(
  rows: string[][],
  colIndex: number,
  direction: "asc" | "desc",
): string[][] {
  return [...rows].sort((a, b) => {
    const valA = a[colIndex] ?? "";
    const valB = b[colIndex] ?? "";

    const numA = Number(valA.replace(/,/g, ""));
    const numB = Number(valB.replace(/,/g, ""));

    let cmp = 0;
    if (!isNaN(numA) && !isNaN(numB) && valA.trim() !== "" && valB.trim() !== "") {
      cmp = numA - numB;
    } else {
      cmp = valA.localeCompare(valB, undefined, { numeric: true, sensitivity: "base" });
    }

    return direction === "asc" ? cmp : -cmp;
  });
}

export function filterCsvRows(rows: string[][], query: string): string[][] {
  const q = query.trim().toLowerCase();
  if (!q) return rows;
  return rows.filter((row) => row.some((cell) => cell.toLowerCase().includes(q)));
}
