// Test helpers for jsdom mount tests of Companion components. Not used by the app.

import { act } from "react";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** jsdom has no modal dialogs: open and close just toggle the attribute. */
export function polyfillDialog(): void {
  const proto = HTMLDialogElement.prototype as HTMLDialogElement & {
    showModal: () => void;
    close: () => void;
  };
  proto.showModal = function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  proto.close = function (this: HTMLDialogElement) {
    this.removeAttribute("open");
  };
  // Like a browser, nothing inside a closed dialog can take focus (it isn't rendered).
  const focus = HTMLElement.prototype.focus;
  HTMLElement.prototype.focus = function (this: HTMLElement, options?: FocusOptions) {
    const d = this.closest("dialog");
    if (d && !d.hasAttribute("open")) return;
    focus.call(this, options);
  };
  // jsdom doesn't lay out, so it has no scrolling either.
  Element.prototype.scrollIntoView ??= function () {};
}

/** Lets pending promises and the effects they trigger settle. */
export async function settle(rounds = 6): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

export function buttonIn(root: ParentNode, label: string): HTMLButtonElement {
  const b = [...root.querySelectorAll("button")].find(
    (x) => x.textContent?.trim() === label || x.getAttribute("aria-label") === label,
  );
  if (!b) throw new Error(`no button "${label}" in: ${root.textContent ?? ""}`);
  return b;
}

export async function click(el: HTMLElement): Promise<void> {
  await act(async () => {
    el.click();
  });
  await settle();
}

/** Types into a React-controlled input or textarea. */
export async function typeInto(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  const setter = Object.getOwnPropertyDescriptor(proto.prototype, "value")?.set;
  await act(async () => {
    setter?.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

export async function key(
  el: Element,
  k: string,
  more: { shiftKey?: boolean; ctrlKey?: boolean; isComposing?: boolean; keyCode?: number } = {},
): Promise<boolean> {
  const e = new KeyboardEvent("keydown", {
    key: k,
    bubbles: true,
    cancelable: true,
    shiftKey: !!more.shiftKey,
    ctrlKey: !!more.ctrlKey,
    isComposing: !!more.isComposing,
  });
  if (more.keyCode !== undefined) Object.defineProperty(e, "keyCode", { value: more.keyCode });
  let notCancelled = true;
  await act(async () => {
    notCancelled = el.dispatchEvent(e);
  });
  await settle(2);
  return notCancelled;
}
