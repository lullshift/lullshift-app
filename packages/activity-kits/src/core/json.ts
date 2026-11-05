// Plain JSON helpers. Saved state comes back from the host as untrusted data, so kits check its
// shape before using it and copy it so later edits never alias the saved value.

/** True for JSON values (finite numbers, strings, booleans, null, plain arrays and objects). */
export function isJson(value: unknown, depth = 0): boolean {
  if (depth > 32) return false;
  if (value === null) return true;
  switch (typeof value) {
    case "string":
    case "boolean":
      return true;
    case "number":
      return Number.isFinite(value);
    case "object":
      if (Array.isArray(value)) return value.every((v) => isJson(v, depth + 1));
      if (Object.getPrototypeOf(value) !== Object.prototype) return false;
      return Object.keys(value).every(
        (k) => k !== "__proto__" && isJson((value as Record<string, unknown>)[k], depth + 1),
      );
    default:
      return false;
  }
}

/** A deep copy of a JSON value. */
export function cloneJson<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T);
}

/** True when `value` is an object that is not an array or null. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** True for an integer in [min, max]. */
export function isIntIn(value: unknown, min: number, max: number): value is number {
  return Number.isInteger(value) && (value as number) >= min && (value as number) <= max;
}
