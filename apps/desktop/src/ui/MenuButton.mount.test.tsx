// @vitest-environment jsdom
// The ⋮ menu button (decision 36): ARIA attributes, roving focus, Home/End, Escape.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { click, key, settle } from "../companion/testDom";
import { MenuButton } from "./MenuButton";

let host: HTMLDivElement;
let root: Root;
const rename = vi.fn();
const remove = vi.fn();

beforeEach(async () => {
  rename.mockClear();
  remove.mockClear();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(
      <MenuButton
        label="More actions for Plans"
        items={[
          { label: "Rename", onSelect: rename },
          { label: "Delete", onSelect: remove, danger: true },
        ]}
      />,
    );
  });
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const button = () => host.querySelector<HTMLButtonElement>(".menu-button")!;
const items = () => [...host.querySelectorAll<HTMLButtonElement>("[role=menuitem]")];

describe("MenuButton", () => {
  it("is a labelled menu button that starts closed", () => {
    expect(button().getAttribute("aria-haspopup")).toBe("menu");
    expect(button().getAttribute("aria-expanded")).toBe("false");
    expect(button().getAttribute("aria-label")).toBe("More actions for Plans");
    expect(host.querySelector("[role=menu]")).toBeNull();
  });

  it("opens from the keyboard and moves focus with arrows, Home and End", async () => {
    button().focus();
    await key(button(), "ArrowDown");
    expect(button().getAttribute("aria-expanded")).toBe("true");
    const menu = host.querySelector("[role=menu]")!;
    expect(button().getAttribute("aria-controls")).toBe(menu.id);
    expect(document.activeElement).toBe(items()[0]);
    await key(menu, "ArrowDown");
    expect(document.activeElement).toBe(items()[1]);
    await key(menu, "ArrowDown");
    expect(document.activeElement).toBe(items()[0]);
    await key(menu, "ArrowUp");
    expect(document.activeElement).toBe(items()[1]);
    await key(menu, "Home");
    expect(document.activeElement).toBe(items()[0]);
    await key(menu, "End");
    expect(document.activeElement).toBe(items()[1]);
    // Only the focused item is in the tab order.
    expect(items().map((i) => i.tabIndex)).toEqual([-1, 0]);
    await key(menu, "Escape");
    expect(host.querySelector("[role=menu]")).toBeNull();
    expect(document.activeElement).toBe(button());
    expect(remove).not.toHaveBeenCalled();
  });

  it("ArrowUp opens on the last item; choosing runs it and returns focus", async () => {
    await key(button(), "ArrowUp");
    expect(document.activeElement).toBe(items()[1]);
    await click(items()[0]!);
    await settle();
    expect(rename).toHaveBeenCalledTimes(1);
    expect(host.querySelector("[role=menu]")).toBeNull();
    expect(document.activeElement).toBe(button());
  });
});
