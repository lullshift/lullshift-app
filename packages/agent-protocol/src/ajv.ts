// The one Ajv setup for every Lullshift schema (S05-T06): the runtime validators in this package,
// the fixture script and the creator CLI all build their validators here, so they can never
// disagree about strictness, formats or regex flavour.
import { Ajv2020 } from "ajv/dist/2020.js";
import addFormatsModule from "ajv-formats";

// ajv-formats is CommonJS; normalise the default export across bundlers and Node.
const addFormats = ((addFormatsModule as unknown as { default?: typeof addFormatsModule })
  .default ?? addFormatsModule) as typeof addFormatsModule;

export interface AjvOptions {
  /** Report every problem (tools) rather than stop at the first (runtime). */
  allErrors?: boolean;
}

export function createAjv({ allErrors = false }: AjvOptions = {}): Ajv2020 {
  // `unicodeRegExp` (the default) compiles patterns with the `u` flag, which the plain-text
  // patterns rely on.
  const ajv = new Ajv2020({ allErrors, strict: true, strictRequired: false, unicodeRegExp: true });
  addFormats(ajv);
  return ajv;
}
