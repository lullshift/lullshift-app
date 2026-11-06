#!/usr/bin/env node
// prepublishOnly guard for the npm packages (owner decision 29, ADR-017 amendment). It refuses a
// publish from a package folder (`pnpm publish`, `npm publish`) anywhere but this repository's
// .github/workflows/publish-npm.yml on main.
//
// It is a seatbelt, not the lock:
// - npm runs prepublishOnly only when publishing a folder. `npm publish <file>.tgz` never runs it,
//   with or without --dry-run, so publishing a tarball skips this guard entirely. The workflow
//   publishes tarballs, so it never reaches this file either.
// - `--ignore-scripts` skips it.
// The locks are the `npm` environment on GitHub (deployment branch policy: main only, with the
// owner as required reviewer) and the npm credential that only that environment can use: the
// NPM_TOKEN environment secret, or npm's trusted publisher configured WITH environment `npm`.
//
// To see what would be uploaded, without any risk of uploading it:
//   node scripts/check-npm-packages.mjs --keep <dir>     then   tar -tzf <dir>/<file>.tgz
// `npm publish --dry-run` in a package folder sets npm_config_dry_run, which this guard lets
// through; pnpm gives lifecycle scripts no dry-run signal, so `pnpm publish --dry-run` stops here.
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Whether a publish may go ahead in this environment, and why. Exported for the tests. */
export function decide(env) {
  const fromWorkflow =
    env.LULLSHIFT_NPM_PUBLISH === "1" &&
    env.GITHUB_ACTIONS === "true" &&
    env.GITHUB_REF === "refs/heads/main" &&
    env.GITHUB_REPOSITORY === "lullshift/lullshift-app" &&
    /^lullshift\/lullshift-app\/\.github\/workflows\/publish-npm\.yml@refs\/heads\/main$/.test(
      env.GITHUB_WORKFLOW_REF ?? "",
    );
  if (fromWorkflow) return { ok: true, why: "publishing from the approved workflow run" };
  if (env.npm_config_dry_run === "true") return { ok: true, why: "dry run, nothing is uploaded" };
  return { ok: false, why: "not the publish workflow on main" };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const env = process.env;
  const { ok, why } = decide(env);
  if (ok) {
    console.log(`npm-publish-guard: ${why}`);
  } else {
    console.error(
      [
        `npm-publish-guard: refusing to publish ${env.npm_package_name ?? "this package"}.`,
        "Lullshift's npm packages are published only by the 'Publish npm packages' workflow on main,",
        "after the owner approves the run (docs/RELEASING.md, section 6).",
        "To see what would be uploaded: node scripts/check-npm-packages.mjs --keep <dir>,",
        "then tar -tzf <dir>/<package>.tgz",
      ].join("\n"),
    );
    process.exit(1);
  }
}
