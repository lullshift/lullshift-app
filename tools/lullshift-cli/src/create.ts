// `lullshift create` and `lullshift marketplace init` (ADR-017): scaffolding only, never
// overwriting. Everything written passes `validate` straight away.

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { BRIDGE_FILE, SDK_VERSION, isPlainText } from "@lullshift/activity-sdk";
import { validateMarketplaceIndex } from "@lullshift/agent-protocol";
import { BRIDGE_PATH, TEMPLATES_DIR } from "./paths.ts";

export type Kind = "app" | "content";

export interface CreateOptions {
  dir: string;
  kind: Kind;
  id: string;
  name: string;
  author: string;
  /** Content packs only: `id@major`. */
  engine: string;
}

const ID = /^[a-z0-9]+(\.[a-z0-9-]+)+$/;
const ENGINE = /^[a-z0-9]+(\.[a-z0-9-]+)+@[0-9]+$/;

/** Turns a folder name into a readable activity name: "quiet-counter" → "Quiet Counter". */
export function nameFromFolder(dir: string): string {
  const base = path
    .basename(path.resolve(dir))
    .replace(/[-_.]+/g, " ")
    .trim();
  return base.replace(/\b\p{Ll}/gu, (c) => c.toUpperCase()) || "My Activity";
}

/** "Quiet Counter" → "dev.local.quiet-counter". */
export function idFromName(name: string): string {
  const slug = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60);
  return `dev.local.${slug || "activity"}`;
}

export function checkCreateOptions(o: CreateOptions): string[] {
  const problems: string[] = [];
  if (!ID.test(o.id) || o.id.length > 160) {
    problems.push(
      `--id ${o.id}: use reverse-DNS style, lower case, e.g. io.github.yourname.pebbles`,
    );
  }
  if (!o.name.trim() || !isPlainText(o.name, 100)) {
    problems.push("--name: 1 to 100 characters of plain text");
  }
  if (o.author !== "" && !isPlainText(o.author, 100)) {
    problems.push("--author: at most 100 characters of plain text");
  }
  if (o.kind === "content" && !ENGINE.test(o.engine)) {
    problems.push(`--engine ${o.engine}: use id@major, e.g. lullshift.cards@1`);
  }
  return problems;
}

/** Writes a new activity folder. Refuses a folder that already has files. */
export function createActivity(o: CreateOptions): string[] {
  const target = path.resolve(o.dir);
  if (existsSync(target) && readdirSync(target).length > 0) {
    throw new Error(`${o.dir} isn't empty; pick a new folder`);
  }
  mkdirSync(target, { recursive: true });
  const values: Record<string, string> = {
    id: o.id,
    name: o.name,
    author: o.author,
    sdkVersion: SDK_VERSION,
    engine: o.engine,
  };
  const fill = (text: string, json: boolean) =>
    text.replace(/\{\{(\w+)\}\}/g, (_, key: string) => {
      const v = values[key] ?? "";
      // In JSON files the value lands inside a string: escape it as JSON.
      return json ? JSON.stringify(v).slice(1, -1) : v;
    });
  const written: string[] = [];
  const from = path.join(TEMPLATES_DIR, o.kind);
  for (const file of readdirSync(from).sort()) {
    const text = readFileSync(path.join(from, file), "utf8");
    writeFileSync(path.join(target, file), fill(text, file.endsWith(".json")));
    written.push(file);
  }
  if (o.kind === "app") {
    copyFileSync(BRIDGE_PATH, path.join(target, BRIDGE_FILE));
    written.push(BRIDGE_FILE);
  }
  return written.sort();
}

/** Writes `.lullshift/marketplace.json` in `repo`. Refuses to replace one. */
export function initMarketplace(repo: string, name: string, owner: string): string {
  const file = path.join(path.resolve(repo), ".lullshift", "marketplace.json");
  if (existsSync(file)) throw new Error(`${file} already exists`);
  const index = {
    schemaVersion: 1,
    name,
    description: "Activities for Lullshift.",
    ...(owner ? { owner } : {}),
    activities: [],
  };
  const checked = validateMarketplaceIndex(index);
  if (!checked.ok) {
    throw new Error(`--name ${name}: ${checked.errors.join("; ")} (2–40 of a-z, 0-9 and -)`);
  }
  if (name === "lullshift")
    throw new Error("the name lullshift is reserved for the built-in marketplace");
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(index, null, 2)}\n`);
  return file;
}
