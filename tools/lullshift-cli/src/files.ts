// Reading an activity folder safely (ADR-017): no links, only allowed file types, canonical
// relative paths, and bounded sizes. Everything else in the CLI works from this list, never
// from the raw directory, so a file that fails here is never validated, served or packed.

import {
  closeSync,
  constants,
  fstatSync,
  lstatSync,
  openSync,
  readSync,
  readdirSync,
} from "node:fs";
import path from "node:path";

/** Limits for one activity folder. */
export const LIMITS = {
  files: 2000,
  fileBytes: 5 * 1024 * 1024,
  totalBytes: 25 * 1024 * 1024,
  previewBytes: 256 * 1024,
  dataBytes: 256 * 1024,
  depth: 8,
} as const;

/** File types the host serves (mirrors `mime()` in `activities.rs`), plus docs that stay local. */
export const SERVED_TYPES: Record<string, string> = {
  html: "text/html; charset=utf-8",
  js: "text/javascript; charset=utf-8",
  mjs: "text/javascript; charset=utf-8",
  css: "text/css; charset=utf-8",
  json: "application/json",
  svg: "image/svg+xml",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  woff2: "font/woff2",
  ogg: "audio/ogg",
  mp3: "audio/mpeg",
};

/** Text files a creator may keep beside an activity; packed, never served. */
const DOC_TYPES = new Set(["md", "txt"]);

/** Code a content pack must never contain (ADR-003). */
export const CODE_TYPES = new Set(["html", "js", "mjs", "css"]);

export interface FileEntry {
  /** Relative path with forward slashes. */
  rel: string;
  abs: string;
  bytes: number;
  ext: string;
}

export interface Listing {
  files: FileEntry[];
  errors: string[];
  /** Hidden files and folders (.git, .gitignore, .DS_Store): never served or packed. */
  skipped: string[];
}

/** A path segment the host would serve: starts with a letter, digit or underscore. */
const SEGMENT = /^[A-Za-z0-9_][A-Za-z0-9._-]{0,63}$/;

export const extOf = (rel: string) => {
  const dot = rel.lastIndexOf(".");
  return dot < 0 ? "" : rel.slice(dot + 1).toLowerCase();
};

/** Lists every file under `root`, reporting (not following) anything unsafe. */
export function listFiles(root: string): Listing {
  const files: FileEntry[] = [];
  const errors: string[] = [];
  const skipped: string[] = [];
  const seen = new Map<string, string>();
  let total = 0;

  const walk = (dir: string, relDir: string, depth: number) => {
    let names: string[];
    try {
      names = readdirSync(dir).sort();
    } catch {
      errors.push(`${relDir || "."}: can't be read`);
      return;
    }
    for (const name of names) {
      const rel = relDir ? `${relDir}/${name}` : name;
      const abs = path.join(dir, name);
      const st = lstatSync(abs);
      if (st.isSymbolicLink()) {
        errors.push(`${rel}: links aren't allowed`);
        continue;
      }
      if (name.startsWith(".")) {
        skipped.push(rel);
        continue;
      }
      if (!SEGMENT.test(name)) {
        errors.push(
          `${rel}: name must start with a letter, digit or underscore and use only A-Z a-z 0-9 . _ -`,
        );
        continue;
      }
      // Two names that differ only by case collide on Windows and macOS.
      const folded = rel.toLowerCase();
      const clash = seen.get(folded);
      if (clash) {
        errors.push(`${rel}: clashes with ${clash} on case-insensitive file systems`);
        continue;
      }
      seen.set(folded, rel);
      if (st.isDirectory()) {
        if (depth + 1 >= LIMITS.depth) {
          errors.push(`${rel}: folders may be at most ${LIMITS.depth - 1} levels deep`);
          continue;
        }
        walk(abs, rel, depth + 1);
        continue;
      }
      if (!st.isFile()) {
        errors.push(`${rel}: not a regular file`);
        continue;
      }
      if (st.nlink > 1) {
        errors.push(`${rel}: hard links aren't allowed`);
        continue;
      }
      const ext = extOf(name);
      if (!(ext in SERVED_TYPES) && !DOC_TYPES.has(ext)) {
        errors.push(`${rel}: .${ext || "(no extension)"} files aren't allowed`);
        continue;
      }
      if (st.size > LIMITS.fileBytes) {
        errors.push(`${rel}: larger than ${LIMITS.fileBytes / 1024 / 1024} MiB`);
        continue;
      }
      total += st.size;
      files.push({ rel, abs, bytes: st.size, ext });
    }
  };

  walk(root, "", 0);
  if (files.length > LIMITS.files) errors.push(`more than ${LIMITS.files} files`);
  if (total > LIMITS.totalBytes) {
    errors.push(`larger than ${LIMITS.totalBytes / 1024 / 1024} MiB in all`);
  }
  files.sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));
  return { files, errors, skipped };
}

/**
 * Reads one listed file at the moment it is served or packed. The listing ran its link checks
 * earlier; the folder may have changed since (a file swapped for a link to ~/.ssh, say), so this
 * checks again: a plain file, opened without following a final link where the platform allows,
 * the same file as the one checked, and within the size limit. Throws otherwise.
 */
export function readListed(f: Pick<FileEntry, "rel" | "abs">): Buffer {
  const before = lstatSync(f.abs);
  if (!before.isFile()) throw new Error(`${f.rel} is no longer a plain file`);
  const noFollow = (constants as { O_NOFOLLOW?: number }).O_NOFOLLOW ?? 0;
  const fd = openSync(f.abs, constants.O_RDONLY | noFollow);
  try {
    const st = fstatSync(fd);
    if (!st.isFile() || st.dev !== before.dev || st.ino !== before.ino) {
      throw new Error(`${f.rel} changed while it was being read`);
    }
    if (st.size > LIMITS.fileBytes) throw new Error(`${f.rel} is now over the size limit`);
    const buf = Buffer.alloc(st.size);
    let got = 0;
    while (got < st.size) {
      const n = readSync(fd, buf, got, st.size - got, got);
      if (n === 0) break;
      got += n;
    }
    return buf.subarray(0, got);
  } finally {
    closeSync(fd);
  }
}

/** Text from an activity, made safe to print in a terminal (no control or escape sequences). */
export const printable = (text: string): string =>
  // eslint-disable-next-line no-control-regex
  text.replace(/[\u0000-\u001f\u007f-\u009f]/g, " ");
