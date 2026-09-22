import { describe, expect, it } from "vitest";
import { detectDelimiter, filterCsvRows, parseCsv, sortCsvRows } from "../csv-parser";

describe("csv-parser", () => {
  it("detects delimiters correctly", () => {
    expect(detectDelimiter("name,age,city")).toBe(",");
    expect(detectDelimiter("name\tage\tcity")).toBe("\t");
    expect(detectDelimiter("name;age;city")).toBe(";");
  });

  it("parses simple CSV correctly", () => {
    const csv = `Name,Age,Role\nAlice,30,Engineer\nBob,25,Designer`;
    const result = parseCsv(csv);
    expect(result.headers).toEqual(["Name", "Age", "Role"]);
    expect(result.rows).toHaveLength(2);
    expect(result.rows[0]).toEqual(["Alice", "30", "Engineer"]);
    expect(result.rows[1]).toEqual(["Bob", "25", "Designer"]);
    expect(result.totalRows).toBe(2);
    expect(result.totalCols).toBe(3);
  });

  it("handles quotes and commas within cells", () => {
    const csv = `Product,Description,Price\n"MacBook Pro","16-inch, M3 Max",3499\n"iPhone 15","Titanium, 256GB",1199`;
    const result = parseCsv(csv);
    expect(result.rows[0]).toEqual(["MacBook Pro", "16-inch, M3 Max", "3499"]);
    expect(result.rows[1]).toEqual(["iPhone 15", "Titanium, 256GB", "1199"]);
  });

  it("handles TSV parsing", () => {
    const tsv = "ID\tTitle\tStatus\n1\tTask 1\tDone\n2\tTask 2\tPending";
    const result = parseCsv(tsv);
    expect(result.delimiter).toBe("\t");
    expect(result.headers).toEqual(["ID", "Title", "Status"]);
    expect(result.rows).toHaveLength(2);
  });

  it("sorts rows numerically and alphabetically", () => {
    const rows = [
      ["Alice", "30"],
      ["Charlie", "100"],
      ["Bob", "5"],
    ];

    const sortedNumAsc = sortCsvRows(rows, 1, "asc");
    expect(sortedNumAsc.map((r) => r[0])).toEqual(["Bob", "Alice", "Charlie"]);

    const sortedNumDesc = sortCsvRows(rows, 1, "desc");
    expect(sortedNumDesc.map((r) => r[0])).toEqual(["Charlie", "Alice", "Bob"]);

    const sortedNameAsc = sortCsvRows(rows, 0, "asc");
    expect(sortedNameAsc.map((r) => r[0])).toEqual(["Alice", "Bob", "Charlie"]);
  });

  it("filters rows by keyword search", () => {
    const rows = [
      ["Alice", "London", "Admin"],
      ["Bob", "Paris", "User"],
      ["Charlie", "London", "User"],
    ];

    expect(filterCsvRows(rows, "london")).toHaveLength(2);
    expect(filterCsvRows(rows, "admin")).toHaveLength(1);
    expect(filterCsvRows(rows, "")).toHaveLength(3);
  });
});
