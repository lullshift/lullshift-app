import { describe, expect, it } from "vitest";
import { ERROR_KINDS, type CatalogModel, type Scope } from "./api";
import {
  baseUrlHint,
  baseUrlProblem,
  CATALOG_PROVIDER,
  catalogCountText,
  catalogErrorText,
  countOf,
  contextText,
  costText,
  DEFAULT_CATALOG_FILTERS,
  filterModels,
  filterProviders,
  formatUsd,
  isOlderThanAYear,
  loadedAtText,
  modelFacts,
  releaseTime,
  budgetView,
  buildScope,
  canSend,
  charCountText,
  EMPTY_SCOPE,
  historyCount,
  historyText,
  isContextChanged,
  MAX_HISTORY_MESSAGES,
  MAX_SCOPE_SESSIONS,
  type DraftSession,
  shouldMountCompanion,
  errorText,
  keyInputProblem,
  needsPreview,
  needsSettings,
  parseBudget,
  plainLines,
  retryAfterSeconds,
  scopeKey,
  scopeLabel,
  sendsToText,
  sharedLabel,
  UNAVAILABLE_TEXT,
} from "./logic";

describe("errorText", () => {
  it("has distinct, non-empty text for every kind", () => {
    const texts = new Set<string>();
    for (const kind of [...ERROR_KINDS, "unavailable"]) {
      const t = errorText({ kind, message: "" }, "api.example.com");
      expect(t.length).toBeGreaterThan(10);
      texts.add(t);
    }
    expect(texts.size).toBe(ERROR_KINDS.length + 1);
  });

  it("names the host when offline", () => {
    expect(errorText({ kind: "offline" }, "api.anthropic.com")).toBe(
      "Can't reach api.anthropic.com. Check your connection.",
    );
  });

  it("shows the provider's wait time when rate limited", () => {
    expect(errorText({ kind: "rateLimited", message: "Try again in 30 seconds." }, "h")).toContain(
      "30 seconds",
    );
    expect(errorText({ kind: "rateLimited", message: "Wait 2 minutes" }, "h")).toContain(
      "2 minutes",
    );
    expect(errorText({ kind: "rateLimited", message: "" }, "h")).toContain("didn't say how long");
  });

  it("never echoes the backend message", () => {
    const t = errorText({ kind: "providerError", message: "sk-secret-123 <b>hi</b>" }, "h");
    expect(t).not.toContain("sk-secret");
    expect(t).not.toContain("<b>");
  });

  it("treats an unknown kind as internal", () => {
    expect(errorText({ kind: "weird", message: "x" })).toBe(errorText({ kind: "internal" }));
  });

  it("says Companion is unavailable when the commands are missing", () => {
    expect(errorText({ kind: "unavailable" })).toBe(UNAVAILABLE_TEXT);
  });

  it("links noKey, keyRejected and budgetReached to settings", () => {
    expect(needsSettings("noKey")).toBe(true);
    expect(needsSettings("budgetReached")).toBe(true);
    expect(needsSettings("offline")).toBe(false);
  });

  it("reads wait times", () => {
    expect(retryAfterSeconds("in 45 seconds")).toBe(45);
    expect(retryAfterSeconds("in 3 minutes")).toBe(180);
    expect(retryAfterSeconds("later")).toBeNull();
    expect(retryAfterSeconds("in 0 seconds")).toBeNull();
  });
});

