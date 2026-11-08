// @vitest-environment jsdom
// The checklist rows: the text edits in place (focus, Enter, blur, Esc, empty), the checkbox only
// toggles, reordering works from the keyboard, and Delete asks first.

import { act, useRef, useState } from "react";
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
const onUpdate = vi.fn<(...a: unknown[]) => Promise<boolean>>(async () => true);
const onDelete = vi.fn<(...a: unknown[]) => Promise<boolean>>(async () => true);
const onMove = vi.fn<(...a: unknown[]) => Promise<boolean>>(async () => true);

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
  it("has no Edit button; the text is a button named Edit: <text> and Tab only focuses it", async () => {
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "Edit")).toBe(false);
    expect(textButton(0).getAttribute("aria-label")).toBe("Edit: Refresh path has a test");
    await focus(textButton(0));
    await focus(textButton(1));
    await focus(textButton(0));
    expect(editor()).toBeNull();
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("F2 opens the editor, named Edit: <text>, with the caret at the end", async () => {
    await key(textButton(0), "F2");
    const input = editor()!;
    expect(input.getAttribute("aria-label")).toBe("Edit: Refresh path has a test");
    expect(input.value).toBe("Refresh path has a test");
    expect(input.selectionStart).toBe(input.value.length);
    expect(input.selectionEnd).toBe(input.value.length);
  });

  it("Alt+Down works from the text button", async () => {
    await act(async () => {
      textButton(0).dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "ArrowDown",
          altKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(onMove).toHaveBeenCalledWith(1, 2);
  });

  it("Enter ending an IME composition does not save", async () => {
    await click(textButton(0));
    await typeInto(editor()!, "composing");
    await act(async () => {
      editor()!.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Enter",
          isComposing: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(onUpdate).not.toHaveBeenCalled();
    expect(editor()).not.toBeNull();
  });

  it("clicking the text opens the editor too", async () => {
    await click(textButton(1));
    expect(editor()?.value).toBe("Sign-out clears the profile");
  });

  it("Enter saves the trimmed text and closes the editor", async () => {
    await click(textButton(0));
    await typeInto(editor()!, "  Refresh path is tested  ");
    await key(editor()!, "Enter");
    expect(onUpdate).toHaveBeenCalledWith(1, { text: "Refresh path is tested" });
    expect(editor()).toBeNull();
  });

  it("leaving the field saves, once", async () => {
    await click(textButton(0));
    await typeInto(editor()!, "Changed");
    await act(async () => editor()!.blur());
    await settle();
    expect(onUpdate).toHaveBeenCalledTimes(1);
    expect(onUpdate).toHaveBeenCalledWith(1, { text: "Changed" });
  });

  it("Esc cancels without saving", async () => {
    await click(textButton(0));
    await typeInto(editor()!, "Never saved");
    const notCancelled = await key(editor()!, "Escape");
    expect(notCancelled).toBe(false);
    expect(onUpdate).not.toHaveBeenCalled();
    expect(editor()).toBeNull();
    expect(textButton(0).textContent).toBe("Refresh path has a test");
  });

  it("empty text on save is a cancel: nothing is saved or deleted", async () => {
    await click(textButton(0));
    await typeInto(editor()!, "   ");
    await key(editor()!, "Enter");
    expect(onUpdate).not.toHaveBeenCalled();
    expect(onDelete).not.toHaveBeenCalled();
    expect(editor()).toBeNull();
    expect(textButton(0).textContent).toBe("Refresh path has a test");
  });

  it("returns focus to the row, not the text, so the editor does not reopen", async () => {
    await click(textButton(0));
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

// A list that behaves like Rust: it applies changes and re-renders.
function Live({
  start,
  failDelete = false,
  failUpdate = false,
  filterDone = false,
}: {
  start: SessionItem[];
  failDelete?: boolean;
  failUpdate?: boolean;
  filterDone?: boolean;
}) {
  const [items, setItems] = useState(start);
  const addRef = useRef<HTMLInputElement>(null);
  const shown = filterDone ? items.filter((i) => !i.done) : items;
  return (
    <>
      <ItemRows
        kind="checklist"
        items={shown}
        focusFallback={() => addRef.current?.focus()}
        onUpdate={async (id, change) => {
          onUpdate(id, change);
          if (failUpdate) return false;
          setItems((l) =>
            l.map((i) => (i.id === id ? { ...i, ...(change as Partial<SessionItem>) } : i)),
          );
          return true;
        }}
        onDelete={async (id) => {
          onDelete(id);
          if (failDelete) return false;
          setItems((l) => l.filter((i) => i.id !== id));
          return true;
        }}
        onMove={onMove}
      />
      <input ref={addRef} aria-label="Add" />
    </>
  );
}

async function mountLive(props: Parameters<typeof Live>[0]) {
  await act(async () => root.unmount());
  host.remove();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<Live {...props} />));
}

const three = [item(1, "one"), item(2, "two"), item(3, "three")];
const confirmDelete = async (label: string) => {
  await click(host.querySelector<HTMLButtonElement>(`[aria-label="Delete ${label}"]`)!);
  await click(buttonIn(host.querySelector("dialog")!, "Delete"));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 5));
  });
};

