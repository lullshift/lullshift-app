// @vitest-environment jsdom
// The checklist rows: the text edits in place (focus, Enter, blur, Esc, empty), the checkbox only
// toggles, reordering works from the keyboard, and Delete asks first.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buttonIn, click, key, polyfillDialog, settle, typeInto } from "../companion/testDom";
import type { SessionItem } from "./api";
import { ItemRows } from "./ItemRows";

polyfillDialog();

const item = (id: number, text: string, done = false): SessionItem => ({
  id,
  kind: "checklist",
  category: null,
  text,
  done,
  position: id,
  createdUtc: "2026-11-05T12:00:00Z",
});

let host: HTMLDivElement;
let root: Root;
const onUpdate = vi.fn(async () => true);
const onDelete = vi.fn(async () => true);
const onMove = vi.fn(async () => true);

beforeEach(async () => {
  onUpdate.mockClear();
  onDelete.mockClear();
  onMove.mockClear();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(
      <ItemRows
        kind="checklist"
        items={[item(1, "Refresh path has a test"), item(2, "Sign-out clears the profile")]}
        onUpdate={onUpdate}
        onDelete={onDelete}
        onMove={onMove}
      />,
    );
  });
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const textButton = (n: number) => host.querySelectorAll<HTMLButtonElement>(".item-text")[n]!;
const editor = () => host.querySelector<HTMLInputElement>("input.item-edit");
const focus = (el: HTMLElement) => act(async () => el.focus());

describe("inline edit", () => {
  it("has no Edit button; focusing the text opens an input holding it", async () => {
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "Edit")).toBe(false);
    expect(editor()).toBeNull();
    await focus(textButton(0));
    expect(editor()?.value).toBe("Refresh path has a test");
  });

  it("clicking the text opens the editor too", async () => {
    await click(textButton(1));
    expect(editor()?.value).toBe("Sign-out clears the profile");
  });

  it("Enter saves the trimmed text and closes the editor", async () => {
    await focus(textButton(0));
    await typeInto(editor()!, "  Refresh path is tested  ");
    await key(editor()!, "Enter");
    expect(onUpdate).toHaveBeenCalledWith(1, { text: "Refresh path is tested" });
    expect(editor()).toBeNull();
  });

  it("leaving the field saves, once", async () => {
    await focus(textButton(0));
    await typeInto(editor()!, "Changed");
    await act(async () => editor()!.blur());
    await settle();
    expect(onUpdate).toHaveBeenCalledTimes(1);
    expect(onUpdate).toHaveBeenCalledWith(1, { text: "Changed" });
  });

  it("Esc cancels without saving", async () => {
    await focus(textButton(0));
    await typeInto(editor()!, "Never saved");
    const notCancelled = await key(editor()!, "Escape");
    expect(notCancelled).toBe(false);
    expect(onUpdate).not.toHaveBeenCalled();
    expect(editor()).toBeNull();
    expect(textButton(0).textContent).toBe("Refresh path has a test");
  });

  it("empty text on save is a cancel: nothing is saved or deleted", async () => {
    await focus(textButton(0));
    await typeInto(editor()!, "   ");
    await key(editor()!, "Enter");
    expect(onUpdate).not.toHaveBeenCalled();
    expect(onDelete).not.toHaveBeenCalled();
    expect(editor()).toBeNull();
    expect(textButton(0).textContent).toBe("Refresh path has a test");
  });

  it("returns focus to the row, not the text, so the editor does not reopen", async () => {
    await focus(textButton(0));
    await key(editor()!, "Escape");
    await settle();
    expect(editor()).toBeNull();
    expect(document.activeElement).toBe(host.querySelector('[data-item-id="1"]'));
  });
});

describe("checkbox", () => {
  it("toggles done and does not open the editor", async () => {
    const box = host.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    await click(box);
    expect(onUpdate).toHaveBeenCalledWith(1, { done: true });
    expect(editor()).toBeNull();
  });
});

describe("reordering", () => {
  it("keeps icon-only move buttons with names and the shortcut in the tooltip", () => {
    const down = host.querySelector<HTMLButtonElement>('[data-item-id="1"] [data-move="down"]')!;
    expect(down.getAttribute("aria-label")).toBe("Move down: Refresh path has a test");
    expect(down.textContent).toBe("");
    expect(down.title).toContain("Alt+Down");
    expect(
      host.querySelector<HTMLButtonElement>('[data-item-id="1"] [data-move="up"]')!.disabled,
    ).toBe(true);
  });

  it("Alt+Down and Alt+Up on a row move the item", async () => {
    const row1 = host.querySelector('[data-item-id="1"]')!;
    const row2 = host.querySelector('[data-item-id="2"]')!;
    const alt = (el: Element, k: string) =>
      act(async () => {
        el.dispatchEvent(
          new KeyboardEvent("keydown", { key: k, altKey: true, bubbles: true, cancelable: true }),
        );
      });
    await alt(row1, "ArrowDown");
    expect(onMove).toHaveBeenLastCalledWith(1, 2);
    await alt(row2, "ArrowUp");
    expect(onMove).toHaveBeenLastCalledWith(2, 1);
    // The first item cannot move up.
    onMove.mockClear();
    await alt(row1, "ArrowUp");
    expect(onMove).not.toHaveBeenCalled();
  });
});

describe("delete", () => {
  const trash = () =>
    host.querySelector<HTMLButtonElement>('[aria-label="Delete Refresh path has a test"]')!;

  it("is an icon-only button named for the item", () => {
    expect(trash().textContent).toBe("");
    expect(trash().querySelector("svg")).not.toBeNull();
  });

  it("asks first: nothing is deleted until confirmed", async () => {
    await click(trash());
    expect(onDelete).not.toHaveBeenCalled();
    const dialog = host.querySelector("dialog")!;
    expect(dialog.hasAttribute("open")).toBe(true);
    expect(dialog.textContent).toContain("Refresh path has a test");
  });

  it("Cancel keeps the item", async () => {
    await click(trash());
    await click(buttonIn(host.querySelector("dialog")!, "Cancel"));
    expect(onDelete).not.toHaveBeenCalled();
    expect(host.querySelector("dialog")!.hasAttribute("open")).toBe(false);
  });

  it("Delete in the dialog deletes that item", async () => {
    await click(trash());
    await click(buttonIn(host.querySelector("dialog")!, "Delete"));
    expect(onDelete).toHaveBeenCalledWith(1);
    expect(host.querySelector("dialog")!.hasAttribute("open")).toBe(false);
  });
});
