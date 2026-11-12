// Unit tests for the platform record editor's text helpers (db-edit.ts).
import { describe, expect, it } from "vitest";
import { lineDiff, readProvenance, setProvenance } from "./db-edit";

/** setProvenance on a record that parses. */
function set(text: string, field: string, value: string): string {
  const t = setProvenance(text, field, value);
  if (t === null) throw new Error("not a record");
  return t;
}

describe("lineDiff", () => {
  it("marks changed lines and keeps the rest", () => {
    const d = lineDiff("a\nb\nc", "a\nB\nc\nd");
    expect(d).toEqual([
      { kind: "same", text: "a" },
      { kind: "removed", text: "b" },
      { kind: "added", text: "B" },
      { kind: "same", text: "c" },
      { kind: "added", text: "d" },
    ]);
    expect(lineDiff("x", "x")).toEqual([{ kind: "same", text: "x" }]);
  });
});

describe("provenance", () => {
  const rec = `{
      "id": "GEN-X", "name": "X", "category": "surface_combatant",
      "max_speed_kn": 20
    }`;

  it("adds a provenance object after the id, on one line, leaving the rest", () => {
    const t = set(rec, "source_citation", "Jane, 2024, p. 12");
    expect(t).toContain(`"id": "GEN-X", "provenance": {"source_citation": "Jane, 2024, p. 12"},`);
    expect(t).toContain(`"max_speed_kn": 20`);
    expect(readProvenance(t)).toEqual({ source_citation: "Jane, 2024, p. 12" });
  });

  it("edits the existing object and removes a cleared field", () => {
    let t = set(rec, "source_citation", "A");
    t = set(t, "confidence", "C");
    expect(readProvenance(t)).toEqual({ source_citation: "A", confidence: "C" });
    t = set(t, "source_citation", "");
    expect(readProvenance(t)).toEqual({ confidence: "C" });
    t = set(t, "confidence", "");
    expect((JSON.parse(t) as { provenance?: unknown }).provenance).toBeUndefined();
    expect((JSON.parse(t) as { max_speed_kn: number }).max_speed_kn).toBe(20);
  });

  it("keeps quotes inside values valid", () => {
    const t = set(rec, "source_citation", 'The "Book", 2nd ed.');
    expect(readProvenance(t)?.source_citation).toBe('The "Book", 2nd ed.');
  });

  it("refuses text that is not a record", () => {
    expect(setProvenance("{ not json", "confidence", "C")).toBeNull();
    expect(readProvenance("nope")).toBeNull();
  });
});
