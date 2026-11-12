// Text helpers for the platform record editor (DbRecordEditor.svelte). The editor works on the
// record's authored JSON text, so a saved file differs from the original only where the user
// edited it (ADR-0004: data reviews are ordinary diffs).

export interface DiffLine {
  kind: "same" | "added" | "removed";
  text: string;
}

/** A line diff (longest common subsequence) from `a` to `b`. */
export function lineDiff(a: string, b: string): DiffLine[] {
  const x = a.split("\n");
  const y = b.split("\n");
  const n = x.length;
  const m = y.length;
  // lcs[i * (m + 1) + j]: length of the LCS of x[i..] and y[j..].
  const lcs = new Array<number>((n + 1) * (m + 1)).fill(0);
  const at = (i: number, j: number) => lcs[i * (m + 1) + j] ?? 0;
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * (m + 1) + j] =
        x[i] === y[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      out.push({ kind: "same", text: x[i] ?? "" });
      i++;
      j++;
    } else if (at(i + 1, j) >= at(i, j + 1)) {
      out.push({ kind: "removed", text: x[i++] ?? "" });
    } else {
      out.push({ kind: "added", text: y[j++] ?? "" });
    }
  }
  while (i < n) out.push({ kind: "removed", text: x[i++] ?? "" });
  while (j < m) out.push({ kind: "added", text: y[j++] ?? "" });
  return out;
}

/** The record's own provenance fields (they override the file's defaults), if the text parses. */
export function readProvenance(text: string): Record<string, string> | null {
  try {
    const v = JSON.parse(text) as { provenance?: Record<string, unknown> };
    const p = v.provenance ?? {};
    return Object.fromEntries(
      Object.entries(p).filter((e): e is [string, string] => typeof e[1] === "string"),
    );
  } catch {
    return null;
  }
}

const PROVENANCE = /"provenance"\s*:\s*\{[^{}]*\}/;
const ID_FIELD = /("id"\s*:\s*"[^"]*"\s*,)/;

/** `{"a": "x", "b": "y"}`: one line, in the sources' spacing. */
function inline(o: Record<string, string>): string {
  return `{${Object.entries(o)
    .map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`)
    .join(", ")}}`;
}

/**
 * Sets one provenance field on the record text, editing only the `provenance` object (written on
 * one line), or adding it after the id. An empty value removes the field, so the file's default
 * applies again. Returns `null` if the text is not a record.
 */
export function setProvenance(text: string, field: string, value: string): string | null {
  const current = readProvenance(text);
  if (current === null) return null;
  const others = Object.entries(current).filter(([k]) => k !== field);
  const next: Record<string, string> = Object.fromEntries(
    value.trim() ? [...others, [field, value]] : others,
  );
  const body = Object.keys(next).length ? `"provenance": ${inline(next)}` : "";
  if (PROVENANCE.test(text)) {
    if (body) return text.replace(PROVENANCE, body);
    // Remove the object and the comma that joined it to the next field.
    return text.replace(/\s*"provenance"\s*:\s*\{[^{}]*\}\s*,?/, "");
  }
  if (!body) return text;
  return ID_FIELD.test(text) ? text.replace(ID_FIELD, `$1 ${body},`) : null;
}
