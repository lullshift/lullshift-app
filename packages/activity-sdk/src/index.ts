// @lullshift/activity-sdk: everything a creator or a host needs to speak bridge protocol v1
// (ADR-012, ADR-017). Activities themselves load `bridge/lullshift-bridge-v1.js` as a plain
// script; this module is for tools, hosts and TypeScript types.

import type { ActivityManifestV1, Help } from "@lullshift/agent-protocol";

/** Version of this SDK. Manifests built with it declare `"sdkVersion": SDK_VERSION`. */
export const SDK_VERSION = "0.2.0";
/** Bridge protocol version spoken by this SDK. */
export const BRIDGE_PROTOCOL = 1;
/** File name of the bridge client that every activity carries. */
export const BRIDGE_FILE = "lullshift-bridge-v1.js";

export type { ActivityManifestV1, Help };
export type HelpControl = NonNullable<Help["controls"]>[number];

export {
  MESSAGE_MAX,
  PROTOCOL,
  SUMMARY_ITEMS_MAX,
  SUMMARY_TEXT_MAX,
  VALUE_MAX,
  isPlainText,
  parseSummary,
  type SummaryItem,
} from "./host/protocol.ts";
