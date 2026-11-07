import { describe, expect, it } from "vitest";
import { catchUpSummary, type CatchUpStatus } from "./sessionTracking";

const done = (more: Partial<CatchUpStatus> = {}): CatchUpStatus => ({
  outcome: "done",
  finishedUtc: "2026-11-07T10:00:00Z",
  updated: 0,
  unreadable: [],
  incomplete: false,
  ...more,
});

describe("catchUpSummary", () => {
  it("says nothing while the setting is off", () => {
    expect(catchUpSummary(false, null)).toBeNull();
    expect(catchUpSummary(false, done({ updated: 3 }))).toBeNull();
  });

  it("explains that a freshly turned-on setting waits for the next start", () => {
    expect(catchUpSummary(true, null)).toMatch(/next time Lullshift starts/);
    expect(
      catchUpSummary(true, {
        outcome: "not_run",
        finishedUtc: null,
        updated: 0,
        unreadable: [],
        incomplete: false,
      }),
    ).toMatch(/next time Lullshift starts/);
  });

  it("reports what was updated and that it is only a guess", () => {
    expect(catchUpSummary(true, done())).toBe("Last catch-up: nothing to update.");
    expect(catchUpSummary(true, done({ updated: 1 }))).toMatch(/updated 1 session .*guess/);
    expect(catchUpSummary(true, done({ updated: 4 }))).toMatch(/updated 4 sessions .*guesses/);
  });

  it("is honest about unreadable logs and partial scans", () => {
    const text = catchUpSummary(
      true,
      done({ updated: 2, unreadable: ["codex", "cursor", "codex"], incomplete: true }),
    );
    expect(text).toMatch(/Couldn't read the logs of Codex and Cursor\./);
    expect(text).toMatch(/Older sessions may not have been checked\./);
    expect(catchUpSummary(true, done({ unreadable: ["weird-agent"] }))).toMatch(
      /logs of an agent\./,
    );
  });

  it("names every failure outcome", () => {
    const base = done();
    expect(catchUpSummary(true, { ...base, outcome: "busy" })).toMatch(/import was running/);
    expect(catchUpSummary(true, { ...base, outcome: "cancelled" })).toMatch(/Nothing was changed/);
    expect(catchUpSummary(true, { ...base, outcome: "store_unavailable" })).toMatch(
      /local data is unavailable/,
    );
  });
});
