// @vitest-environment jsdom
// Mounted reply rendering with the Tauri bridge mocked (decision 36): untrusted model output
// becomes text, code blocks and confirm-first links, never elements.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: unknown];
const calls: Call[] = [];
let answer: (command: string, args: Record<string, unknown>) => unknown = () => null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    try {
      return Promise.resolve(answer(command, args));
    } catch (err) {
      return Promise.reject(err);
    }
  },
  Channel: class {},
}));

import { loadHighlighter } from "./highlight";
import { ReplyText } from "./ReplyText";

// The grammars are a separate chunk; load them up front so blocks highlight on first render.
beforeAll(async () => {
  await loadHighlighter();
}, 30_000);

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no modal dialog support: toggle the open attribute instead.
HTMLDialogElement.prototype.showModal = function showModal() {
  this.setAttribute("open", "");
};
HTMLDialogElement.prototype.close = function close() {
  this.removeAttribute("open");
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  calls.length = 0;
  answer = () => null;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

async function render(text: string, streaming = false) {
  await act(async () => {
    root.render(<ReplyText text={text} streaming={streaming} />);
  });
}

async function settle() {
  for (let i = 0; i < 5; i++) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

function button(label: string): HTMLButtonElement {
  const b = [...host.querySelectorAll("button")].find((x) => x.textContent?.trim() === label);
  if (!b) throw new Error(`no button "${label}" in: ${host.textContent}`);
  return b;
}

async function click(el: HTMLElement) {
  await act(async () => {
    el.click();
  });
  await settle();
}

describe("ReplyText", () => {
  it("renders text lines and a code block", async () => {
    await render("Hello\n\nworld\n```ts\nconst a: number = 1;\n```");
    expect(host.querySelectorAll(".msg-line")).toHaveLength(3);
    const group = host.querySelector('[role="group"]');
    expect(group?.getAttribute("aria-label")).toBe("TypeScript code");
    const pre = host.querySelector("pre");
    expect(pre?.getAttribute("tabindex")).toBe("0");
    expect(pre?.textContent).toBe("const a: number = 1;");
    expect(pre?.querySelector('code[class~="hljs"][class~="language-ts"]')).not.toBeNull();
    expect(pre?.querySelector('span[class^="hljs-"]')).not.toBeNull();
  });

  it("never makes elements from untrusted HTML", async () => {
    await render("<img src=x onerror=alert(1)> <b>x</b>\n```html\n<script>alert(1)</script>\n```");
    expect(host.querySelector("img")).toBeNull();
    expect(host.querySelector("script")).toBeNull();
    expect(host.querySelector("b")).toBeNull();
    expect(host.textContent).toContain("<img src=x onerror=alert(1)>");
    expect(host.querySelector("pre")?.textContent).toBe("<script>alert(1)</script>");
  });

  it("shows a still-writing note only while streaming an unclosed block", async () => {
    await render("```js\nlet a", true);
    expect(host.textContent).toContain("Still writing…");
    await render("```js\nlet a", false);
    expect(host.textContent).not.toContain("Still writing…");
    expect(host.querySelector("pre")?.textContent).toBe("let a");
  });

  it("does not make buttons from non-http links", async () => {
    await render("javascript:alert(1) and data:text/html,x and file:///C:/x and mailto:a@b.c");
    expect(host.querySelector("button.reply-link")).toBeNull();
    expect(host.querySelector("a")).toBeNull();
  });

  it("does not detect links inside code", async () => {
    await render("```\nhttps://example.com\n```");
    expect(host.querySelector("button.reply-link")).toBeNull();
  });

  it("confirms before opening a link and sends the punycode URL", async () => {
    await render("See https://ex\u00e4mple.com/a b.");
    const link = host.querySelector<HTMLButtonElement>("button.reply-link");
    expect(link?.textContent).toBe("https://ex\u00e4mple.com/a");
    expect(calls).toEqual([]);

    await click(link as HTMLButtonElement);
    const dialog = host.querySelector("dialog");
    expect(dialog?.hasAttribute("open")).toBe(true);
    expect(dialog?.textContent).toContain("Open this link?");
    expect(dialog?.querySelector("code")?.textContent).toBe("https://xn--exmple-cua.com/a");
    expect(dialog?.textContent).toContain("xn--exmple-cua.com");
    expect(dialog?.textContent).toContain("doesn't check where links lead");

    await click(button("Cancel"));
    expect(calls).toEqual([]);
    expect(dialog?.hasAttribute("open")).toBe(false);

    await click(link as HTMLButtonElement);
    await click(button("Open in browser"));
    expect(calls).toEqual([["companion_open_link", { url: "https://xn--exmple-cua.com/a" }]]);
    expect(dialog?.hasAttribute("open")).toBe(false);
  });

  it("warns when a link points at this computer or the local network", async () => {
    await render("Try http://192.168.1.1/admin and https://example.com/");
    const [local, remote] = [...host.querySelectorAll<HTMLButtonElement>("button.reply-link")];
    const dialog = host.querySelector("dialog");

    await click(local!);
    expect(dialog?.querySelector(".reply-link-local")?.textContent).toContain(
      "on your computer or local network",
    );
    await click(button("Cancel"));

    await click(remote!);
    expect(dialog?.querySelector(".reply-link-local")).toBeNull();
    await click(button("Cancel"));
    expect(calls).toEqual([]);
  });

  it("shows an error and stays open when opening fails", async () => {
    answer = () => {
      throw { kind: "offline", message: "no" };
    };
    await render("https://example.com/x");
    await click(host.querySelector("button.reply-link") as HTMLButtonElement);
    await click(button("Open in browser"));
    const dialog = host.querySelector("dialog");
    expect(dialog?.hasAttribute("open")).toBe(true);
    expect(dialog?.querySelector('[role="alert"]')).not.toBeNull();
  });

  it("copies the exact code and announces it", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    await render("```ts\nconst a = 1;\nconst b = 2;\n```");
    const copy = host.querySelector<HTMLButtonElement>("button.reply-copy");
    expect(copy?.getAttribute("aria-label")).toBe("Copy TypeScript code");
    await click(copy as HTMLButtonElement);
    expect(writeText).toHaveBeenCalledWith("const a = 1;\nconst b = 2;");
    expect(host.querySelector('[role="status"]')?.textContent).toBe("Copied");
    expect(copy?.textContent).toBe("Copied");
  });

  it("resets the Copy button after two seconds", async () => {
    vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
    await render("```\nx\n```");
    await click(host.querySelector("button.reply-copy") as HTMLButtonElement);
    vi.useFakeTimers();
    await click(host.querySelector("button.reply-copy") as HTMLButtonElement);
    await act(async () => {
      vi.advanceTimersByTime(2100);
    });
    expect(host.querySelector("button.reply-copy")?.textContent).toBe("Copy");
  });

  it("falls back to a hidden textarea when the clipboard is missing", async () => {
    vi.stubGlobal("navigator", {});
    const exec = vi.fn().mockReturnValue(true);
    (document as unknown as { execCommand: unknown }).execCommand = exec;
    await render("```\nplain\n```");
    await click(host.querySelector("button.reply-copy") as HTMLButtonElement);
    expect(exec).toHaveBeenCalledWith("copy");
    expect(document.querySelector("textarea")).toBeNull();
    expect(host.querySelector('[role="status"]')?.textContent).toBe("Copied");
  });

  it("says so when copying fails", async () => {
    vi.stubGlobal("navigator", {});
    (document as unknown as { execCommand: unknown }).execCommand = () => false;
    await render("```\nplain\n```");
    await click(host.querySelector("button.reply-copy") as HTMLButtonElement);
    expect(host.querySelector('[role="status"]')?.textContent).toBe("Couldn't copy");
  });
});
