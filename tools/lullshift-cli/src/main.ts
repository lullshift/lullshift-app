#!/usr/bin/env node
// lullshift: the Lullshift creator CLI (ADR-017). Local and offline; no account.

import { writeFileSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { SDK_VERSION } from "@lullshift/activity-sdk";
import { findBrowser, launchBrowser } from "./browser.ts";
import {
  checkCreateOptions,
  createActivity,
  idFromName,
  initMarketplace,
  nameFromFolder,
  type Kind,
  type KitName,
} from "./create.ts";
import { packFiles } from "./pack.ts";
import { startDir } from "./paths.ts";
import { startServers } from "./serve.ts";
import { servedFiles, simulate } from "./simulate.ts";
import { printReport, validateFolder } from "./validate.ts";

const HELP = `lullshift ${SDK_VERSION}: build Lullshift activities locally. No network, no account.

Usage
  lullshift create <folder> [--kind app|content] [--name "Name"] [--id reverse.dns.id]
                            [--author "You"] [--engine lullshift.cards@1]
                            [--kit grid|card|arcade]
  lullshift validate <folder> [--engines <folder>]...
  lullshift simulate <folder> [--browser <path>] [--engines <folder>]... [--json]
  lullshift dev <folder> [--browser <path>] [--engines <folder>]...
  lullshift pack <folder> [--out <file.lullpack>]
  lullshift marketplace init [<repo folder>] --name <name> [--owner "You"]

create --kit copies a game kit (ADR-019) into the folder and loads it after the bridge.
Exit codes: 0 ok, 1 problems found, 2 usage or environment error.
simulate and dev use Microsoft Edge, Google Chrome or Chromium already on this computer; point
at one with --browser or LULLSHIFT_BROWSER.`;

class Usage extends Error {}

async function main(argv: string[]): Promise<number> {
  // `pnpm lullshift` runs from the repository root; resolve folders from where it was typed.
  // The installed CLI (dist/) ignores INIT_CWD (see startDir).
  const start = startDir(process.env);
  if (start) process.chdir(start);
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      kind: { type: "string" },
      name: { type: "string" },
      id: { type: "string" },
      author: { type: "string" },
      engine: { type: "string" },
      kit: { type: "string" },
      engines: { type: "string", multiple: true },
      browser: { type: "string" },
      out: { type: "string" },
      owner: { type: "string" },
      json: { type: "boolean" },
      help: { type: "boolean", short: "h" },
      version: { type: "boolean" },
    },
  });
  const [command, ...rest] = positionals;
  if (values.version) {
    console.log(SDK_VERSION);
    return 0;
  }
  if (values.help || !command) {
    console.log(HELP);
    return command || values.help ? 0 : 2;
  }
  const folder = () => {
    const f = rest[0];
    if (!f) throw new Usage(`${command} needs a folder`);
    return f;
  };
  const engines = (values.engines ?? []).map((e) => path.resolve(e));

  switch (command) {
    case "create": {
      const dir = folder();
      const kind = (values.kind ?? "app") as Kind;
      if (kind !== "app" && kind !== "content") throw new Usage("--kind is app or content");
      const name = values.name ?? nameFromFolder(dir);
      const options = {
        dir,
        kind,
        name,
        id: values.id ?? idFromName(name),
        author: values.author ?? "",
        engine: values.engine ?? "lullshift.cards@1",
        kit: values.kit as KitName | undefined,
      };
      const problems = checkCreateOptions(options);
      if (problems.length) throw new Usage(problems.join("\n"));
      const files = createActivity(options);
      console.log(`Created ${options.name} (${options.id}) in ${dir}: ${files.join(", ")}`);
      console.log(`Next: lullshift dev ${dir}`);
      return printReport(validateFolder(dir, { engines }));
    }

    case "validate": {
      const report = validateFolder(folder(), { engines });
      return printReport(report);
    }

    case "pack": {
      const dir = folder();
      const report = validateFolder(dir, { engines });
      if (printReport(report) !== 0) {
        console.log("Not packed: fix the errors above first.");
        return 1;
      }
      const m = report.manifest!;
      const packed = packFiles(report.files);
      const out = values.out ?? `${m.id}-${m.version}.lullpack`;
      writeFileSync(out, packed.bytes);
      console.log(`Packed ${report.files.length} files into ${out} (${packed.bytes.length} bytes)`);
      console.log(`sha256 ${packed.sha256}`);
      return 0;
    }

    case "simulate": {
      const dir = folder();
      const report = validateFolder(dir, { engines });
      if (printReport(report) !== 0) {
        console.log("Not simulated: fix the errors above first.");
        return 1;
      }
      const exe = findBrowser(values.browser);
      if (!exe) {
        console.error(
          "No Chromium-family browser found. Install Microsoft Edge, Google Chrome or Chromium, or point at one with --browser <path> or LULLSHIFT_BROWSER.",
        );
        return 2;
      }
      console.log(`Simulating with ${path.basename(exe)} (headless)`);
      const outcomes = await simulate(report, exe, values.json ? () => {} : console.log);
      const errors = outcomes.filter((o) => o.level === "error").length;
      const warnings = outcomes.filter((o) => o.level === "warning").length;
      if (values.json) console.log(JSON.stringify({ outcomes, errors, warnings }, null, 2));
      console.log(
        errors
          ? `✗ ${errors} scenario problem(s), ${warnings} warning(s)`
          : `✓ all scenarios passed${warnings ? `, ${warnings} warning(s)` : ""}`,
      );
      return errors ? 1 : 0;
    }

    case "dev": {
      const dir = folder();
      const report = validateFolder(dir, { engines });
      if (printReport(report) !== 0) return 1;
      const exe = findBrowser(values.browser);
      const m = report.manifest!;
      const { files, content } = servedFiles(report);
      const servers = await startServers(files, {
        mode: "dev",
        name: m.name,
        storage: (m.permissions ?? []).includes("activity.storage"),
        content,
      });
      console.log(
        `Harness: ${servers.harnessOrigin}/  (activity served from ${servers.activityOrigin})`,
      );
      let browser: Awaited<ReturnType<typeof launchBrowser>> | null = null;
      if (exe) {
        browser = await launchBrowser(exe, { visible: true });
        await browser.cdp.send("Page.navigate", { url: `${servers.harnessOrigin}/` }, browser.page);
      } else {
        console.log(
          "No browser found to open; open the harness address in a Chromium-family browser.",
        );
      }
      console.log(
        "Files are served fresh on every load: edit, then press Start or resume. Ctrl+C stops.",
      );
      await new Promise<void>((resolve) => process.once("SIGINT", () => resolve()));
      await browser?.close();
      await servers.close();
      return 0;
    }

    case "marketplace": {
      if (rest[0] !== "init") throw new Usage("marketplace has one subcommand: init");
      if (!values.name) throw new Usage("marketplace init needs --name");
      const file = initMarketplace(rest[1] ?? ".", values.name, values.owner ?? "");
      console.log(`Wrote ${file}. Add activities under "activities", then validate each one.`);
      return 0;
    }

    default:
      throw new Usage(`unknown command ${command}\n\n${HELP}`);
  }
}

main(process.argv.slice(2)).then(
  (code) => process.exit(code),
  (e: unknown) => {
    if (e instanceof Usage || (e as { code?: string }).code === "ERR_PARSE_ARGS_UNKNOWN_OPTION") {
      console.error((e as Error).message);
      process.exit(2);
    }
    console.error((e as Error).message ?? e);
    process.exit(2);
  },
);
