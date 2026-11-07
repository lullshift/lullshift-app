// Made-up sessions, notes and Companion history for the browser demo. Nothing here is real
// user data; the wording is written to read naturally in screenshots.

import type { SessionDetail, SessionItem } from "../context/api";
import type { Message, ProviderProfile, Thread } from "../companion/api";
import type { SessionView } from "../sessions";

const LOADED = Date.now();
export const ago = (minutes: number) => new Date(LOADED - minutes * 60_000).toISOString();

interface DemoSession {
  view: SessionView;
  detail: SessionDetail;
}

function view(
  provider: string,
  id: string,
  state: SessionView["state"],
  minutes: number,
  userTitle: string,
  extra: Partial<SessionView> = {},
): SessionView {
  return {
    provider,
    providerSessionId: id,
    state,
    reason: "observed",
    lastKnownState: state,
    lastSeenUtc: ago(minutes),
    lastReceivedUtc: ago(minutes),
    needsReview: false,
    userTitle,
    agentTitle: null,
    intentLine: null,
    disconnectedUtc: null,
    ...extra,
  };
}

let nextItemId = 1;
function items(
  list: [SessionItem["kind"], string, Partial<Pick<SessionItem, "done" | "category">>?][],
): SessionItem[] {
  const positions: Record<string, number> = {};
  return list.map(([kind, text, extra]) => ({
    id: nextItemId++,
    kind,
    category: extra?.category ?? null,
    text,
    done: extra?.done ?? false,
    position: (positions[kind] = (positions[kind] ?? -1) + 1),
    createdUtc: ago(20),
  }));
}

function detail(
  partial: Omit<Partial<SessionDetail>, "context"> & { context: Partial<SessionDetail["context"]> },
): SessionDetail {
  return {
    items: partial.items ?? [],
    links: partial.links ?? [],
    timeline: partial.timeline ?? [],
    retentionDays: partial.retentionDays ?? 30,
    context: {
      intent: null,
      latestRequest: null,
      ask: null,
      notes: "",
      reviewedUtc: null,
      hasCaptured: false,
      userTitle: null,
      agentTitle: null,
      ...partial.context,
    },
  };
}

export const SESSIONS: DemoSession[] = [
  {
    view: view("claude-code", "7f3a9c21", "needs_attention", 1, "Token refresh fix"),
    detail: detail({
      context: {
        userTitle: "Token refresh fix",
        intent: {
          text: "Refresh expired tokens silently and sign the user out cleanly if refresh fails.",
          source: "user",
        },
        ask: { text: "Allow running the auth test suite (pnpm test auth)?", capturedUtc: ago(1) },
        hasCaptured: true,
        notes: "Check the two-tab case before merging.",
      },
      items: items([
        ["next_move", "Should the retry limit be a setting?", { category: "question" }],
        ["next_move", "An expired token signs the user out cleanly.", { category: "acceptance" }],
        ["next_move", "Two tabs refreshing the token at once.", { category: "test" }],
        ["decision", "Keep the refresh in the HTTP client, not in each screen."],
        ["checklist", "Refresh path has a test", { done: true }],
        ["checklist", "Sign-out clears the cached profile"],
      ]),
      timeline: [
        { timestampUtc: ago(24), eventType: "session_started", reason: null },
        { timestampUtc: ago(23), eventType: "working", reason: null },
        { timestampUtc: ago(9), eventType: "turn_ended", reason: null },
        { timestampUtc: ago(8), eventType: "working", reason: null },
        { timestampUtc: ago(1), eventType: "needs_attention", reason: "permission" },
      ],
    }),
  },
  {
    view: view("codex", "b81e04d6", "working", 2, "Flaky test sweep"),
    detail: detail({
      context: {
        userTitle: "Flaky test sweep",
        intent: { text: "Find and fix the three flaky integration tests.", source: "user" },
      },
      items: items([["checklist", "Run the suite 20 times after the fix"]]),
      timeline: [
        { timestampUtc: ago(40), eventType: "session_started", reason: null },
        { timestampUtc: ago(39), eventType: "working", reason: null },
      ],
    }),
  },
  {
    view: view("cursor", "c2d95f30", "working", 4, "Settings page copy"),
    detail: detail({
      context: { userTitle: "Settings page copy" },
      timeline: [
        { timestampUtc: ago(15), eventType: "session_started", reason: null },
        { timestampUtc: ago(14), eventType: "working", reason: null },
      ],
    }),
  },
  {
    view: view("claude-code", "4e6a1b8f", "ended", 6, "Move sessions to SQLite", {
      lastKnownState: "ended",
      needsReview: true,
    }),
    detail: detail({
      context: {
        userTitle: "Move sessions to SQLite",
        intent: {
          text: "Move session storage to SQLite without changing the public API.",
          source: "user",
        },
        notes: "Old JSON files stay as a backup for one release.",
      },
      items: items([
        ["decision", "One table per entity; no JSON blobs."],
        ["checklist", "Migration has a rollback test", { done: true }],
        ["checklist", "Old JSON files are imported once"],
      ]),
      timeline: [
        { timestampUtc: ago(55), eventType: "session_started", reason: null },
        { timestampUtc: ago(54), eventType: "working", reason: null },
        { timestampUtc: ago(7), eventType: "turn_ended", reason: null },
        { timestampUtc: ago(6), eventType: "session_ended", reason: null },
      ],
    }),
  },
  {
    view: view("codex", "91f0c7ae", "completed", 32, "README examples", {
      lastKnownState: "completed",
    }),
    detail: detail({
      context: { userTitle: "README examples", reviewedUtc: ago(30) },
      timeline: [
        { timestampUtc: ago(50), eventType: "session_started", reason: null },
        { timestampUtc: ago(32), eventType: "completed", reason: null },
      ],
    }),
  },
];

