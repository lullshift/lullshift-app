// @vitest-environment jsdom
// Adding and editing a provider happens in a modal dialog, not an inline form.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const PROFILE = {
  id: "pa",
  kind: "anthropic",
  label: "Provider A",
  baseUrl: "https://api.anthropic.com",
  host: "api.anthropic.com",
  model: "a1",
  models: [{ model: "a1", label: null, isDefault: true }],
  monthlyTokenBudget: null,
  isDefault: true,
  hasKey: true,
  keychainOk: true,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    if (command === "companion_profiles") return Promise.resolve([PROFILE]);
    if (command === "companion_prefs")
      return Promise.resolve({ toolsEnabled: false, lastScope: null });
    if (command === "companion_usage") return Promise.resolve([]);
    return Promise.resolve(null);
  },
  Channel: class {},
}));

import { CompanionSettings } from "./CompanionSettings";
import { buttonIn, click, polyfillDialog, settle } from "./testDom";

polyfillDialog();

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<CompanionSettings />));
  await settle();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const openDialog = () => host.querySelector<HTMLDialogElement>("dialog[open]");

describe("provider dialog", () => {
  it("shows no form on the page until a provider is added or edited", () => {
    expect(host.querySelector(".profile-form")).toBeNull();
    expect(openDialog()).toBeNull();
  });

  it("opens Edit in a modal dialog and closes it on cancel (Esc)", async () => {
    await click(buttonIn(host, "Edit Provider A"));
    const dialog = openDialog()!;
    expect(dialog).not.toBeNull();
    expect(dialog.querySelector("h2")?.textContent).toBe("Edit Provider A");
    expect(dialog.querySelector("form.profile-form")).not.toBeNull();
    expect(host.querySelectorAll("form.profile-form")).toHaveLength(1);
    await act(async () => {
      dialog.dispatchEvent(new Event("cancel", { cancelable: true }));
    });
    await settle();
    expect(openDialog()).toBeNull();
  });

  it("opens Add a provider in a dialog with a Save button tied to its form", async () => {
    await click(buttonIn(host, "Add a provider"));
    const dialog = openDialog()!;
    expect(dialog.querySelector("h2")?.textContent).toBe("Add a provider");
    const form = dialog.querySelector("form")!;
    const save = buttonIn(dialog, "Save provider");
    expect(save.getAttribute("form")).toBe(form.id);
    await click(buttonIn(dialog, "Cancel"));
    expect(openDialog()).toBeNull();
  });
});
