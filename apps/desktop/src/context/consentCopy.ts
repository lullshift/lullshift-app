// What the "Task context" consent dialog says is kept (ADR-013, ADR-021). Exported so a test can
// check that every captured field is named. Each tool has its own switch and its own copy.

import type { AgentId } from "../adapters";

const AGENT_NAME: Record<AgentId, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  cursor: "Cursor",
};

const SECRETS_ITEM =
  "Strings that look like secrets are hidden. That is a safety net, not a guarantee.";
const KEPT_ITEM = "Kept on this device until you clear it or delete the session.";

/** Everything the dialog lists for `agent`, in order. */
export function consentItems(agent: AgentId): readonly string[] {
  const first = "The first prompt of each session, as its intent (up to 2,000 characters).";
  if (agent === "claude-code") {
    return [
      first,
      "What Claude Code asks you: its question, or for a permission only the tool name (up to 300 characters).",
      SECRETS_ITEM,
      "Never tool inputs, commands, files, paths or transcripts.",
      KEPT_ITEM,
    ];
  }
  return [
    first,
    `Nothing else from ${AGENT_NAME[agent]}: not its replies, tool inputs, commands, files, paths, transcripts or your email.`,
    SECRETS_ITEM,
    KEPT_ITEM,
  ];
}

export function consentSwitchDescription(agent: AgentId): string {
  return agent === "claude-code"
    ? "Keep the first prompt of each session and what Claude Code asks, so you can pick up where you left off. Off by default."
    : `Keep the first prompt of each ${AGENT_NAME[agent]} session, so you can pick up where you left off. Off by default.`;
}

export const CONSENT_SCOPE_NOTE =
  "Task context is available for Claude Code, Codex and Cursor. Each tool has its own switch, off by default.";

export function agentDisplayName(agent: AgentId): string {
  return AGENT_NAME[agent];
}
