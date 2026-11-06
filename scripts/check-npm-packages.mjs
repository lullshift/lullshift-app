#!/usr/bin/env node
// Packs the three npm packages (owner decision 29) and checks each tarball: only allowlisted
// regular files, nothing from tests or fixtures, no source maps, .env or .npmrc, no local paths,
// no `workspace:` specifiers, entry points that exist, and no secrets (the repository's secretlint
// rules over every extracted file). Uploads nothing. The publish workflow uploads exactly the
// tarballs this leaves with --keep; it never packs again.
//   node scripts/check-npm-packages.mjs              pack into a temp folder, check, delete
//   node scripts/check-npm-packages.mjs --keep <dir> also leave the tarballs in <dir>
// To look inside a tarball by hand: `tar -tzf <file>.tgz`. Don't use `npm publish <file>.tgz`
// for that, even with --dry-run: npm skips the prepublishOnly guard for a tarball.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir, userInfo } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync, gzipSync } from "node:zlib";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** In publish (dependency) order. `allow`: every file in the tarball must match one pattern. */
export const PACKAGES = [
  {
    name: "@lullshift/agent-protocol",
    dir: "packages/agent-protocol",
    allow: [/^dist\/(generated\/)?[a-z-]+\.(js|d\.ts)$/],
    required: ["dist/index.js", "dist/index.d.ts", "dist/generated/schemas.js"],
  },
  {
    name: "@lullshift/activity-sdk",
    dir: "packages/activity-sdk",
    allow: [/^dist\/(host\/)?[a-z-]+\.(js|d\.ts)$/, /^bridge\/lullshift-bridge-v1\.(js|d\.ts)$/],
    required: ["dist/index.js", "dist/host/protocol.js", "bridge/lullshift-bridge-v1.js"],
  },
  {
    name: "@lullshift/cli",
    dir: "tools/lullshift-cli",
    allow: [
      /^dist\/[a-z-]+\.js$/,
      /^dist\/harness\/(index\.html|harness\.css|host\.js)$/,
      /^dist\/assets\/kits\/lullshift-[a-z]+-kit-v1\.js$/,
      /^dist\/assets\/engines\/[a-z0-9-]+\/([a-z0-9._-]+\.(html|css|js|json|svg)|README\.md)$/,
      /^templates\/(app|content)\/([a-z0-9._-]+\.(html|css|js|json|svg)|README\.md)$/,
    ],
    required: [
      "dist/main.js",
      "dist/harness/host.js",
      "dist/assets/kits/lullshift-grid-kit-v1.js",
      "dist/assets/engines/calm-cards/engine.json",
      "templates/app/manifest.json",
      "templates/content/manifest.json",
    ],
  },
];
const COMMON = ["package.json", "README.md", "LICENSE"];
const MAX_BYTES = 4 * 1024 * 1024; // unpacked, per package

/**
 * Minimal tar reader: [{ name, data }] for regular files, [{ name, data, type }] for anything else
 * except folders. Honours GNU long names (`L`) and pax `path=` records (`x`), which override the
 * ustar name of the next entry, the way npm and tar read them.
 */
export function readTgz(bytes) {
  const tar = gunzipSync(bytes);
  const files = [];
  let longName = null;
  for (let off = 0; off + 512 <= tar.length;) {
    const header = tar.subarray(off, off + 512);
    if (header.every((b) => b === 0)) break;
    const str = (a, b) => header.subarray(a, b).toString("utf8").replace(/\0.*$/s, "");
    const size = parseInt(str(124, 136).trim() || "0", 8);
    const type = String.fromCharCode(header[156] || 48);
    const prefix = str(345, 500);
    let name = longName ?? (prefix ? `${prefix}/${str(0, 100)}` : str(0, 100));
    const data = tar.subarray(off + 512, off + 512 + size);
    if (type === "L") longName = data.toString("utf8").replace(/\0.*$/s, "");
    else if (type === "x") longName = paxRecords(data).path ?? longName;
    else if (type === "g") {
      // A global header would rename every later entry; npm never writes one with a path.
      const g = paxRecords(data);
      if (g.path || g.linkpath) files.push({ name: `pax global path=${g.path}`, data, type });
    } else {
      longName = null;
      if (type === "0" || type === "\0") files.push({ name, data: Buffer.from(data) });
      else if (type !== "5") files.push({ name, data, type });
    }
    off += 512 + Math.ceil(size / 512) * 512;
  }
  return files;
}

