/* Generated from reference/*.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */

/** reference/AGENT_EVENT_V1.schema.json */
export const agentEventSchema: object = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: "urn:lullshift:schema:agent-event:v1",
  title: "Lullshift Agent Event v1",
  type: "object",
  additionalProperties: false,
  required: [
    "schemaVersion",
    "eventId",
    "provider",
    "providerSessionId",
    "eventType",
    "timestampUtc",
  ],
  properties: {
    schemaVersion: { const: 1 },
    eventId: {
      type: "string",
      description: "Lowercase hyphenated UUID.",
      pattern: "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
    },
    provider: {
      type: "string",
      description: "Lowercase provider slug, e.g. claude-code.",
      pattern: "^[a-z0-9][a-z0-9._-]{0,79}$",
    },
    providerSessionId: {
      type: "string",
      description:
        "Opaque provider session id. No control, bidi, zero-width or other invisible formatting characters.",
      minLength: 1,
      maxLength: 256,
      pattern:
        "^[^\\u0000-\\u001F\\u007F-\\u009F\\u061C\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2069\\uFEFF\\uFFF9-\\uFFFB]+$",
    },
    eventType: {
      enum: [
        "session_started",
        "working",
        "needs_attention",
        "turn_ended",
        "completed",
        "failed",
        "cancelled",
        "session_ended",
        "heartbeat",
      ],
    },
    timestampUtc: {
      type: "string",
      format: "date-time",
      description:
        "Strict RFC 3339: T separator, seconds 00-59, 1-9 fractional digits, Z or +HH:MM/-HH:MM.",
      pattern:
        "^[0-9]{4}-[0-9]{2}-[0-9]{2}T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\\.[0-9]{1,9})?(Z|[+-]([01][0-9]|2[0-3]):[0-5][0-9])$",
    },
    correlationId: {
      type: "string",
      description:
        "Opaque correlation id. No control, bidi, zero-width or other invisible formatting characters.",
      minLength: 1,
      maxLength: 256,
      pattern:
        "^[^\\u0000-\\u001F\\u007F-\\u009F\\u061C\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2069\\uFEFF\\uFFF9-\\uFFFB]+$",
    },
    capabilities: {
      type: "array",
      maxItems: 32,
      uniqueItems: true,
      items: { type: "string", pattern: "^[a-z][a-z0-9_.-]{0,63}$" },
    },
    metadata: {
      type: "object",
      description:
        "Small, non-sensitive hints only. Scalar values; never prompts, code or task content.",
      maxProperties: 20,
      propertyNames: { pattern: "^[A-Za-z][A-Za-z0-9_.-]{0,63}$" },
      additionalProperties: {
        oneOf: [
          {
            type: "string",
            maxLength: 256,
            pattern:
              "^[^\\u0000-\\u001F\\u007F-\\u009F\\u061C\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2069\\uFEFF\\uFFF9-\\uFFFB]*$",
          },
          { type: "number" },
          { type: "boolean" },
          { type: "null" },
        ],
      },
    },
  },
};

