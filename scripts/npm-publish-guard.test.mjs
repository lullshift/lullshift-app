// Tests for the prepublishOnly guard (decision 29). Run: node --test scripts/
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { decide } from "./npm-publish-guard.mjs";

const guard = path.join(path.dirname(fileURLToPath(import.meta.url)), "npm-publish-guard.mjs");

const WORKFLOW = {
  LULLSHIFT_NPM_PUBLISH: "1",
  GITHUB_ACTIONS: "true",
  GITHUB_REF: "refs/heads/main",
  GITHUB_REPOSITORY: "lullshift/lullshift-app",
  GITHUB_WORKFLOW_REF: "lullshift/lullshift-app/.github/workflows/publish-npm.yml@refs/heads/main",
};

test("the publish workflow of this repository on main may publish", () => {
  assert.equal(decide(WORKFLOW).ok, true);
});

test("a fork or another repository with the same workflow file is refused", () => {
  for (const repo of ["someone/lullshift-app", "lullshift/lullshift-app-fork", "evil/x"]) {
    const env = {
      ...WORKFLOW,
      GITHUB_REPOSITORY: repo,
      GITHUB_WORKFLOW_REF: `${repo}/.github/workflows/publish-npm.yml@refs/heads/main`,
    };
    assert.equal(decide(env).ok, false, repo);
  }
  // A forged GITHUB_REPOSITORY alone doesn't help: the workflow ref must match too.
  const forged = {
    ...WORKFLOW,
    GITHUB_WORKFLOW_REF: "someone/lullshift-app/.github/workflows/publish-npm.yml@refs/heads/main",
  };
  assert.equal(decide(forged).ok, false);
});

test("other workflows, branches and plain shells are refused", () => {
  const cases = [
    { ...WORKFLOW, GITHUB_REF: "refs/heads/dev" },
    {
      ...WORKFLOW,
      GITHUB_WORKFLOW_REF:
        "lullshift/lullshift-app/.github/workflows/publish-npm.yml@refs/heads/dev",
    },
    {
      ...WORKFLOW,
      GITHUB_WORKFLOW_REF: "lullshift/lullshift-app/.github/workflows/release.yml@refs/heads/main",
    },
    {
      ...WORKFLOW,
      GITHUB_WORKFLOW_REF:
        "lullshift/lullshift-app/x/.github/workflows/publish-npm.yml@refs/heads/main",
    },
    { ...WORKFLOW, LULLSHIFT_NPM_PUBLISH: undefined },
    { ...WORKFLOW, GITHUB_ACTIONS: undefined },
    {},
  ];
  for (const env of cases) assert.equal(decide(env).ok, false, JSON.stringify(env));
});

test("a folder dry run is let through, nothing else", () => {
  assert.equal(decide({ npm_config_dry_run: "true" }).ok, true);
  assert.equal(decide({ npm_config_dry_run: "false" }).ok, false);
});

test("run as prepublishOnly, it exits 1 outside the workflow and 0 inside", () => {
  const base = { PATH: process.env.PATH ?? "", SystemRoot: process.env.SystemRoot ?? "" };
  const run = (env) =>
    spawnSync(process.execPath, [guard], { env: { ...base, ...env }, encoding: "utf8" });
  const refused = run({ npm_package_name: "@lullshift/cli" });
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /refusing to publish @lullshift\/cli/);
  assert.match(refused.stderr, /tar -tzf/);
  assert.equal(run(WORKFLOW).status, 0);
});
