// What the "Task context" consent dialog says is kept (ADR-013). Exported so a test can check
// that every captured field is named.

export const CONSENT_ITEMS: readonly string[] = [
  "The first prompt of each session, as its intent (up to 2,000 characters).",
  "What Claude Code asks you: its question, or for a permission only the tool name (up to 300 characters).",
  "Strings that look like secrets are hidden. That is a safety net, not a guarantee.",
  "Never tool inputs, commands, files, paths or transcripts.",
  "Kept on this device until you clear it or delete the session.",
];

export const CONSENT_SWITCH_DESCRIPTION =
  "Keep the first prompt of each session and what Claude Code asks, so you can pick up where you left off. Off by default.";
