import { describe, expect, it } from "vitest";
import { ERROR_KINDS, type Scope } from "./api";
import {
  baseUrlHint,
  baseUrlProblem,
  budgetView,
  buildScope,
  canSend,
  charCountText,
  EMPTY_SCOPE,
  historyCount,
  historyText,
  isContextChanged,
  MAX_HISTORY_MESSAGES,
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

  it("builds scopes only when complete", () => {
    expect(buildScope(EMPTY_SCOPE)).toEqual({ kind: "none" });
    expect(buildScope({ ...EMPTY_SCOPE, kind: "sessionSummary" })).toBeNull();
    expect(
      buildScope({ ...EMPTY_SCOPE, kind: "sessionSummary", provider: "claude", sessionId: "s1" }),
    ).toEqual({ kind: "sessionSummary", provider: "claude", sessionId: "s1" });
    const base = { ...EMPTY_SCOPE, kind: "selectedNotes" as const, provider: "c", sessionId: "s" };
    expect(buildScope(base)).toBeNull();
    expect(buildScope({ ...base, itemIds: [3, 1] })).toEqual({
      kind: "selectedNotes",
      provider: "c",
      sessionId: "s",
      notes: false,
      itemIds: [1, 3],
    });
  });

  it("compares scopes regardless of item order", () => {
    const a: Scope = {
      kind: "selectedNotes",
      provider: "c",
      sessionId: "s",
      notes: true,
      itemIds: [2, 1],
    };
    const b: Scope = { ...a, itemIds: [1, 2] };
    expect(scopeKey(a)).toBe(scopeKey(b));
    expect(scopeKey(a)).not.toBe(scopeKey({ ...b, notes: false }));
  });
});

describe("preview gating", () => {
  const summary: Scope = { kind: "sessionSummary", provider: "c", sessionId: "s" };
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
    const other: Scope = { ...summary, sessionId: "s2" };
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