describe("scopes", () => {
  it("labels the three scopes", () => {
    expect(scopeLabel("none")).toBe("No context");
    expect(scopeLabel("sessionSummary")).toBe("Session summary");
    expect(scopeLabel("selectedNotes")).toBe("Selected notes");
  });

  it("describes what a past message shared", () => {
    expect(sharedLabel("none", "h")).toBe("Nothing shared");
    expect(sharedLabel("selectedNotes", "api.openai.com")).toBe(
      "Shared with api.openai.com: Selected notes",
    );
  });

  const ds = (id: string, more: Partial<DraftSession> = {}): DraftSession => ({
    provider: "c",
    sessionId: id,
    notes: false,
    itemIds: [],
    ...more,
  });

  it("builds scopes only when complete", () => {
    expect(buildScope(EMPTY_SCOPE)).toEqual({ kind: "none" });
    expect(buildScope({ kind: "sessionSummary", sessions: [] })).toBeNull();
    expect(buildScope({ kind: "sessionSummary", sessions: [ds("s1"), ds("s2")] })).toEqual({
      kind: "sessionSummary",
      sessions: [
        { provider: "c", sessionId: "s1" },
        { provider: "c", sessionId: "s2" },
      ],
    });
    expect(buildScope({ kind: "selectedNotes", sessions: [ds("s")] })).toBeNull();
    // Every chosen session needs something ticked.
    expect(
      buildScope({
        kind: "selectedNotes",
        sessions: [ds("a", { itemIds: [3, 1] }), ds("b")],
      }),
    ).toBeNull();
    expect(
      buildScope({
        kind: "selectedNotes",
        sessions: [ds("a", { itemIds: [3, 1] }), ds("b", { notes: true })],
      }),
    ).toEqual({
      kind: "selectedNotes",
      sessions: [
        { provider: "c", sessionId: "a", notes: false, itemIds: [1, 3] },
        { provider: "c", sessionId: "b", notes: true, itemIds: [] },
      ],
    });
  });

  it("refuses more than the session cap", () => {
    const many = Array.from({ length: MAX_SCOPE_SESSIONS + 1 }, (_, i) => ds(`s${i}`));
    expect(buildScope({ kind: "sessionSummary", sessions: many })).toBeNull();
    expect(
      buildScope({ kind: "sessionSummary", sessions: many.slice(0, MAX_SCOPE_SESSIONS) }),
    ).not.toBeNull();
  });

  it("compares scopes regardless of item order", () => {
    const a: Scope = {
      kind: "selectedNotes",
      sessions: [{ provider: "c", sessionId: "s", notes: true, itemIds: [2, 1] }],
    };
    const b: Scope = {
      kind: "selectedNotes",
      sessions: [{ provider: "c", sessionId: "s", notes: true, itemIds: [1, 2] }],
    };
    const c: Scope = {
      kind: "selectedNotes",
      sessions: [{ provider: "c", sessionId: "s", notes: false, itemIds: [1, 2] }],
    };
    expect(scopeKey(a)).toBe(scopeKey(b));
    expect(scopeKey(a)).not.toBe(scopeKey(c));
  });

  it("changing the chosen sessions changes the key", () => {
    const one: Scope = { kind: "sessionSummary", sessions: [{ provider: "c", sessionId: "s" }] };
    const two: Scope = {
      kind: "sessionSummary",
      sessions: [
        { provider: "c", sessionId: "s" },
        { provider: "c", sessionId: "t" },
      ],
    };
    expect(scopeKey(one)).not.toBe(scopeKey(two));
  });
});

describe("preview gating", () => {
  const summary: Scope = { kind: "sessionSummary", sessions: [{ provider: "c", sessionId: "s" }] };
  const gate = { text: "hello", hasProfile: true, sending: false };

  it("sends with no context straight away", () => {
    expect(canSend({ ...gate, scope: { kind: "none" }, confirmed: null })).toBe(true);
    expect(needsPreview({ kind: "none" }, null)).toBe(false);
  });

  it("blocks any other scope until the preview is confirmed", () => {
    expect(canSend({ ...gate, scope: summary, confirmed: null })).toBe(false);
    expect(needsPreview(summary, null)).toBe(true);
    const confirmed = { scope: summary, text: "t", sha256: "ab" };
    expect(canSend({ ...gate, scope: summary, confirmed })).toBe(true);
    expect(needsPreview(summary, confirmed)).toBe(false);
  });

  it("invalidates a confirmation when the scope changes", () => {
    const confirmed = { scope: summary, text: "t", sha256: "ab" };
    const other: Scope = { kind: "sessionSummary", sessions: [{ provider: "c", sessionId: "s2" }] };
    expect(canSend({ ...gate, scope: other, confirmed })).toBe(false);
    expect(needsPreview(other, confirmed)).toBe(true);
  });

  it("blocks empty text, no profile, an incomplete scope and a send in progress", () => {
    const none: Scope = { kind: "none" };
    expect(canSend({ ...gate, text: "  \n", scope: none, confirmed: null })).toBe(false);
    expect(canSend({ ...gate, hasProfile: false, scope: none, confirmed: null })).toBe(false);
    expect(canSend({ ...gate, scope: null, confirmed: null })).toBe(false);
    expect(canSend({ ...gate, sending: true, scope: none, confirmed: null })).toBe(false);
  });

  it("counts characters", () => {
    expect(charCountText(1)).toBe("1 character");
    expect(charCountText(1234)).toBe("1,234 characters");
  });
});

describe("plain text", () => {
  it("splits on every newline style and keeps blank lines", () => {
    expect(plainLines("a\r\nb\rc\n\nd")).toEqual(["a", "b", "c", "", "d"]);
  });

  it("leaves URLs and markup untouched", () => {
    const text = 'see https://evil.example/x and <a href="x">y</a> **bold** [l](http://x)';
    expect(plainLines(text)).toEqual([text]);
  });
});

