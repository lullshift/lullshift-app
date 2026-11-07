import { describe, expect, it } from "vitest";
import { displayUrl, findLinks, hostOf, isLocalHost, parseReply } from "./replyFormat";

const links = (text: string) =>
  findLinks(text)
    .filter((p) => p.kind === "link")
    .map((p) => (p.kind === "link" ? p.url : ""));

describe("parseReply", () => {
  it("returns plain text as one segment", () => {
    expect(parseReply("hello\nworld")).toEqual([{ kind: "text", text: "hello\nworld" }]);
    expect(parseReply("")).toEqual([]);
  });

  it("splits a closed fence with a language", () => {
    expect(parseReply("before\n```TypeScript\nconst a = 1;\n```\nafter")).toEqual([
      { kind: "text", text: "before" },
      { kind: "code", lang: "typescript", code: "const a = 1;", closed: true },
      { kind: "text", text: "after" },
    ]);
  });

  it("keeps an unclosed fence as an open code block (streaming)", () => {
    expect(parseReply("see:\n```rust\nfn main() {")).toEqual([
      { kind: "text", text: "see:" },
      { kind: "code", lang: "rust", code: "fn main() {", closed: false },
    ]);
    expect(parseReply("```")).toEqual([{ kind: "code", lang: null, code: "", closed: false }]);
  });

  it("does not close on a shorter or different fence, and keeps inner backticks", () => {
    const r = parseReply("````md\n```js\nx\n```\n````");
    expect(r).toEqual([{ kind: "code", lang: "md", code: "```js\nx\n```", closed: true }]);
    const t = parseReply("~~~\n```\nstill code\n~~~");
    expect(t).toEqual([{ kind: "code", lang: null, code: "```\nstill code", closed: true }]);
  });

  it("closes on a longer fence", () => {
    expect(parseReply("```\na\n`````")).toEqual([
      { kind: "code", lang: null, code: "a", closed: true },
    ]);
  });

  it("sanitizes and truncates the language", () => {
    const [seg] = parseReply('```C#<script>"x"\ncode\n```');
    expect(seg).toMatchObject({ kind: "code", lang: "c#script" + "x" });
    const [long] = parseReply("```" + "a".repeat(50) + "\nx\n```");
    expect(long).toMatchObject({ lang: "a".repeat(20) });
    const [none] = parseReply("```<>!\nx\n```");
    expect(none).toMatchObject({ lang: null });
  });

  it("allows up to three leading spaces, not four", () => {
    expect(parseReply("   ```js\nx\n   ```")[0]).toMatchObject({ kind: "code", closed: true });
    expect(parseReply("    ```js\nx")).toEqual([{ kind: "text", text: "    ```js\nx" }]);
  });

  it("does not treat inline triple backticks as a fence", () => {
    expect(parseReply("use ```x``` inline")).toEqual([
      { kind: "text", text: "use ```x``` inline" },
    ]);
    expect(parseReply("```a`b\nx")).toEqual([{ kind: "text", text: "```a`b\nx" }]);
  });

  it("handles CRLF", () => {
    expect(parseReply("a\r\n```\r\nb\r\n```\r\n")[1]).toEqual({
      kind: "code",
      lang: null,
      code: "b",
      closed: true,
    });
  });

  it("stays fast on very long input", () => {
    const big = "line ```\n".repeat(200_000) + "```js\n" + "x\n".repeat(200_000);
    const t = Date.now();
    parseReply(big);
    expect(Date.now() - t).toBeLessThan(2000);
  });
});

describe("findLinks", () => {
  it("links http and https, any case", () => {
    expect(links("go to http://a.com and HTTPS://B.com/x ok")).toEqual([
      "http://a.com",
      "HTTPS://B.com/x",
    ]);
  });

  it("trims trailing punctuation", () => {
    expect(links("see https://a.com/x.")).toEqual(["https://a.com/x"]);
    expect(links("(https://a.com/x), and 'https://b.com'!")).toEqual([
      "https://a.com/x",
      "https://b.com",
    ]);
    expect(links("https://a.com/x?!;:")).toEqual(["https://a.com/x"]);
  });

  it("keeps a balanced closing paren", () => {
    expect(links("https://en.example.org/wiki/Foo_(bar)")).toEqual([
      "https://en.example.org/wiki/Foo_(bar)",
    ]);
    expect(links("(see https://en.example.org/wiki/Foo_(bar))")).toEqual([
      "https://en.example.org/wiki/Foo_(bar)",
    ]);
  });

  it("splits text and link parts in order", () => {
    expect(findLinks("a https://x.io b")).toEqual([
      { kind: "text", text: "a " },
      { kind: "link", url: "https://x.io", text: "https://x.io" },
      { kind: "text", text: " b" },
    ]);
  });

  it.each([
    "javascript:alert(1)",
    "JaVaScRiPt:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "file:///C:/x",
    "vbscript:msgbox(1)",
    "mailto:a@b.com",
    "ftp://a.com/x",
    "https://",
    "http://",
    "https:// spaced",
    "xhttps://a.com",
  ])("never links %s", (s) => {
    expect(links(s)).toEqual([]);
  });

  it("does not link a javascript URL that hides inside an http-looking string", () => {
    expect(links("http://a.com/javascript:alert(1)")).toEqual(["http://a.com/javascript:alert(1)"]);
    expect(links("[x](javascript:alert(1))")).toEqual([]);
  });

  it("does not link URLs with credentials", () => {
    expect(links("https://user:pw@evil.com/")).toEqual([]);
    expect(links("https://google.com@evil.com/")).toEqual([]);
  });

  it("does not link over-long URLs", () => {
    expect(links("https://a.com/" + "x".repeat(2100))).toEqual([]);
  });
});

