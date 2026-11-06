#!/usr/bin/env node
// prepublishOnly guard for the npm packages (owner decision 29, ADR-017 amendment). A publish runs
// only inside .github/workflows/publish-npm.yml on main, after the owner approved the run in the
// protected `npm` environment. Anywhere else, `pnpm publish` and `npm publish` stop here.
//
// To try a publish locally without uploading, pack and check instead:
//   node scripts/check-npm-packages.mjs --keep <dir>     then   npm publish --dry-run <dir>/<file>.tgz
// (`npm publish --dry-run` sets npm_config_dry_run, which this guard lets through; pnpm gives
// lifecycle scripts no dry-run signal, so `pnpm publish --dry-run` stops here outside the workflow.)
//
// This is a seatbelt, not the lock: `--ignore-scripts` skips it. The lock is that the only npm
// credential lives in the `npm` environment (or is npm's trusted publisher for that workflow).
const env = process.env;
const dryRun = env.npm_config_dry_run === "true";
const fromWorkflow =
  env.LULLSHIFT_NPM_PUBLISH === "1" &&
  env.GITHUB_ACTIONS === "true" &&
  env.GITHUB_REF === "refs/heads/main" &&
  /^[^@]+\/\.github\/workflows\/publish-npm\.yml@refs\/heads\/main$/.test(
    env.GITHUB_WORKFLOW_REF ?? "",
  );

if (fromWorkflow) {
  console.log("npm-publish-guard: publishing from the approved workflow run");
} else if (dryRun) {
  console.log("npm-publish-guard: dry run, nothing is uploaded");
} else {
  console.error(
    [
      `npm-publish-guard: refusing to publish ${env.npm_package_name ?? "this package"}.`,
      "Lullshift's npm packages are published only by the 'Publish npm packages' workflow on main,",
      "after the owner approves the run (docs/RELEASING.md, section 6).",
      "To try it without uploading: node scripts/check-npm-packages.mjs --keep <dir>,",
      "then npm publish --dry-run <dir>/<package>.tgz",
    ].join("\n"),
  );
  process.exit(1);
}