/** pax extended header records: "<length> <key>=<value>\n", repeated. */
function paxRecords(data) {
  const out = {};
  let at = 0;
  while (at < data.length) {
    const space = data.indexOf(0x20, at);
    if (space < 0) break;
    const len = parseInt(data.subarray(at, space).toString("utf8"), 10);
    if (!Number.isInteger(len) || len <= 0 || at + len > data.length) break;
    const record = data.subarray(space + 1, at + len - 1).toString("utf8");
    const eq = record.indexOf("=");
    if (eq > 0) out[record.slice(0, eq)] = record.slice(eq + 1);
    at += len;
  }
  return out;
}

/** Strings that mean a file leaked something from the machine that packed it. */
function localMarkers(extra = []) {
  const markers = new Set();
  const add = (p) => {
    if (!p || p.length < 4) return; // never "/", "C:\" and the like
    markers.add(p);
    markers.add(p.replaceAll("\\", "/"));
    markers.add(p.replaceAll("\\", "\\\\")); // as written inside a JS or JSON string
  };
  add(root);
  add(homedir());
  add(process.env.RUNNER_TEMP);
  extra.forEach(add);
  return [...markers];
}

/** A home folder of the current user, written any common way (/home/u, /Users/u, C:\Users\u). */
function userHomePattern() {
  let user;
  try {
    user = userInfo().username;
  } catch {
    user = process.env.USER ?? process.env.USERNAME ?? "";
  }
  if (!user) return null;
  const u = user.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`[\\\\/]+(home|Users)[\\\\/]+${u}([\\\\/"'\`\\s]|$)`, "i");
}

