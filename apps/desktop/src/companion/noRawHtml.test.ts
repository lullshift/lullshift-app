// Model output is untrusted (decision 36): no Companion source may build HTML from strings.

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));
const SELF = "noRawHtml.test.ts";
const FORBIDDEN = [
  "dangerouslySetInnerHTML",
  ".innerHTML",
  "outerHTML",
  "insertAdjacentHTML",
  "document.write",
];

describe("Companion sources", () => {
  const files = readdirSync(dir).filter((f) => /\.tsx?$/.test(f) && f !== SELF);

  it("finds the sources", () => {
    expect(files).toContain("ReplyText.tsx");
  });

  it.each(FORBIDDEN)("never uses %s", (needle) => {
    const offenders = files.filter((f) => readFileSync(join(dir, f), "utf8").includes(needle));
    expect(offenders).toEqual([]);
  });
});
