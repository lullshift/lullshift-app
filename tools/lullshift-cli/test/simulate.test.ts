// The simulator must catch broken activities, not just pass good ones. Runs only where a
// Chromium-family browser is installed; otherwise the suite is skipped (and says so).
import { writeFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { findBrowser } from "../src/browser.ts";
import { diffPaths, simulate, type Outcome } from "../src/simulate.ts";
import { validateFolder } from "../src/validate.ts";
import { scaffold } from "./helpers.ts";

const browser = findBrowser();
const TIMEOUT = 120_000;

async function run(mainJs?: string, html?: string): Promise<Outcome[]> {
  const dir = scaffold();
  if (mainJs !== undefined) writeFileSync(path.join(dir, "main.js"), mainJs);
  if (html !== undefined) writeFileSync(path.join(dir, "index.html"), html);
  const report = validateFolder(dir);
  expect(report.errors).toEqual([]);
  return simulate(report, browser!);
}

const level = (outcomes: Outcome[], scenario: string) =>
  outcomes.find((o) => o.scenario === scenario)?.level;

/** A minimal activity: `body` runs inside onInit with `state` in scope. */
const activity = (opts: { init?: string; save?: string; start?: boolean } = {}) => `
(function () {
  var state = { n: 0 };
  ${opts.start === false ? "" : "LullshiftBridge.start({"}
  ${opts.start === false ? "" : `onInit: function (init) { if (init.restore) state = init.restore; ${opts.init ?? ""} },`}
  ${opts.start === false ? "" : `saveState: function () { ${opts.save ?? "return state;"} } });`}
})();`;

describe("diffPaths", () => {
  it("tells shape changes from value changes", () => {
    expect(diffPaths({ a: 1, b: [1] }, { a: 2, b: [1] })).toEqual({
      structural: [],
      values: ["$.a"],
    });
    expect(diffPaths({ a: 1 }, { a: "1" }).structural).toEqual(["$.a"]);
    expect(diffPaths({ a: [1] }, { a: [1, 2] }).structural).toEqual(["$.a.length"]);
    expect(diffPaths({ a: 1 }, { b: 1 }).structural.sort()).toEqual(["$.a", "$.b"]);
  });
});

describe.skipIf(!browser)(
  `simulate (${browser ? path.basename(browser) : "no browser found: skipped"})`,
  () => {
    it(
      "passes the app template",
      async () => {
        const outcomes = await run();
        expect(outcomes.filter((o) => o.level === "error")).toEqual([]);
      },
      TIMEOUT,
    );

    it(
      "fails an activity that never starts the bridge",
      async () => {
        const outcomes = await run(activity({ start: false }));
        expect(level(outcomes, "fresh start")).toBe("error");
      },
      TIMEOUT,
    );

    it(
      "fails an activity that reaches for the network, and one that throws",
      async () => {
        const net = await run(
          activity({
            init: "var i = new Image(); i.src = 'https://example.com/pixel.png'; document.body.appendChild(i);",
          }),
        );
        expect(level(net, "offline")).toBe("error");
        const throws = await run(
          activity({ init: "setTimeout(function () { throw new Error('boom'); }, 10);" }),
        );
        expect(level(throws, "errors")).toBe("error");
      },
      TIMEOUT,
    );

    it(
      "fails a restore that changes the state's shape, and an activity with nothing to focus",
      async () => {
        const shape = await run(
          activity({ save: "return state.restored ? { other: 1 } : { n: 0, restored: true };" }),
        );
        expect(level(shape, "restore")).toBe("error");
        const noFocus = await run(
          activity(),
          '<!doctype html><html><head><meta charset="utf-8"><title>x</title></head><body><p>Nothing here</p><script src="lullshift-bridge-v1.js"></script><script src="main.js"></script></body></html>',
        );
        expect(level(noFocus, "keyboard")).toBe("error");
      },
      TIMEOUT,
    );

    it(
      "warns when an activity can't answer a save and still pauses it",
      async () => {
        const outcomes = await run(
          activity({ save: "var t = Date.now(); while (Date.now() - t < 3000) {} return state;" }),
        );
        expect(level(outcomes, "needs_attention, duplicate, out of order")).toBe("pass");
      },
      TIMEOUT,
    );
  },
);