/** Problems with one packed tarball (an empty list means it passed). */
export function checkTarball(spec, tgz, { localPaths = [] } = {}) {
  const problems = [];
  const entries = readTgz(tgz);
  const markers = localMarkers(localPaths);
  const userHome = userHomePattern();
  const files = new Map();
  for (const e of entries) {
    if (e.type) problems.push(`${e.name}: not a regular file (tar type ${e.type})`);
    if (!e.name.startsWith("package/")) {
      problems.push(`${e.name}: outside package/`);
      continue;
    }
    files.set(e.name.slice("package/".length), e.data);
  }

  for (const [rel, data] of files) {
    if (rel.split("/").some((p) => p === ".." || p === "" || p.startsWith("."))) {
      problems.push(`${rel}: unsafe or hidden path`);
    }
    if (/(^|\/)(test|tests|fixtures|__tests__|src|scripts|node_modules)\//.test(rel)) {
      problems.push(`${rel}: tests, sources or scripts don't belong in the package`);
    }
    if (/\.(map|tsbuildinfo|tgz|env|npmrc|pem|key)$|(^|\/)\.env/.test(rel)) {
      problems.push(`${rel}: forbidden file type`);
    }
    if (!COMMON.includes(rel) && !spec.allow.some((re) => re.test(rel))) {
      problems.push(`${rel}: not in the allowlist`);
    }
    const text = data.toString("utf8");
    if (/sourceMappingURL=/.test(text)) problems.push(`${rel}: has a sourceMappingURL`);
    const leaked = markers.find((m) => text.includes(m));
    if (leaked) problems.push(`${rel}: contains a local path (${leaked})`);
    else if (userHome?.test(text)) problems.push(`${rel}: contains a local path (a home folder)`);
    if (rel.endsWith(".js") && rel.startsWith("dist/") && !rel.startsWith("dist/assets/")) {
      const tsImport = /(?:from|import)\s*\(?\s*["']\.\.?\/[^"']+\.ts["']/.exec(text);
      if (tsImport) problems.push(`${rel}: imports a .ts file (${tsImport[0]})`);
    }
  }
  for (const rel of [...COMMON, ...spec.required]) {
    if (!files.has(rel)) problems.push(`${rel}: missing`);
  }
  const total = [...files.values()].reduce((n, d) => n + d.length, 0);
  if (total > MAX_BYTES) problems.push(`unpacked size ${total} bytes is over ${MAX_BYTES}`);

  const raw = files.get("package.json")?.toString("utf8") ?? "{}";
  const pkg = JSON.parse(raw);
  if (raw.includes("workspace:")) problems.push("package.json still has a workspace: specifier");
  if (pkg.name !== spec.name) problems.push(`package.json name is ${pkg.name}`);
  if (pkg.private) problems.push("package.json is private");
  if (pkg.license !== "Apache-2.0") problems.push(`license is ${pkg.license}`);
  if (pkg.publishConfig?.access !== "public") problems.push("publishConfig.access isn't public");
  if (!/^>=22(\.|$)/.test(pkg.engines?.node ?? "")) {
    problems.push(`engines.node is ${pkg.engines?.node}; published packages support Node 22+`);
  }
  if (pkg.repository?.url !== "git+https://github.com/lullshift/lullshift-app.git") {
    problems.push("repository.url must name this repository (npm provenance checks it)");
  }
  for (const [dep, range] of Object.entries(pkg.dependencies ?? {})) {
    if (dep.startsWith("@lullshift/") && !/^\d+\.\d+\.\d+$/.test(range)) {
      problems.push(`dependency ${dep}@${range}: expected the exact workspace version`);
    }
  }
  // Every entry point the manifest names must be in the tarball and must not be TypeScript.
  const targets = [];
  const walk = (v) => {
    if (typeof v === "string") targets.push(v);
    else if (v && typeof v === "object") Object.values(v).forEach(walk);
  };
  walk(pkg.exports);
  walk(pkg.bin);
  walk(pkg.types);
  walk(pkg.main);
  if (targets.length === 0) problems.push("package.json names no entry point");
  for (const t of targets) {
    const rel = t.replace(/^\.\//, "");
    if (!files.has(rel)) problems.push(`entry point ${t} isn't in the tarball`);
    if (/\.ts$/.test(rel) && !/\.d\.ts$/.test(rel)) problems.push(`entry point ${t} is TypeScript`);
  }
  for (const b of Object.values(pkg.bin ?? {})) {
    const data = files.get(b.replace(/^\.\//, ""));
    if (data && !data.toString("utf8").startsWith("#!/usr/bin/env node\n")) {
      problems.push(`bin ${b} doesn't start with #!/usr/bin/env node`);
    }
  }
  return { problems, files: [...files.keys()].sort(), contents: files, pkg, bytes: total };
}

/** npm's dist.integrity for a tarball: "sha512-" + base64 of its SHA-512. */
export function integrity(bytes) {
  return `sha512-${createHash("sha512").update(bytes).digest("base64")}`;
}

/** A minimal ustar + gzip writer, for the self-test only. Entries: [name, text, type = "0"]. */
function makeTgz(entries) {
  const blocks = [];
  for (const [name, text, type = "0"] of entries) {
    const data = Buffer.from(text);
    const h = Buffer.alloc(512);
    h.write(name, 0, 100);
    h.write("0000644\0", 100);
    h.write("0000000\0", 108);
    h.write("0000000\0", 116);
    h.write(`${data.length.toString(8).padStart(11, "0")}\0`, 124);
    h.write("00000000000\0", 136);
    h.write("        ", 148);
    h.write(type, 156);
    if (type === "2") h.write("../../../etc/passwd", 157); // link target
    h.write("ustar\0", 257);
    h.write("00", 263);
    const sum = h.reduce((n, b) => n + b, 0);
    h.write(`${sum.toString(8).padStart(6, "0")}\0 `, 148);
    blocks.push(h, data, Buffer.alloc((512 - (data.length % 512)) % 512));
  }
  blocks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(blocks));
}

/** One pax extended-header record, "<length> <key>=<value>\n" (the length counts itself). */
function paxRecord(key, value) {
  const body = ` ${key}=${value}\n`;
  let len = body.length + 1;
  while (String(len).length + body.length !== len) len = String(len).length + body.length;
  return `${len}${body}`;
}

/**
 * Runs the repository's secret scan (secretlint with .secretlintrc.json, as `pnpm secrets` does)
 * over every file under `dir`. The repository's .secretlintignore skips dist/, which is most of a
 * package, so an empty ignore file is used instead. Returns secretlint's exit code (0 = clean).
 */
export function secretScan(dir, { quiet = false } = {}) {
  const ignore = path.join(dir, ".secretlintignore-none");
  writeFileSync(ignore, "");
  try {
    const r = spawnSync(
      process.execPath,
      [
        path.join(root, "node_modules", "secretlint", "bin", "secretlint.js"),
        "--secretlintrc",
        path.join(root, ".secretlintrc.json"),
        "--secretlintignore",
        ignore,
        "--no-gitignore",
        "--no-color",
        "**/*",
      ],
      { cwd: dir, stdio: quiet ? "pipe" : ["ignore", "inherit", "inherit"] },
    );
    return r.error ? 2 : (r.status ?? 2);
  } finally {
    rmSync(ignore, { force: true });
  }
}

/** Writes a checked tarball's files under `dir` for the secret scan (unsafe names are skipped). */
function extract(contents, dir) {
  for (const [rel, data] of contents) {
    if (rel.split("/").some((p) => p === ".." || p === "" || p.includes(":"))) continue;
    const target = path.join(dir, ...rel.split("/"));
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, data);
  }
}

/** The checker must catch what it claims to: run on a deliberately bad tarball first. */
export function selfTest() {
  // Outside the home folder, so only the extra localPaths marker can catch it.
  const packTemp = path.join(path.parse(tmpdir()).root, "lullshift-npm-check-selftest");
  // Another OS's home folder form, so it doesn't contain homedir() itself.
  const otherHome = homedir().includes("/Users/") ? "/home" : "/Users";
  const bad = makeTgz([
    [
      "package/package.json",
      JSON.stringify({
        name: "@lullshift/agent-protocol",
        private: true,
        license: "Apache-2.0",
        engines: { node: ">=24" },
        exports: { ".": "./src/index.ts" },
        dependencies: { "@lullshift/activity-sdk": "workspace:*" },
      }),
    ],
    ["package/README.md", "x"],
    ["package/LICENSE", "x"],
    ["package/dist/index.js", 'import "./a.ts";\n//# sourceMappingURL=index.js.map'],
    ["package/dist/index.js.map", "{}"],
    ["package/test/fixtures/secret.json", "{}"],
    ["package/.env", "TOKEN=x"],
    ["package/dist/notes.txt", root],
    ["package/dist/home.js", `const h = ${JSON.stringify(homedir())};`],
    ["package/dist/user.js", `const h = "${otherHome}/${userInfo().username}/project";`],
    ["package/dist/pack.js", `// packed in ${packTemp}`],
    // pax: the ustar name looks harmless; the path= record is what tar and npm extract.
    ["package/PaxHeader/ok.js", paxRecord("path", "package/test/hidden.js"), "x"],
    ["package/dist/ok.js", "export {};"],
    ["package/dist/link.js", "", "2"],
  ]);
  const { problems } = checkTarball(PACKAGES[0], bad, { localPaths: [packTemp] });
  const expect = [
    "workspace: specifier",
    "is private",
    "engines.node",
    "src/index.ts isn't in the tarball",
    "is TypeScript",
    "sourceMappingURL",
    "index.js.map: forbidden file type",
    "test/fixtures/secret.json: tests",
    ".env: unsafe or hidden path",
    `contains a local path (${root})`,
    "dist/home.js: contains a local path",
    "dist/user.js: contains a local path (a home folder)",
    `dist/pack.js: contains a local path (${packTemp})`,
    "test/hidden.js: tests",
    "dist/link.js: not a regular file (tar type 2)",
    "imports a .ts file",
    "dist/index.d.ts: missing",
    "not in the allowlist",
    "repository.url",
  ];
  const missed = expect.filter((e) => !problems.some((p) => p.includes(e)));
  if (problems.some((p) => p.startsWith("dist/ok.js"))) missed.push("the pax path= record");
  if (missed.length) {
    throw new Error(`self-test: the checker missed ${missed.join("; ")}\n${problems.join("\n")}`);
  }

  // The secret scan must flag a token-shaped string (assembled here, so this file stays clean).
  const scanDir = mkdtempSync(path.join(tmpdir(), "lullshift-npm-secret-selftest-"));
  try {
    mkdirSync(path.join(scanDir, "dist"));
    const fake = ["npm", "_", "Q7xK".repeat(9)].join("");
    writeFileSync(path.join(scanDir, "dist", "config.js"), `export const token = "${fake}";\n`);
    const code = secretScan(scanDir, { quiet: true });
    if (code !== 1)
      throw new Error(`self-test: the secret scan didn't flag a token (exit ${code})`);
  } finally {
    rmSync(scanDir, { recursive: true, force: true });
  }
}

function pack(spec, dest) {
  const before = new Set(readdirSync(dest));
  // pnpm pack runs the package's prepack (the build) and rewrites workspace: and publishConfig
  // the same way pnpm publish does. shell:true finds pnpm.cmd on Windows; arguments are fixed.
  const r = spawnSync("pnpm", ["pack", "--pack-destination", JSON.stringify(dest)], {
    cwd: path.join(root, spec.dir),
    stdio: ["ignore", "pipe", "inherit"],
    shell: true,
  });
  if (r.status !== 0) throw new Error(`pnpm pack failed for ${spec.name} (exit ${r.status})`);
  const made = readdirSync(dest).filter((f) => f.endsWith(".tgz") && !before.has(f));
  if (made.length !== 1) throw new Error(`expected one tarball for ${spec.name}, got ${made}`);
  return path.join(dest, made[0]);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const keepAt = process.argv.indexOf("--keep");
  const keep = keepAt > 0 ? path.resolve(process.argv[keepAt + 1] ?? "") : null;
  selfTest();
  console.log("✓ checker self-test: a bad tarball and a planted token are refused");
  const tmp = mkdtempSync(path.join(tmpdir(), "lullshift-npm-check-"));
  const packDir = path.join(tmp, "packed");
  const scanDir = path.join(tmp, "extracted");
  mkdirSync(packDir);
  let failed = 0;
  try {
    for (const spec of PACKAGES) {
      const tgz = pack(spec, packDir);
      const raw = readFileSync(tgz);
      const { problems, files, contents, pkg, bytes } = checkTarball(spec, raw, {
        localPaths: [tmp],
      });
      const deps = Object.entries(pkg.dependencies ?? {})
        .map(([d, v]) => `${d}@${v}`)
        .join(", ");
      console.log(
        `${problems.length ? "✗" : "✓"} ${pkg.name}@${pkg.version}: ${files.length} files, ${bytes} bytes; deps: ${deps || "none"}`,
      );
      console.log(`    ${path.basename(tgz)} ${integrity(raw)}`);
      for (const p of problems) console.log(`    ${p}`);
      failed += problems.length;
      extract(contents, path.join(scanDir, spec.dir.replaceAll("/", "_")));
      if (keep) {
        mkdirSync(keep, { recursive: true });
        copyFileSync(tgz, path.join(keep, path.basename(tgz)));
      }
    }
    const code = secretScan(scanDir);
    console.log(`${code === 0 ? "✓" : "✗"} secret scan of every packed file (exit ${code})`);
    if (code !== 0) failed += 1;
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  if (keep) console.log(`Tarballs kept in ${keep}`);
  console.log(failed ? `\n${failed} problem(s)` : "\nall npm packages look right");
  process.exit(failed ? 1 : 0);
}
