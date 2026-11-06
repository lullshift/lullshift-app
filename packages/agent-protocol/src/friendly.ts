// Plain-language schema errors for creators (ADR-016, S07-A03): "deck.json: cards[3].front is
// longer than 200 characters" instead of Ajv's "/cards/3/front must NOT have more than 200
// characters". Used by `lullshift validate` and the engine compatibility check. The desktop host
// words its own (Rust) check the same way; keep `data_schema.rs` in step with this file.
import type { ErrorObject } from "ajv";

/** `/cards/3/front` → `cards[3].front`. Odd field names are quoted and cleaned (they're data). */
export function dataPath(instancePath: string): string {
  if (!instancePath) return "";
  let out = "";
  for (const raw of instancePath.slice(1).split("/")) {
    const seg = raw.replaceAll("~1", "/").replaceAll("~0", "~");
    if (/^(0|[1-9][0-9]*)$/.test(seg)) out += `[${seg}]`;
    else if (/^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(seg)) out += out ? `.${seg}` : seg;
    else out += `["${cleanKey(seg)}"]`;
  }
  return out;
}

// Control, bidirectional and invisible characters (as in the engine schemas' plain text, which
// also refuse the line and paragraph separators U+2028 and U+2029).
const HIDDEN =
  // eslint-disable-next-line no-control-regex
  /[\u0000-\u001F\u007F-\u009F\u00AD\u061C\u180E\u200B-\u200F\u2028\u2029\u202A-\u202E\u2060-\u2064\u2066-\u206F\uFEFF"]/gu;

function cleanKey(key: string): string {
  return [...key].slice(0, 40).join("").replace(HIDDEN, "?");
}

const TYPE_WORD: Record<string, string> = {
  string: "text",
  number: "a number",
  integer: "a whole number",
  boolean: "true or false",
  array: "a list",
  object: "an object",
  null: "null",
};

const child = (at: string, name: string) =>
  dataPath(`${at}/${name.replaceAll("~", "~0").replaceAll("/", "~1")}`);

/** One Ajv error in plain words, or null for errors that only repeat another (`if`). */
export function friendlyError(e: ErrorObject): string | null {
  const p = e.params as Record<string, unknown>;
  const at = dataPath(e.instancePath);
  const subject = at || "the data";
  const say = (message: string, where = subject) => `${where} ${message}`;
  switch (e.keyword) {
    case "required":
      return say("is missing", child(e.instancePath, String(p.missingProperty)));
    case "additionalProperties":
      return say("isn't a known field", child(e.instancePath, String(p.additionalProperty)));
    case "type": {
      const types = String(p.type).split(",");
      return say(`should be ${types.map((t) => TYPE_WORD[t] ?? t).join(" or ")}`);
    }
    case "minLength":
      return say(p.limit === 1 ? "is empty" : `is shorter than ${String(p.limit)} characters`);
    case "maxLength":
      return say(`is longer than ${String(p.limit)} characters`);
    case "pattern":
      return say(
        String(p.pattern).startsWith("^[^\\u0000-\\u001F")
          ? "contains a line break, tab or invisible character, which isn't allowed"
          : "doesn't have the expected form",
      );
    case "minItems":
      return say(`needs at least ${String(p.limit)} ${p.limit === 1 ? "item" : "items"}`);
    case "maxItems":
      return say(`has more than ${String(p.limit)} items`);
    case "uniqueItems":
      return say("has duplicate items");
    case "minProperties":
      return say(`needs at least ${String(p.limit)} fields`);
    case "maxProperties":
      return say(`has more than ${String(p.limit)} fields`);
    case "enum": {
      const values = (p.allowedValues as unknown[]) ?? [];
      const shown = values.slice(0, 10).map((v) => JSON.stringify(v));
      return say(`should be one of: ${shown.join(", ")}${values.length > 10 ? ", …" : ""}`);
    }
    case "const":
      return say(`should be ${JSON.stringify(p.allowedValue)}`);
    case "minimum":
      return say(`should be at least ${String(p.limit)}`);
    case "maximum":
      return say(`should be at most ${String(p.limit)}`);
    case "exclusiveMinimum":
      return say(`should be more than ${String(p.limit)}`);
    case "exclusiveMaximum":
      return say(`should be less than ${String(p.limit)}`);
    case "multipleOf":
      return say(`should be a multiple of ${String(p.multipleOf)}`);
    case "anyOf":
      return say("doesn't match any of the allowed shapes");
    case "oneOf":
      return say(
        p.passingSchemas
          ? "matches more than one allowed shape"
          : "doesn't match any of the allowed shapes",
      );
    case "not":
      return say("has a form that isn't allowed");
    case "false schema":
      return say("isn't allowed");
    case "format":
      return say(`isn't a valid ${String(p.format)}`);
    case "if":
      return null;
    default:
      return say(e.message ?? "is invalid");
  }
}

/**
 * Ajv errors as plain, de-duplicated lines, each prefixed with `file: ` when given:
 * `deck.json: cards[3].front is longer than 200 characters`.
 */
export function friendlyErrors(
  errors: readonly ErrorObject[] | null | undefined,
  file?: string,
): string[] {
  const lines: string[] = [];
  for (const e of errors ?? []) {
    const text = friendlyError(e);
    if (!text) continue;
    const line = file ? `${file}: ${text}` : text;
    if (!lines.includes(line)) lines.push(line);
  }
  return lines;
}