describe("budget", () => {
  it("shows usage with no budget", () => {
    const v = budgetView({ inputTokens: 1000, outputTokens: 234 }, null);
    expect(v.state).toBe("none");
    expect(v.fraction).toBeNull();
    expect(v.line).toBe("1,234 tokens used this month. No budget set.");
  });

  it("flags near and reached", () => {
    expect(budgetView({ inputTokens: 10, outputTokens: 0 }, 100).state).toBe("ok");
    expect(budgetView({ inputTokens: 80, outputTokens: 0 }, 100).state).toBe("near");
    const r = budgetView({ inputTokens: 90, outputTokens: 20 }, 100);
    expect(r.state).toBe("reached");
    expect(r.fraction).toBe(1);
    expect(r.line).toContain("sending is blocked");
  });

  it("treats missing usage as zero", () => {
    expect(budgetView(null, 500).line).toBe("0 of 500 tokens used this month.");
  });

  it("parses the budget field", () => {
    expect(parseBudget("")).toBeNull();
    expect(parseBudget(" 1,000,000 ")).toBe(1_000_000);
    expect(parseBudget("0")).toBe("invalid");
    expect(parseBudget("12.5")).toBe("invalid");
    expect(parseBudget("abc")).toBe("invalid");
  });
});

describe("base URL", () => {
  it("hints differently for compatible servers", () => {
    expect(baseUrlHint("openaiCompatible")).toContain("http:// is accepted only");
    expect(baseUrlHint("openai")).toBe("Fixed for this provider.");
  });

  it("accepts https and loopback http", () => {
    expect(baseUrlProblem("https://api.example.com/v1")).toBeNull();
    expect(baseUrlProblem("http://localhost:11434/v1")).toBeNull();
    expect(baseUrlProblem("http://127.0.0.1:8080")).toBeNull();
    expect(baseUrlProblem("http://[::1]:8080")).toBeNull();
  });

  it("rejects the rest", () => {
    expect(baseUrlProblem("")).not.toBeNull();
    expect(baseUrlProblem("nonsense")).not.toBeNull();
    expect(baseUrlProblem("http://example.com")).toContain(
      "only allowed for a server on this computer",
    );
    expect(baseUrlProblem("https://user:pw@example.com")).not.toBeNull();
    expect(baseUrlProblem("https://example.com/?k=1")).not.toBeNull();
    expect(baseUrlProblem("ftp://example.com")).not.toBeNull();
  });

  it("words the sends-to line", () => {
    expect(sendsToText("api.openai.com")).toBe("Sends to api.openai.com");
    expect(sendsToText(null)).toBe("No provider set up");
  });
});

describe("key input check", () => {
  it("never puts the key in its message", () => {
    const secret = "sk-é-secret-value-XYZ";
    const msg = keyInputProblem(secret);
    expect(msg).not.toBeNull();
    expect(msg).not.toContain("secret");
    expect(keyInputProblem("")).not.toBeNull();
    expect(keyInputProblem("short")).not.toBeNull();
    expect(keyInputProblem("sk-abcdefgh12345")).toBeNull();
  });
});

describe("cancelled wording", () => {
  it("says the request may already have reached the provider", () => {
    const t = errorText({ kind: "cancelled", message: "" }, "api.example.com");
    expect(t).toContain("may already have reached the provider");
    expect(t).not.toContain("Nothing more was sent");
  });
});

describe("stale context", () => {
  it("recognises the backend's context-changed refusal only", () => {
    const msg = "The context changed since you previewed it. Review it again.";
    expect(isContextChanged({ kind: "invalidInput", message: msg })).toBe(true);
    expect(isContextChanged({ kind: "invalidInput", message: "Write a message first." })).toBe(
      false,
    );
    expect(isContextChanged({ kind: "providerError", message: msg })).toBe(false);
  });
});

describe("history count", () => {
  const m = (status: string, host: string | null, text = "hi") => ({ status, host, text });

  it("counts only delivered messages that went to the same host", () => {
    const list = [
      m("sent", "a.example"),
      m("sent", "a.example"),
      m("sent", "b.example"),
      m("failed", "a.example"),
      m("pending", "a.example"),
      m("sent", null),
    ];
    expect(historyCount(list, "a.example")).toBe(2);
    expect(historyCount(list, "b.example")).toBe(1);
    expect(historyCount(list, "c.example")).toBe(0);
    expect(historyCount(list, null)).toBe(0);
  });

  it("applies the same caps as the backend", () => {
    const many = Array.from({ length: 30 }, () => m("sent", "a.example"));
    expect(historyCount(many, "a.example")).toBe(MAX_HISTORY_MESSAGES);
    const big = [
      m("sent", "a.example", "x".repeat(40_000)),
      m("sent", "a.example", "y".repeat(40_000)),
    ];
    expect(historyCount(big, "a.example")).toBe(1);
  });

  it("words the line with the host and the count", () => {
    expect(historyText(0, "a.example")).toBe("No earlier messages are sent with this one.");
    expect(historyText(1, "a.example")).toBe(
      "Also sends 1 earlier message from this conversation to a.example.",
    );
    expect(historyText(3, "a.example")).toContain("3 earlier messages");
  });
});

