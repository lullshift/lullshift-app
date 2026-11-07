import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/figtree";
import "../styles/tokens.css";
import "../styles.css";
import "../styles/home.css";
import "./tray.css";
import { startThemeSync } from "../theme";
import TrayApp from "./TrayApp";

startThemeSync();

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <StrictMode>
    <TrayApp />
  </StrictMode>,
);
