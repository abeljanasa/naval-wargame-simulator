// Saving files from the webview: the app has no filesystem permission, so files are handed to
// the browser as downloads (the user picks where they go).

/** Hands `data` to the browser as a file download called `name`. */
export function download(name: string, data: BlobPart, type: string): void {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  // Give the download a moment to start before the URL is released.
  setTimeout(() => URL.revokeObjectURL(url), 1_000);
}

/** A file-name-safe stem from a scenario name ("Phase 2 — Surface action" → "phase-2-surface-action"). */
export function fileStem(name: string): string {
  const stem = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return stem || "exercise";
}
