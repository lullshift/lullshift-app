/* Generated from reference/MARKETPLACE_V1.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */

/**
 * A `/`-separated path of at most 12 parts. Each part starts with a letter, digit or underscore (so no empty, `.`, `..`, `.git` or leading `-`), has at most 64 characters from letters, digits, `.`, `_`, `-`, does not end with `.`, and is not a Windows reserved device name (CON, PRN, AUX, NUL, COM1-9, LPT1-9, with or without an extension). Enforced again by the Rust marketplace crate.
 */
export type RelativePath = string;
/**
 * A branch, tag or commit id. No leading '-', '..', '//', ':', '^', '~', '*', '?', '[', backslash, whitespace or '@{', no part starting with '.', no '.lock' suffix. At run time it must also pass `git check-ref-format --allow-onelevel`, or be a 40-hex commit id.
 */
export type GitRef = string;

/**
 * .lullshift/marketplace.json at the root of a Git repository that acts as a Lullshift marketplace (ADR-002). Each entry points to an activity folder containing manifest.json, either in this repository or in another Git repository.
 */
export interface MarketplaceIndexV1 {
  schemaVersion: 1;
  /**
   * Short marketplace name shown in the UI and used in commands. 'lullshift' is reserved for the built-in marketplace.
   */
  name: string;
  description?: string;
  /**
   * Self-declared, unverified. Plain text.
   */
  owner?: string;
  /**
   * @maxItems 1000
   */
  activities: Entry[];
}
export interface Entry {
  /**
   * Must equal the id in the referenced manifest.json.
   */
  id: string;
  source:
    | {
        path: RelativePath;
      }
    | {
        /**
         * https://host/path, ssh://[user@]host[:port]/path, git@host:path or GitHub shorthand owner/repo. No userinfo (https allows none; ssh allows a user name only, never a password), no option-looking host, owner or path part, no `..` or `.git` path part (a final `repo.git` is fine), no control characters or whitespace, and no localhost, IP literal or local-network host name.
         */
        git: string;
        ref?: GitRef;
        path?: RelativePath;
      };
  featured?: boolean;
}
