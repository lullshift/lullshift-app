/* Generated from reference/AGENT_EVENT_V1.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */

export interface AgentEventV1 {
  schemaVersion: 1;
  /**
   * Lowercase hyphenated UUID.
   */
  eventId: string;
  /**
   * Lowercase provider slug, e.g. claude-code.
   */
  provider: string;
  /**
   * Opaque provider session id. No control, bidi, zero-width or other invisible formatting characters.
   */
  providerSessionId: string;
  eventType:
    | "session_started"
    | "working"
    | "needs_attention"
    | "turn_ended"
    | "completed"
    | "failed"
    | "cancelled"
    | "session_ended"
    | "heartbeat";
  /**
   * Strict RFC 3339: T separator, seconds 00-59, 1-9 fractional digits, Z or +HH:MM/-HH:MM.
   */
  timestampUtc: string;
  /**
   * Opaque correlation id. No control, bidi, zero-width or other invisible formatting characters.
   */
  correlationId?: string;
  /**
   * @maxItems 32
   */
  capabilities?: string[];
  /**
   * Small, non-sensitive hints only. Scalar values; never prompts, code or task content.
   */
  metadata?: {
    [k: string]: string | number | boolean | null | undefined;
  };
}
