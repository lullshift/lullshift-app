// Browser demo of the tray window with made-up data (dev server only: open /tray-demo.html).
// The real window is 380 x 560 with no frame; here it sits in a page of the same size.
//
//   lullshiftDemo.shown()                    what Rust sends each time the window is shown
//   lullshiftDemo.attention("claude-code")   a needs-you notice
//   lullshiftDemo.changed()                  re-read the sessions
//   /tray-demo.html?saved                    start with two saved activities (the Resume suggestion)
//
// Play, Resume, Go to session, Library and Talk would ask the main window to open a view; here
// they only log the request (see `open_main` in backend.ts).

import { emit } from "@tauri-apps/api/event";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDemoBackend } from "./backend";
import "@fontsource-variable/figtree";
import "../styles/tokens.css";
import "../styles.css";
import "../styles/home.css";
import "../tray/tray.css";
import { startThemeSync } from "../theme";

installDemoBackend();
startThemeSync();

// Same name as demo.html's helper (the two pages never load together); typed loosely because two
// declarations of `Window.lullshiftDemo` would clash.
(window as unknown as { lullshiftDemo: unknown }).lullshiftDemo = {
  shown: () => emit("tray-shown"),
  attention: (provider = "claude-code") => emit("agent-attention", provider),
  changed: () => emit("sessions-changed"),
};

// A fixed-size frame like the real window; the page around it is just the page.
const style = document.createElement("style");
style.textContent = `
  html.tray, html.tray body { overflow: auto; }
  #root { width: 380px; height: 560px; margin: 24px auto; box-shadow: var(--shadow-lg); }
`;
document.head.append(style);

// Imported after the backend is in place, so nothing calls the real IPC first.
const { default: TrayApp } = await import("../tray/TrayApp");

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");
createRoot(root).render(
  <StrictMode>
    <TrayApp />
  </StrictMode>,
);
