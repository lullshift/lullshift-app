// ADR-028: ids under `lullshift.generated.` belong to sets the user's model writes. The host,
// the marketplace crate and this SDK say the same words.
import { describe, expect, it } from "vitest";
import {
  RESERVED_GENERATED_MESSAGE,
  RESERVED_GENERATED_PREFIX,
  isReservedGeneratedId,
} from "../src/index";

describe("reserved generated-set ids", () => {
  it("recognises the namespace and nothing that only looks like it", () => {
    expect(RESERVED_GENERATED_PREFIX).toBe("lullshift.generated.");
    expect(isReservedGeneratedId("lullshift.generated.quiz.abcdefghij12")).toBe(true);
    for (const id of [
      "lullshift.generatedx.quiz",
      "lullshift.generate.quiz",
      "com.lullshift.generated.quiz",
      "app.lullshift.quiz",
      "",
    ]) {
      expect(isReservedGeneratedId(id), id).toBe(false);
    }
  });

  it("has one fixed message that names the prefix", () => {
    expect(RESERVED_GENERATED_MESSAGE).toContain("lullshift.generated.");
    expect(RESERVED_GENERATED_MESSAGE).toBe(
      "activity ids starting with 'lullshift.generated.' are reserved for sets written by the user's own model, so this id can't be used",
    );
  });
});
