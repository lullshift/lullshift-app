// Made-up sessions, notes and Companion history for the browser demo. Nothing here is real
// user data; the wording is written to read naturally in screenshots.

import type { SessionDetail, SessionItem } from "../context/api";
import type { Message, ProviderProfile, Thread } from "../companion/api";
import type { FoundSession } from "../importSessions";
import type { SessionView } from "../sessions";

const LOADED = Date.now();
export const ago = (minutes: number) => new Date(LOADED - minutes * 60_000).toISOString();

export interface DemoSession {
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
  {
    // Decision 35: the agent's program exited without saying goodbye.
    view: view("cursor", "e52d7a10", "ended", 95, "Rename the settings keys", {
      endedReason: "process_gone",
      lastKnownState: "ended",
    }),
    detail: detail({
      context: {
        userTitle: "Rename the settings keys",
        intent: { text: "Rename the old settings keys and keep a migration.", source: "user" },
      },
      timeline: [
        { timestampUtc: ago(140), eventType: "session_started", reason: null },
        { timestampUtc: ago(139), eventType: "working", reason: null },
        { timestampUtc: ago(96), eventType: "turn_ended", reason: null },
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
  monthlyTokenBudget: 500_000,
  isDefault: true,
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
): Message => ({
  id,
  threadId,
  role,
  text,
  attachedContext: null,
  scopeKind: "none",
  host: role === "assistant" ? PROFILE.host : null,
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
    "That's the contended case. Let the first refresh win and have the second tab re-read the new token instead of retrying its own refresh.",
    16,
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

// ---- Import sessions (decision 34): made-up sessions "found" in the agents' logs -----------

export const IMPORTABLE: FoundSession[] = [
  {
    provider: "claude-code",
    sessionId: "c41d9e07-5b2a-4f61-9a0e-3d7c8b1f2e55",
    project: "shop/checkout",
    projectPath: "/Users/sam/code/shop/checkout",
    title: "Split the payment form into steps",
    lastActivityUtc: ago(1),
    guess: "working",
    tracked: false,
  },
  {
    provider: "cursor",
    sessionId: "8a6f2c19-0d3e-4b7a-b5c1-92e4f7a03d68",
    project: "users-sam-code-docs-site",
    projectPath: "users-sam-code-docs-site",
    title: "Add a dark theme toggle",
    lastActivityUtc: ago(4),
    guess: "may_need_you",
    tracked: false,
  },
  {
    provider: "codex",
    sessionId: "019e1a2b-7c3d-7e4f-8a9b-0c1d2e3f4a5b",
    project: "infra/terraform",
    projectPath: "/Users/sam/code/infra/terraform",
    title: "Tidy the staging variables",
    lastActivityUtc: ago(38),
    guess: "waiting",
    tracked: false,
  },
  {
    provider: "claude-code",
    sessionId: "7f3a9c21-1111-4222-8333-444455556666",
    project: "auth/service",
    projectPath: "/Users/sam/code/auth/service",
    title: "Token refresh fix",
    lastActivityUtc: ago(60),
    guess: "waiting",
    tracked: true,
  },
  {
    provider: "opencode",
    sessionId: "ses_3kq8v2m0x1",
    project: "tools/cli",
    projectPath: "/Users/sam/code/tools/cli",
    title: null,
    lastActivityUtc: ago(60 * 26),
    guess: "unknown",
    tracked: false,
  },
];

const GUESS_STATE: Record<FoundSession["guess"], SessionView["state"]> = {
  working: "working",
  waiting: "idle",
  may_need_you: "needs_attention",
  unknown: "unknown",
};

/** The dashboard entry for a made-up session after the demo "imports" it. */
export function importedSession(f: FoundSession): DemoSession {
  const state = GUESS_STATE[f.guess];
  return {
    view: {
      ...view(f.provider, f.sessionId, state, 0, "", {
        userTitle: null,
        agentTitle: f.title,
        reason: "unconfirmed_after_restart",
        state: "unknown",
        lastKnownState: state,
        lastSeenUtc: f.lastActivityUtc,
        lastReceivedUtc: f.lastActivityUtc,
      }),
      importedUtc: new Date().toISOString(),
    },
    detail: detail({
      context: {
        agentTitle: f.title ? { text: f.title, capturedUtc: new Date().toISOString() } : null,
        hasCaptured: f.title !== null,
      },
    }),
  };
}
