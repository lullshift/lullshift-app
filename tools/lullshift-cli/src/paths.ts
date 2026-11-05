// Where the CLI finds the SDK, schemas, engines and templates inside this repository.
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

export const CLI_ROOT = path.resolve(here, "..");
export const REPO_ROOT = path.resolve(CLI_ROOT, "../..");
export const SDK_ROOT = path.join(REPO_ROOT, "packages/activity-sdk");
export const BRIDGE_PATH = path.join(SDK_ROOT, "bridge/lullshift-bridge-v1.js");
/** The generated classic-script kits (ADR-019): lullshift-<kit>-kit-v1.js. */
export const KITS_DIR = path.join(REPO_ROOT, "packages/activity-kits/dist");
export const HOST_PROTOCOL_PATH = path.join(SDK_ROOT, "src/host/protocol.ts");
export const ENGINES_DIR = path.join(REPO_ROOT, "activities/engines");
export const TEMPLATES_DIR = path.join(CLI_ROOT, "templates");
export const HARNESS_DIR = path.join(CLI_ROOT, "harness");
export const SCHEMA_DIR = path.join(REPO_ROOT, "reference");
