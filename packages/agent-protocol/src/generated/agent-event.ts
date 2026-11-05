/* Generated from reference/AGENT_EVENT_V1.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */

export interface AgentEventV1 {
  schemaVersion: 1;
  /**
   * Lowercase hyphenated UUID.
   */
  eventId: string;
  provider: string;
  providerSessionId: string;
  eventType:
    | "session_started"
    | "working"
    | "needs_attention"
    | "turn_ended"
    | "completed"
    | "failed"
    | "cancelled"
    | "heartbeat";
  /**
   * Strict RFC 3339: T separator, seconds 00-59, 1-9 fractional digits, Z or +HH:MM/-HH:MM.
   */
  timestampUtc: string;
  correlationId?: string;
  /**
   * @maxItems 32
   */
  capabilities?: string[];
  metadata?: {};
}