/** reference/ACTIVITY_MANIFEST_V1.schema.json */
export const activityManifestSchema: object = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: "urn:lullshift:schema:activity-manifest:v1",
  title: "Lullshift Activity Manifest v1",
  description:
    "manifest.json at the root of every activity folder or .lullpack. kind=app is a sandboxed static web app; kind=content is code-free data for a named engine (ADR-003).",
  type: "object",
  additionalProperties: false,
  required: [
    "schemaVersion",
    "kind",
    "id",
    "version",
    "name",
    "category",
    "minHostVersion",
    "sdkVersion",
    "permissions",
    "offline",
  ],
  properties: {
    schemaVersion: { const: 1 },
    kind: { enum: ["app", "content"] },
    id: {
      description: "Reverse-DNS style identifier, unique within a marketplace.",
      type: "string",
      pattern: "^[a-z0-9]+(\\.[a-z0-9-]+)+$",
      maxLength: 160,
    },
    version: { type: "string", pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+(?:-[0-9A-Za-z.-]+)?$" },
    name: {
      type: "string",
      minLength: 1,
      maxLength: 100,
      pattern:
        "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
      description: "Plain text, shown as the activity's title.",
    },
    description: {
      type: "string",
      maxLength: 500,
      pattern:
        "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
    },
    author: {
      description:
        "Self-declared and unverified (there are no accounts). The install UI shows the source repository as the trust signal.",
      type: "string",
      maxLength: 100,
      pattern:
        "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
    },
    license: {
      description: "SPDX license expression for the activity's code and content.",
      type: "string",
      maxLength: 100,
    },
    category: { enum: ["arcade", "break", "productivity", "interactive"] },
    tags: {
      type: "array",
      maxItems: 10,
      uniqueItems: true,
      items: { type: "string", pattern: "^[a-z0-9-]{1,30}$" },
    },
    minHostVersion: {
      description: "Oldest Lullshift version that can run this activity (semver).",
      $ref: "#/$defs/semver",
    },
    sdkVersion: {
      description: "Activity SDK version the activity was built against (semver).",
      $ref: "#/$defs/semver",
    },
    permissions: { type: "array", uniqueItems: true, items: { enum: ["activity.storage"] } },
    offline: { const: true },
    entrypoint: { const: "index.html" },
    runtime: { const: "isolated-web-v1" },
    interruptibility: {
      type: "object",
      additionalProperties: false,
      required: ["pause", "resume", "persistState"],
      properties: {
        pause: { const: true },
        resume: { const: true },
        persistState: { const: true },
      },
    },
    engine: {
      description: "Engine id and major version, e.g. lullshift.quiz@1.",
      type: "string",
      pattern: "^[a-z0-9]+(\\.[a-z0-9-]+)+@[0-9]+$",
    },
    data: {
      description:
        "Relative path to the engine data file inside the activity folder, validated against the engine's data schema. Segments start with a letter, digit or underscore, so `.`, `..`, `.git`, hidden names and a leading `-` are impossible.",
      type: "string",
      pattern:
        "^(?:[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}/){0,7}[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}\\.json$",
      maxLength: 256,
    },
    help: { $ref: "#/$defs/help" },
    images: {
      description:
        "Content packs only: pictures the engine may show (ADR-016, Sprint 12). Each is a PNG or WebP file directly in the pack's images/ folder, at most 512 KiB and 2048 x 2048 pixels, still (not animated) and without metadata or a colour profile. The pack's data refers to an image by this exact path, with alt text. The host checks and decodes every image before it is installed or served.",
      type: "array",
      minItems: 1,
      maxItems: 20,
      uniqueItems: true,
      items: {
        type: "string",
        maxLength: 80,
        pattern: "^images/[a-z0-9][a-z0-9-]{0,63}\\.(?:png|webp)$",
      },
    },
    preview: {
      description:
        "Relative path to a preview image (PNG, JPEG, WebP or SVG, at most 256 KiB), shown on the activity card and the paused view. The host shows it only as an image.",
      type: "string",
      maxLength: 256,
      pattern:
        "^(?:[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}/){0,7}[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}\\.(?:png|jpg|jpeg|webp|svg)$",
    },
  },
  allOf: [
    {
      if: { properties: { kind: { const: "app" } }, required: ["kind"] },
      then: {
        required: ["entrypoint", "runtime", "interruptibility"],
        properties: { engine: false, data: false, images: false },
      },
    },
    {
      if: { properties: { kind: { const: "content" } }, required: ["kind"] },
      then: {
        required: ["engine", "data"],
        properties: { entrypoint: false, runtime: false, interruptibility: false },
      },
    },
  ],
  $defs: {
    semver: {
      type: "string",
      maxLength: 64,
      pattern: "^[0-9]{1,5}\\.[0-9]{1,5}\\.[0-9]{1,5}(?:-[0-9A-Za-z.-]{1,40})?$",
    },
    help: {
      description:
        "How-to-play help, rendered by the host in its own panel (S05-T07). Plain text only: no control, bidirectional-override or invisible formatting characters, never HTML or Markdown.",
      type: "object",
      additionalProperties: false,
      required: ["summary", "howToPlay"],
      properties: {
        summary: {
          description: "One or two sentences: what this is.",
          type: "string",
          minLength: 1,
          maxLength: 200,
          pattern:
            "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
        },
        goal: {
          description: "What counts as done or winning, if anything.",
          type: "string",
          minLength: 1,
          maxLength: 300,
          pattern:
            "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
        },
        howToPlay: {
          description: "Steps, in order.",
          type: "array",
          minItems: 1,
          maxItems: 8,
          items: {
            type: "string",
            minLength: 1,
            maxLength: 300,
            pattern:
              "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
          },
        },
        controls: {
          description: "Keyboard and pointer controls.",
          type: "array",
          maxItems: 12,
          items: {
            type: "object",
            additionalProperties: false,
            required: ["keys", "action"],
            properties: {
              keys: {
                description:
                  'Key names as shown to the user, e.g. "Arrow keys" or "Enter / Space".',
                type: "string",
                minLength: 1,
                maxLength: 40,
                pattern:
                  "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
              },
              action: {
                type: "string",
                minLength: 1,
                maxLength: 120,
                pattern:
                  "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
              },
            },
          },
        },
        tips: {
          type: "array",
          maxItems: 6,
          items: {
            type: "string",
            minLength: 1,
            maxLength: 200,
            pattern:
              "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
          },
        },
      },
    },
  },
};

