// @vitest-environment jsdom
// Security review F2: a streaming reply must not be parsed, searched for links or highlighted
// again for parts that did not change. The parsing and highlighting functions are wrapped to count
// their calls while the real implementations do the work.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.resolve(null),
  Channel: class {},
}));

const counts = { parse: 0, links: 0, highlight: 0 };

vi.mock("./replyFormat", async (importOriginal) => {
  const real = await importOriginal<typeof import("./replyFormat")>();
  return {
    ...real,
    parseReply: (text: string) => {
      counts.parse++;
      return real.parseReply(text);
    },
    findLinks: (text: string) => {
      counts.links++;
      return real.findLinks(text);
    },
  };
});

vi.mock("./highlight", async (importOriginal) => {
  const real = await importOriginal<typeof import("./highlight")>();
  return {
    ...real,
    highlightToReact: (lang: string | null, code: string) => {
      counts.highlight++;
      return real.highlightToReact(lang, code);
    },
  };
});

import { ReplyText } from "./ReplyText";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  counts.parse = 0;
  counts.links = 0;
  counts.highlight = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

async function render(text: string, streaming: boolean) {
  await act(async () => {
    root.render(<ReplyText text={text} streaming={streaming} />);
  });
}

describe("ReplyText while streaming", () => {
  it("highlights a finished block once and leaves earlier lines alone as text arrives", async () => {
    const head = "Intro https://a.example/x\n```js\nconst a = 1;\n```\n";
    await render(head, true);
    expect(counts).toEqual({ parse: 1, links: 1, highlight: 1 });

    // Ten more chunks of text after the block: one parse per new text, links only for the
    // line that changed, and the finished code block is not highlighted again.
    let text = head;
    for (let i = 0; i < 10; i++) {
      text += "x";
      await render(text, true);
    }
    expect(counts.parse).toBe(11);
    expect(counts.highlight).toBe(1);
    expect(counts.links).toBe(1 + 10);

    // Re-rendering with the same text parses nothing.
    await render(text, true);
    expect(counts.parse).toBe(11);
    expect(host.querySelector('span[class^="hljs-"]')).not.toBeNull();
  });

  it("shows an unfinished block plain and highlights it once when it closes", async () => {
    let text = "```js\nconst a";
    await render(text, true);
    for (let i = 0; i < 5; i++) {
      text += " = 1";
      await render(text, true);
    }
    expect(counts.highlight).toBe(0);
    expect(host.textContent).toContain("Still writing…");
    expect(host.querySelector("code")?.textContent).toBe("const a = 1 = 1 = 1 = 1 = 1");

    await render(`${text}\n\`\`\``, true);
    expect(counts.highlight).toBe(1);
    expect(host.textContent).not.toContain("Still writing…");

    // A reply that ends without closing its fence is highlighted when streaming stops.
    counts.highlight = 0;
    await render("```js\nlet b = 2", true);
    expect(counts.highlight).toBe(0);
    await render("```js\nlet b = 2", false);
    expect(counts.highlight).toBe(1);
  });
});
