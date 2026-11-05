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
  /**
   * Plain text, shown as the activity's title.
   */
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
  /**
   * Oldest Lullshift version that can run this activity (semver).
   */
  minHostVersion: string;
  /**
   * Activity SDK version the activity was built against (semver).
   */
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
   * Relative path to the engine data file inside the activity folder, validated against the engine's data schema. Segments start with a letter, digit or underscore, so `.`, `..`, `.git`, hidden names and a leading `-` are impossible.
   */
  data?: string;
  help?: Help;
  /**
   * Relative path to a preview image (PNG, JPEG, WebP or SVG, at most 256 KiB), shown on the activity card and the paused view. The host shows it only as an image.
   */
  preview?: string;
};

/**
 * How-to-play help, rendered by the host in its own panel (S05-T07). Plain text only: no control, bidirectional-override or invisible formatting characters, never HTML or Markdown.
 */
export interface Help {
  /**
   * One or two sentences: what this is.
   */
  summary: string;
  /**
   * What counts as done or winning, if anything.
   */
  goal?: string;
  /**
   * Steps, in order.
   *
   * @minItems 1
   * @maxItems 8
   */
  howToPlay: string[];
  /**
   * Keyboard and pointer controls.
   *
   * @maxItems 12
   */
  controls?: {
    /**
     * Key names as shown to the user, e.g. "Arrow keys" or "Enter / Space".
     */
    keys: string;
    action: string;
  }[];
  /**
   * @maxItems 6
   */
  tips?: string[];
}
