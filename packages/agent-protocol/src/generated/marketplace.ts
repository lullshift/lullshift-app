/* Generated from reference/MARKETPLACE_V1.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */

export type RelativePath = string;

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
   * Self-declared, unverified.
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
        git: string;
        ref?: string;
        path?: RelativePath;
      };
  featured?: boolean;
}
