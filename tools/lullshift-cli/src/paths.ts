// Where the CLI finds its templates, harness, the SDK bridge, kits and engines. Two layouts:
// - in this repository (`pnpm lullshift`, tests): this file is src/paths.ts. Kits and engines are
//   read from the repository, so a change there shows at once.
// - installed from npm (decision 29): this file is dist/paths.js. The build
//   (scripts/npm-build.mjs) copied the harness, kits and engines into dist/, so the installed CLI
//   reads nothing outside its own package and its dependencies.
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

/** True when running the compiled package from dist/, false when running the sources. */
export const BUILT = path.basename(here) === "dist";
export const CLI_ROOT = path.resolve(here, "..");
const REPO_ROOT = path.resolve(CLI_ROOT, "../..");

/** The bridge script, from the @lullshift/activity-sdk this CLI depends on. */
export const BRIDGE_PATH = require.resolve("@lullshift/activity-sdk/bridge/lullshift-bridge-v1.js");
/** The SDK's host protocol: TypeScript in the workspace, JavaScript when installed. */
export const HOST_PROTOCOL_PATH = require.resolve("@lullshift/activity-sdk/host");
/** The generated classic-script kits (ADR-019): lullshift-<kit>-kit-v1.js. */
export const KITS_DIR = BUILT
  ? path.join(here, "assets", "kits")
  : path.join(REPO_ROOT, "packages/activity-kits/dist");
export const ENGINES_DIR = BUILT
  ? path.join(here, "assets", "engines")
  : path.join(REPO_ROOT, "activities/engines");
export const TEMPLATES_DIR = path.join(CLI_ROOT, "templates");

/**
 * The folder to resolve the user's relative paths from, if not the current one. `pnpm lullshift`
 * runs the sources from the repository root, so there INIT_CWD (the folder the command was typed
 * in, set by pnpm and npm) is honoured. The installed CLI ignores it: it already runs where it was
 * called, and an npm script that changes folder before calling `lullshift` would otherwise
 * write somewhere else.
 */
export function startDir(env: NodeJS.ProcessEnv, built: boolean = BUILT): string | undefined {
  return !built && env.INIT_CWD ? env.INIT_CWD : undefined;
}
export const HARNESS_DIR = BUILT ? path.join(here, "harness") : path.join(CLI_ROOT, "harness");
/** The harness page's script: stripped to JavaScript by the build, or on request in the repo. */
export const HARNESS_SCRIPT = path.join(HARNESS_DIR, BUILT ? "host.js" : "host.ts");
