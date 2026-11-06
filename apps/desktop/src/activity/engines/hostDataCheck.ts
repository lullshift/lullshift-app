// For engine tests: pack data the way the app judges it. The engine's data schema (Ajv, as the
// CLI runs it), then the host's plain-text rule on every string, which a schema pattern the app
// compiles can't fully express (security review LOW-2; crates/marketplace/src/pack_text.rs).
import { createAjv, dataPath } from "@lullshift/agent-protocol";
import { packTextProblems } from "@lullshift/activity-sdk";

export interface DataCheck {
  (data: unknown): boolean;
  /** Ajv's errors and then the text problems from the last call, or null if it passed. */
  errors: unknown[] | null;
}

export function hostDataCheck(schema: object): DataCheck {
  const schemaCheck = createAjv({ allErrors: true }).compile(schema);
  const check = ((data: unknown) => {
    const schemaOk = schemaCheck(data);
    const text = packTextProblems(data, dataPath);
    const errors = [...(schemaOk ? [] : (schemaCheck.errors ?? [])), ...text];
    check.errors = errors.length ? errors : null;
    return errors.length === 0;
  }) as DataCheck;
  check.errors = null;
  return check;
}
