import { renderToStaticMarkup } from "react-dom/server";
import { beforeAll, describe, expect, it } from "vitest";
import {
  highlighterReady,
  highlightToReact,
  languageLabel,
  loadHighlighter,
  MAX_HIGHLIGHT_CHARS,
} from "./highlight";

const render = (lang: string | null, code: string) =>
  renderToStaticMarkup(<code>{highlightToReact(lang, code)}</code>);

const SAMPLES: Record<string, string> = {
  javascript: "const a = 1; function f() { return 'x'; }",
  typescript: "const a: number = 1; interface X { y: string }",
  xml: '<div class="a">text</div>',
  css: ".a { color: red; }",
  json: '{"a": 1, "b": true}',
  yaml: "key: value\nlist:\n  - 1",
  ini: "[section]\nkey = 1",
  sql: "SELECT * FROM t WHERE a = 1;",
  python: "def f(x):\n    return 1",
  rust: 'fn main() { let x = 1; println!("a"); }',
  go: "package main\nfunc main() { var x int = 1 }",
  java: "public class A { private int x = 1; }",
  csharp: "public class A { private int x = 1; }",
  c: "int main(void) { return 0; }",
  cpp: "#include <vector>\nclass A { public: int x; };",
  bash: 'if [ -f x ]; then echo "a"; fi',
  shell: "$ ls -la\nfile",
  powershell: 'Get-ChildItem -Path "x" | ForEach-Object { $_ }',
  markdown: "# Title\n\n**bold** and `code`",
  dockerfile: "FROM node:20\nRUN npm install",
  diff: "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new",
};

const ALIASES: Record<string, string> = {
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  ts: "typescript",
  tsx: "typescript",
  html: "xml",
  htm: "xml",
  svg: "xml",
  toml: "ini",
  py: "python",
  rs: "rust",
  golang: "go",
  cs: "csharp",
  "c#": "csharp",
  h: "c",
  hpp: "cpp",
  cc: "cpp",
  "c++": "cpp",
  sh: "bash",
  zsh: "bash",
  console: "shell",
  ps: "powershell",
  ps1: "powershell",
  pwsh: "powershell",
  yml: "yaml",
  md: "markdown",
  docker: "dockerfile",
  patch: "diff",
};

describe("loading the grammars on demand", () => {
  it("shows plain code until the grammars have loaded, then highlights", async () => {
    // Runs first in this file, before any test has loaded the engine module.
    expect(highlighterReady()).toBe(false);
    expect(highlightToReact("ts", "const a = 1;")).toBe("const a = 1;");
    await loadHighlighter();
    expect(highlighterReady()).toBe(true);
    expect(renderToStaticMarkup(<code>{highlightToReact("ts", "const a = 1;")}</code>)).toContain(
      '<span class="hljs-',
    );
  }, 30_000); // the first import of the grammars is slow under a cold test runner

  it("shares one load between callers", async () => {
    const [a, b] = await Promise.all([loadHighlighter(), loadHighlighter()]);
    expect(a).toBe(b);
  });
});

describe("highlightToReact", () => {
  beforeAll(async () => {
    await loadHighlighter();
  }, 30_000);

  it.each(Object.entries(SAMPLES))("highlights %s", (lang, code) => {
    const html = render(lang, code);
    expect(html).toContain('<span class="hljs-');
    expect(languageLabel(lang)).not.toBe("Code");
  });

  it.each(Object.entries(ALIASES))("alias %s behaves as %s", (alias, canonical) => {
    const sample = SAMPLES[canonical] ?? "";
    expect(render(alias, sample)).toBe(render(canonical, sample));
    expect(languageLabel(alias)).toBe(languageLabel(canonical));
  });

  it("returns plain code for an unknown or missing language", () => {
    expect(highlightToReact("klingon", "a < b")).toBe("a < b");
    expect(highlightToReact(null, "a")).toBe("a");
    expect(languageLabel("klingon")).toBe("Code");
    expect(languageLabel(null)).toBe("Code");
    expect(languageLabel("__proto__")).toBe("Code");
    expect(highlightToReact("constructor", "x")).toBe("x");
  });

  it("labels languages readably", () => {
    expect(languageLabel("ts")).toBe("TypeScript");
    expect(languageLabel("sh")).toBe("Bash");
    expect(languageLabel("console")).toBe("Shell");
  });

  it("escapes HTML in code", () => {
    for (const lang of ["html", "javascript", null]) {
      const html = render(lang, "<script>alert(1)</script>");
      expect(html).not.toContain("<script");
      expect(html).toContain("&lt;");
      expect(html).toContain("script");
    }
    expect(render("html", "<script>alert(1)</script>")).toContain("&lt;");
  });

  it("skips highlighting for huge input", () => {
    const big = "a".repeat(MAX_HIGHLIGHT_CHARS + 1);
    expect(highlightToReact("js", big)).toBe(big);
    const manyLines = "x\n".repeat(3500);
    expect(highlightToReact("js", manyLines)).toBe(manyLines);
  });
});