describe("launch does not touch the keychain", () => {
  it("mounts Companion only after it was opened, and keeps it mounted", () => {
    expect(shouldMountCompanion("sessions", false)).toBe(false);
    expect(shouldMountCompanion("activities", false)).toBe(false);
    expect(shouldMountCompanion("companion", false)).toBe(true);
    expect(shouldMountCompanion("sessions", true)).toBe(true);
  });
});

// ---- the models.dev model list (decision 32) ----------------------------------------------

function model(id: string, more: Partial<CatalogModel> = {}): CatalogModel {
  return {
    id,
    name: id.toUpperCase(),
    family: null,
    toolCall: false,
    reasoning: false,
    releaseDate: null,
    contextTokens: null,
    outputTokens: null,
    costInput: null,
    costOutput: null,
    ...more,
  };
}

const NOW = new Date(Date.UTC(2026, 10, 7));

const MODELS: CatalogModel[] = [
  model("old-cheap", { releaseDate: "2024-01-10", costInput: 0.1, costOutput: 0.4 }),
  model("new-tools", {
    releaseDate: "2026-06-10",
    toolCall: true,
    reasoning: true,
    costInput: 3,
    costOutput: 15,
    family: "sonnet",
  }),
  model("mid", { releaseDate: "2026-01", toolCall: true, costInput: 1 }),
  model("undated"),
];

const ids = (ms: CatalogModel[]) => ms.map((m) => m.id);

describe("model list filters", () => {
  it("maps fixed providers and leaves compatible servers to a picker", () => {
    expect(CATALOG_PROVIDER).toEqual({
      anthropic: "anthropic",
      openai: "openai",
      openaiCompatible: null,
    });
  });

  it("sorts newest first by default, unknown dates last", () => {
    expect(DEFAULT_CATALOG_FILTERS.sort).toBe("newest");
    expect(ids(filterModels(MODELS, DEFAULT_CATALOG_FILTERS, NOW))).toEqual([
      "new-tools",
      "mid",
      "old-cheap",
      "undated",
    ]);
  });

  it("sorts by cheapest input, unknown prices last, and by name", () => {
    const f = { ...DEFAULT_CATALOG_FILTERS };
    expect(ids(filterModels(MODELS, { ...f, sort: "cheapest" }, NOW))).toEqual([
      "old-cheap",
      "mid",
      "new-tools",
      "undated",
    ]);
    expect(ids(filterModels(MODELS, { ...f, sort: "name" }, NOW))).toEqual([
      "mid",
      "new-tools",
      "old-cheap",
      "undated",
    ]);
  });

  it("filters by tools, reasoning and text over name, id and family", () => {
    const f = DEFAULT_CATALOG_FILTERS;
    expect(ids(filterModels(MODELS, { ...f, tools: true }, NOW))).toEqual(["new-tools", "mid"]);
    expect(ids(filterModels(MODELS, { ...f, reasoning: true }, NOW))).toEqual(["new-tools"]);
    expect(ids(filterModels(MODELS, { ...f, text: "SONNET" }, NOW))).toEqual(["new-tools"]);
    expect(ids(filterModels(MODELS, { ...f, text: "old cheap" }, NOW))).toEqual(["old-cheap"]);
    expect(filterModels(MODELS, { ...f, text: "nothing-like-this" }, NOW)).toEqual([]);
  });

  it("hides models released more than a year ago, never undated ones", () => {
    const kept = ids(filterModels(MODELS, { ...DEFAULT_CATALOG_FILTERS, hideOld: true }, NOW));
    expect(kept).toEqual(["new-tools", "mid", "undated"]);
    expect(isOlderThanAYear("2025-11-07", NOW)).toBe(false);
    expect(isOlderThanAYear("2025-11-06", NOW)).toBe(true);
    expect(isOlderThanAYear("2025", NOW)).toBe(true);
    expect(isOlderThanAYear(null, NOW)).toBe(false);
    expect(isOlderThanAYear("soon", NOW)).toBe(false);
  });

  it("reads only real date shapes", () => {
    expect(releaseTime("2026-06-10")).toBe(Date.UTC(2026, 5, 10));
    expect(releaseTime("2026-06")).toBe(Date.UTC(2026, 5, 1));
    expect(releaseTime("2026")).toBe(Date.UTC(2026, 0, 1));
    for (const bad of ["2026-13", "26-01-01", "2026-06-10T00:00", "", null]) {
      expect(releaseTime(bad)).toBeNull();
    }
  });

  it("does not change the list it was given", () => {
    const copy = [...MODELS];
    filterModels(MODELS, { ...DEFAULT_CATALOG_FILTERS, sort: "name" }, NOW);
    expect(MODELS).toEqual(copy);
  });
});