/** reference/MARKETPLACE_V1.schema.json */
export const marketplaceSchema: object = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: "urn:lullshift:schema:marketplace:v1",
  title: "Lullshift Marketplace Index v1",
  description:
    ".lullshift/marketplace.json at the root of a Git repository that acts as a Lullshift marketplace (ADR-002). Each entry points to an activity folder containing manifest.json, either in this repository or in another Git repository.",
  type: "object",
  additionalProperties: false,
  required: ["schemaVersion", "name", "activities"],
  properties: {
    schemaVersion: { const: 1 },
    name: {
      description:
        "Short marketplace name shown in the UI and used in commands. 'lullshift' is reserved for the built-in marketplace.",
      type: "string",
      pattern: "^[a-z0-9][a-z0-9-]{1,39}$",
    },
    description: {
      type: "string",
      maxLength: 500,
      pattern:
        "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
    },
    owner: {
      description: "Self-declared, unverified. Plain text.",
      type: "string",
      maxLength: 100,
      pattern:
        "^(?![\\s\\S]*[\\u0300-\\u036F\\u0483-\\u0489\\u0591-\\u05BD\\u0610-\\u061A\\u064B-\\u065F\\u1AB0-\\u1AFF\\u1DC0-\\u1DFF\\u20D0-\\u20FF\\uFE20-\\uFE2F]{5})[^\\u0000-\\u001F\\u007F-\\u009F\\u00AD\\u034F\\u061C\\u115F-\\u1160\\u17B4-\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u2028-\\u202E\\u2060-\\u2064\\u2066-\\u206F\\u3164\\uFEFF\\uFFA0\\uFFF9-\\uFFFB\\u{1BCA0}-\\u{1BCA3}\\u{1D173}-\\u{1D17A}\\u{E0000}-\\u{E007F}]*$",
    },
    activities: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/entry" } },
  },
  $defs: {
    relativePath: {
      description:
        "A `/`-separated path of at most 12 parts. Each part starts with a letter, digit or underscore (so no empty, `.`, `..`, `.git` or leading `-`), has at most 64 characters from letters, digits, `.`, `_`, `-`, does not end with `.`, and is not a Windows reserved device name (CON, PRN, AUX, NUL, COM1-9, LPT1-9, with or without an extension). Enforced again by the Rust marketplace crate.",
      type: "string",
      pattern:
        "^(?!(?:[Cc][Oo][Nn]|[Pp][Rr][Nn]|[Aa][Uu][Xx]|[Nn][Uu][Ll]|[Cc][Oo][Mm][1-9]|[Ll][Pp][Tt][1-9])(?:\\.|/|$))[A-Za-z0-9_](?:[A-Za-z0-9._-]{0,62}[A-Za-z0-9_-])?(?:/(?!(?:[Cc][Oo][Nn]|[Pp][Rr][Nn]|[Aa][Uu][Xx]|[Nn][Uu][Ll]|[Cc][Oo][Mm][1-9]|[Ll][Pp][Tt][1-9])(?:\\.|/|$))[A-Za-z0-9_](?:[A-Za-z0-9._-]{0,62}[A-Za-z0-9_-])?){0,11}$",
      maxLength: 300,
    },
    entry: {
      type: "object",
      additionalProperties: false,
      required: ["id", "source"],
      properties: {
        id: {
          description: "Must equal the id in the referenced manifest.json.",
          type: "string",
          pattern: "^[a-z0-9]+(\\.[a-z0-9-]+)+$",
          maxLength: 160,
        },
        source: {
          oneOf: [
            {
              description: "Folder in this repository.",
              type: "object",
              additionalProperties: false,
              required: ["path"],
              properties: { path: { $ref: "#/$defs/relativePath" } },
            },
            {
              description:
                "Folder in another Git repository. 'ref' is a tag, branch or commit; the installed version is always pinned to the resolved commit SHA.",
              type: "object",
              additionalProperties: false,
              required: ["git"],
              properties: {
                git: {
                  description:
                    "https://host/path or GitHub shorthand owner/repo. https only: an activity source can't use ssh, because only an https fetch can be pinned to the addresses Lullshift checked (ssh stays available for marketplace addresses the user types). No userinfo, no option-looking host, owner or path part, no `..` or `.git` path part (a final `repo.git` is fine), no control characters or whitespace, and no localhost, IP literal or local-network host name.",
                  type: "string",
                  pattern:
                    "^(?:https://(?![A-Za-z0-9.-]*\\.(?:[Ll][Oo][Cc][Aa][Ll][Hh][Oo][Ss][Tt]|[Ll][Oo][Cc][Aa][Ll]|[Ii][Nn][Tt][Ee][Rr][Nn][Aa][Ll]|[Ll][Aa][Nn]|[Ll][Oo][Cc][Aa][Ll][Dd][Oo][Mm][Aa][Ii][Nn]|[Ii][Nn][Tt][Rr][Aa][Nn][Ee][Tt]|[Cc][Oo][Rr][Pp]|[Hh][Oo][Mm][Ee]|[Pp][Rr][Ii][Vv][Aa][Tt][Ee]|[Aa][Rr][Pp][Aa])(?::|/|$))(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\\.)+[A-Za-z][A-Za-z0-9-]{0,61}[A-Za-z0-9](?::[1-9][0-9]{0,4})?(?:/[A-Za-z0-9_][A-Za-z0-9._-]{0,99}){1,10}|[A-Za-z0-9][A-Za-z0-9-]{0,38}/[A-Za-z0-9_][A-Za-z0-9._-]{0,99})$",
                  maxLength: 300,
                },
                ref: { $ref: "#/$defs/gitRef" },
                path: { $ref: "#/$defs/relativePath" },
              },
            },
          ],
        },
        featured: { type: "boolean" },
      },
    },
    gitRef: {
      description:
        "A branch, tag or commit id. No leading '-', '..', '//', ':', '^', '~', '*', '?', '[', backslash, whitespace or '@{', no part starting with '.', no '.lock' suffix. At run time it must also pass `git check-ref-format --allow-onelevel`, or be a 40-hex commit id.",
      type: "string",
      pattern:
        "^(?!-)(?!/)(?!\\.)(?!.*\\.\\.)(?!.*//)(?!.*/$)(?!.*/\\.)(?!.*\\.$)(?!.*\\.lock(?:/|$))[A-Za-z0-9._/-]{1,200}$",
      maxLength: 200,
    },
  },
};
