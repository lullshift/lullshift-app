// Browser demo of the desktop UI with made-up data (dev server only: open /demo.html).
// Used for website screenshots and UI review without a backend or anyone's real sessions.
//
//   lullshiftDemo.attention("claude-code")   an agent needs you: a running activity pauses
//   lullshiftDemo.changed()                  re-read the sessions
//   /demo.html?welcome                       start with the first-launch welcome
//   /demo.html?saved                         start with two saved activities (Home, Continue)

import { emit } from "@tauri-apps/api/event";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDemoBackend } from "./backend";
import "@fontsource-variable/figtree";
import "../styles/tokens.css";
import "../styles.css";
import "../styles/home.css";
import "../styles/notes.css";
import { startThemeSync } from "../theme";

installDemoBackend();
startThemeSync();

declare global {
  interface Window {
    lullshiftDemo: {
      attention: (provider?: string) => Promise<void>;
      changed: () => Promise<void>;
    };
  }
}
window.lullshiftDemo = {
  attention: (provider = "claude-code") => emit("agent-attention", provider),
  changed: () => emit("sessions-changed"),
};

// Imported after the backend is in place, so nothing calls the real IPC first.
const { default: App } = await import("../App");

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