describe("model list wording", () => {
  it("formats prices per million tokens and says when they are unknown", () => {
    expect(formatUsd(3)).toBe("$3");
    expect(formatUsd(2.5)).toBe("$2.5");
    expect(formatUsd(0.15)).toBe("$0.15");
    expect(formatUsd(0.075)).toBe("$0.075");
    expect(formatUsd(0)).toBe("$0");
    expect(costText({ costInput: 3, costOutput: 15 })).toBe("$3 in / $15 out per 1M tokens");
    expect(costText({ costInput: null, costOutput: null })).toBe("price unknown");
    expect(costText({ costInput: 1, costOutput: null })).toBe(
      "$1 in / output price unknown per 1M tokens",
    );
  });

  it("formats context sizes", () => {
    expect(contextText(200_000)).toBe("200K context");
    expect(contextText(1_000_000)).toBe("1M context");
    expect(contextText(1_048_576)).toBe("1M context");
    expect(contextText(2_500_000)).toBe("2.5M context");
    expect(contextText(512)).toBe("512 tokens context");
    expect(contextText(null)).toBe("context size unknown");
  });

  it("lists the facts of a row", () => {
    expect(modelFacts(MODELS[1]!)).toBe(
      "context size unknown · $3 in / $15 out per 1M tokens · supports tools · reasoning · released 2026-06-10",
    );
    expect(modelFacts(MODELS[3]!)).toContain("release date unknown");
  });

  it("counts with the right plural", () => {
    expect(countOf(1, "model")).toBe("1 model");
    expect(countOf(0, "provider")).toBe("0 providers");
    expect(countOf(235, "provider")).toBe("235 providers");
  });

  it("counts what is shown", () => {
    expect(catalogCountText(4, 4)).toBe("4 models");
    expect(catalogCountText(1, 1)).toBe("1 model");
    expect(catalogCountText(2, 4)).toBe("2 of 4 models");
    expect(catalogCountText(0, 4)).toBe("No models match these filters (out of 4).");
    expect(catalogCountText(0, 0)).toBe("This provider has no chat models in the list.");
  });

  it("says when the list was loaded or checked", () => {
    const at = new Date(2026, 10, 7, 10, 42).toISOString();
    const later = new Date(2026, 10, 7, 11, 5).toISOString();
    expect(loadedAtText({ loadedAt: at, checkedAt: at })).toBe("Loaded at 10:42");
    expect(loadedAtText({ loadedAt: at, checkedAt: later })).toBe(
      "Loaded at 10:42, checked at 11:05: no changes",
    );
    expect(loadedAtText({ loadedAt: "garbage", checkedAt: "garbage" })).toBe(
      "Loaded in this app run",
    );
  });

  it("finds providers by name or id, sorted by name", () => {
    const ps = [
      { id: "openrouter", name: "OpenRouter", modelCount: 300 },
      { id: "deepseek", name: "DeepSeek", modelCount: 2 },
      { id: "x-ai", name: "xAI", modelCount: 5 },
    ];
    expect(filterProviders(ps, "").map((p) => p.id)).toEqual(["deepseek", "openrouter", "x-ai"]);
    expect(filterProviders(ps, "ROUTER").map((p) => p.id)).toEqual(["openrouter"]);
    expect(filterProviders(ps, "x-a").map((p) => p.id)).toEqual(["x-ai"]);
    expect(filterProviders(ps, "zzz")).toEqual([]);
  });

  it("keeps the backend's fixed sentence and words the rest", () => {
    expect(catalogErrorText({ kind: "offline", message: "Can't reach models.dev." })).toBe(
      "Can't reach models.dev.",
    );
    expect(catalogErrorText({ kind: "unavailable", message: "" })).toBe(
      "The model list isn't available in this build.",
    );
    expect(catalogErrorText({ kind: "internal", message: "x" })).toContain("inside Lullshift");
    expect(catalogErrorText({ kind: "timeout" })).toContain("inside Lullshift");
  });
});