describe("displayUrl", () => {
  it("shows IDN hosts as punycode and escapes spaces", () => {
    expect(displayUrl("https://ex\u00e4mple.com/a b")).toBe("https://xn--exmple-cua.com/a%20b");
    expect(hostOf("https://ex\u00e4mple.com/a")).toBe("xn--exmple-cua.com");
  });

  it("falls back for unparseable input", () => {
    expect(displayUrl("not a url")).toBe("not a url");
    expect(hostOf("not a url")).toBe("");
  });

  it("is not linked inside code", () => {
    const segs = parseReply("```\nhttps://a.com\n```");
    expect(segs).toEqual([{ kind: "code", lang: null, code: "https://a.com", closed: true }]);
  });
});

describe("findLinks on hostile input (security review F1)", () => {
  /** Best of three runs, in milliseconds, so a busy machine doesn't fail the test. */
  function fastest(run: () => void): number {
    let best = Infinity;
    for (let i = 0; i < 3; i++) {
      const t = performance.now();
      run();
      best = Math.min(best, performance.now() - t);
    }
    return best;
  }

  const SHAPES: [string, string][] = [
    ["a URL followed by 100k closing brackets", "https://a.b/" + ")".repeat(100_000)],
    ["closing brackets after a short URL", "see https://a.b/x" + ")]}".repeat(33_000)],
    [
      "a URL of 50k opening then 50k closing brackets",
      "https://a.b/" + "(".repeat(50_000) + ")".repeat(50_000),
    ],
    ["many URLs on one line", "https://a.b/c) ".repeat(7_000)],
    ["URLs glued together", "https://".repeat(12_000)],
    ["a trailing punctuation mix", "https://a.b/" + ".,;:!?)]}'\"".repeat(9_000)],
  ];

  for (const [name, text] of SHAPES) {
    it(`handles ${name} quickly`, () => {
      expect(text.length).toBeGreaterThan(90_000);
      expect(fastest(() => findLinks(text))).toBeLessThan(50);
    });
  }

  it("skips candidates too long to be a link and still trims short ones the same way", () => {
    expect(links("https://a.b/" + ")".repeat(100_000))).toEqual([]);
    expect(links("(see https://a.b/x_(y))).")).toEqual(["https://a.b/x_(y)"]);
    expect(links("https://a.b/x)]}")).toEqual(["https://a.b/x"]);
    const long = "https://a.b/" + "x".repeat(2048 - 12);
    expect(long).toHaveLength(2048);
    expect(links(`${long}).`)).toEqual([long]);
    expect(links(`${long}y`)).toEqual([]);
  });

  it("links every URL when there are many", () => {
    expect(links("https://a.b/c) ".repeat(7_000))).toHaveLength(7_000);
  });
});

describe("isLocalHost (security review F6)", () => {
  it("flags this computer and the local network", () => {
    for (const url of [
      "http://localhost:3000/",
      "http://LOCALHOST./x",
      "http://app.localhost/",
      "http://127.0.0.1/",
      "http://127.8.9.10/",
      "http://2130706433/", // 127.0.0.1 written as a number
      "http://0x7f.1/",
      "http://0.0.0.0:8080/",
      "http://10.1.2.3/",
      "http://172.16.0.1/",
      "http://172.31.255.255/",
      "http://192.168.1.1/admin",
      "http://169.254.169.254/latest/meta-data",
      "http://100.64.0.1/",
      "http://[::1]:8080/",
      "http://[::ffff:127.0.0.1]/",
      "http://[::ffff:192.168.0.1]/",
      "http://[fd00::1]/",
      "http://[fe80::1]/",
      "http://printer.local/",
      "http://nas.lan/",
      "http://svc.internal/",
      "http://box.home.arpa/",
      "http://router/",
    ]) {
      expect(isLocalHost(displayUrl(url)), url).toBe(true);
    }
  });

  it("does not flag public hosts", () => {
    for (const url of [
      "https://example.com/",
      "https://localhost.example.com/",
      "https://local.example/",
      "http://8.8.8.8/",
      "http://172.32.0.1/",
      "http://192.169.0.1/",
      "http://[2001:db8::1]/",
      "http://[::ffff:8.8.8.8]/",
      "not a url",
    ]) {
      expect(isLocalHost(displayUrl(url)), url).toBe(false);
    }
  });
});
