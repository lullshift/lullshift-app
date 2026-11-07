// Model output is untrusted (decision 36): no Companion source, and no shared UI component that
// shows it or notices about it, may build HTML from strings. Scans Companion and ui/ sources.

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));
const DIRS = [dir, join(dir, "..", "ui")];
const FORBIDDEN = [
  "dangerouslySetInnerHTML",
  ".innerHTML",
  "outerHTML",
  "insertAdjacentHTML",
  "document.write",
];

const isSource = (f: string) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f);

describe("Companion and ui sources", () => {
  const files = DIRS.flatMap((d) =>
    readdirSync(d)
      .filter(isSource)
      .map((f) => join(d, f)),
  );

  it("finds the sources", () => {
    const names = files.map((f) => f.replace(/^.*[\\/]/, ""));
    expect(names).toContain("ReplyText.tsx");
    expect(names).toContain("Toasts.tsx");
  });

  it.each(FORBIDDEN)("never uses %s", (needle) => {
    const offenders = files.filter((f) => readFileSync(f, "utf8").includes(needle));
    expect(offenders).toEqual([]);
  });
});
