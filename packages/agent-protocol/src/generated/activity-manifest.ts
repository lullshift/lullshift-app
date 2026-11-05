/* Generated from reference/ACTIVITY_MANIFEST_V1.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */

/**
 * manifest.json at the root of every activity folder or .lullpack. kind=app is a sandboxed static web app; kind=content is code-free data for a named engine (ADR-003).
 */
export type ActivityManifestV1 = {
  schemaVersion: 1;
  kind: "app" | "content";
  /**
   * Reverse-DNS style identifier, unique within a marketplace.
   */
  id: string;
  version: string;
  name: string;
  description?: string;
  /**
   * Self-declared and unverified (there are no accounts). The install UI shows the source repository as the trust signal.
   */
  author?: string;
  /**
   * SPDX license expression for the activity's code and content.
   */
  license?: string;
  category: "arcade" | "break" | "productivity" | "interactive";
  /**
   * @maxItems 10
   */
  tags?: string[];
  minHostVersion: string;
  sdkVersion: string;
  permissions: "activity.storage"[];
  offline: true;
  entrypoint?: "index.html";
  runtime?: "isolated-web-v1";
  interruptibility?: {
    pause: true;
    resume: true;
    persistState: true;
  };
  /**
   * Engine id and major version, e.g. lullshift.quiz@1.
   */
  engine?: string;
  /**
   * Relative path to the engine data file inside the activity folder. Validated against the engine's data schema.
   */
  data?: string;
};