export const PROFILE: ProviderProfile = {
  id: "demo-anthropic",
  kind: "anthropic",
  label: "Anthropic",
  baseUrl: "https://api.anthropic.com",
  host: "api.anthropic.com",
  model: "claude-sonnet-5-5",
  models: [
    { model: "claude-sonnet-5-5", label: null, isDefault: true },
    { model: "claude-haiku-5-5", label: "Haiku (quick answers)", isDefault: false },
  ],
  monthlyTokenBudget: 500_000,
  isDefault: true,
  hasKey: true,
  keychainOk: true,
};

/** A second, local provider, so the demo shows models grouped by provider. */
export const LOCAL_PROFILE: ProviderProfile = {
  id: "demo-local",
  kind: "openaiCompatible",
  label: "Local server",
  baseUrl: "http://localhost:11434/v1",
  host: "localhost:11434",
  model: "llama3.3",
  models: [{ model: "llama3.3", label: null, isDefault: true }],
  monthlyTokenBudget: null,
  isDefault: false,
  hasKey: true,
  keychainOk: true,
};

export const THREADS: Thread[] = [
  {
    id: 1,
    title: "Optimistic locking or row locks",
    profileId: PROFILE.id,
    model: PROFILE.model,
    createdAt: ago(18),
    updatedAt: ago(16),
  },
  {
    id: 2,
    title: "Naming the retry setting",
    profileId: PROFILE.id,
    model: PROFILE.model,
    createdAt: ago(300),
    updatedAt: ago(290),
  },
];

const msg = (
  id: number,
  threadId: number,
  role: Message["role"],
  text: string,
  minutes: number,
  model: string = PROFILE.model,
): Message => ({
  id,
  threadId,
  role,
  text,
  attachedContext: null,
  scopeKind: "none",
  host: role === "assistant" ? PROFILE.host : null,
  profileId: PROFILE.id,
  model,
  status: "sent",
  error: null,
  inputTokens: role === "assistant" ? 412 : null,
  outputTokens: role === "assistant" ? 186 : null,
  createdAt: ago(minutes),
  toolCalls: [],
  toolNotices: [],
});

export const MESSAGES: Message[] = [
  msg(
    1,
    1,
    "user",
    "The agent switched the session store to optimistic locking. When does that beat a row lock?",
    18,
  ),
  msg(
    2,
    1,
    "assistant",
    "When conflicts are rare. Each write carries the version it read, and the update only succeeds if that version is still current. Nobody holds a lock while thinking, so readers never wait.\n\nThe cost shows up under contention: every collision means a retry. For a session store where one person edits one session at a time, collisions are rare, so optimistic locking is a good fit. If two background jobs often update the same row, a short row lock is simpler.",
    17,
  ),
  msg(3, 1, "user", "So for two tabs refreshing the same token?", 16),
  msg(
    4,
    1,
    "assistant",
    "That's the contended case. Let the first refresh win and have the second tab re-read the new token instead of retrying its own refresh:\n\n```ts\nconst fresh = await store.read(sessionId);\nif (fresh.version !== seen.version) {\n  return fresh.token; // another tab already refreshed\n}\n```\n\nThe MDN page on the Web Locks API covers a browser-only alternative: https://developer.mozilla.org/docs/Web/API/Web_Locks_API",
    16,
    "claude-haiku-5-5",
  ),
  msg(5, 2, "user", "Is maxRefreshAttempts or refreshRetryLimit clearer?", 300),
  msg(
    6,
    2,
    "assistant",
    "maxRefreshAttempts reads better: it says what is counted and that it is a ceiling.",
    290,
  ),
];