describe("focus after a row goes away", () => {
  it("delete moves focus to the next row", async () => {
    await mountLive({ start: three });
    await confirmDelete("two");
    expect(document.activeElement).toBe(host.querySelector('[data-item-id="3"]'));
  });

  it("deleting the last row focuses the previous one", async () => {
    await mountLive({ start: three });
    await confirmDelete("three");
    expect(document.activeElement).toBe(host.querySelector('[data-item-id="2"]'));
  });

  it("deleting the only row focuses the Add input", async () => {
    await mountLive({ start: [item(1, "one")] });
    await confirmDelete("one");
    expect(document.activeElement).toBe(host.querySelector('[aria-label="Add"]'));
  });

  it("ticking an item that a filter then hides moves focus on", async () => {
    await mountLive({ start: three, filterDone: true });
    const box = host.querySelector<HTMLInputElement>('input[aria-label="Done: two"]')!;
    await act(async () => box.focus());
    await click(box);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(host.querySelector('[data-item-id="2"]')).toBeNull();
    expect(document.activeElement).toBe(host.querySelector('[data-item-id="3"]'));
  });

  it("a failed delete keeps the dialog open with the error inside", async () => {
    await mountLive({ start: three, failDelete: true });
    await confirmDelete("two");
    const dialog = host.querySelector("dialog")!;
    expect(dialog.hasAttribute("open")).toBe(true);
    expect(dialog.textContent).toContain("That wasn't deleted");
    expect(host.querySelector('[data-item-id="2"]')).not.toBeNull();
  });
});

describe("unsaved edits", () => {
  it("a failed save is kept for that row, marked, and survives editing another row", async () => {
    await mountLive({ start: three, failUpdate: true });
    await click(textButton(0));
    await typeInto(editor()!, "one, changed");
    await key(editor()!, "Enter");
    expect(host.querySelector('[data-item-id="1"]')!.textContent).toContain("Not saved yet");
    expect(textButton(0).textContent).toBe("one, changed");
    // Editing and leaving another row does not touch the first draft.
    await click(textButton(1));
    await act(async () => editor()!.blur());
    await settle();
    expect(textButton(0).textContent).toBe("one, changed");
    // Opening the row again starts from the draft.
    await click(textButton(0));
    expect(editor()!.value).toBe("one, changed");
  });
});

describe("saving", () => {
  it("the editor is read-only while a save is in flight and Enter does not save twice", async () => {
    let release: (ok: boolean) => void = () => undefined;
    onUpdate.mockImplementationOnce(() => new Promise<boolean>((r) => (release = r)));
    await click(textButton(0));
    await typeInto(editor()!, "slow");
    await key(editor()!, "Enter");
    expect(editor()!.readOnly).toBe(true);
    await key(editor()!, "Enter");
    expect(onUpdate).toHaveBeenCalledTimes(1);
    await act(async () => release(true));
    await settle();
    expect(editor()).toBeNull();
  });
});
