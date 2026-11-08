// roundSize creator check (ADR-028 section 10, S13-T09).
import { describe, expect, it } from "vitest";
import {
  ROUND_ENGINES,
  ROUND_SIZE_MIN,
  roundSizeProblem,
  type CardsPackData,
  type QuizPackData,
} from "../src/index.ts";

const cards = (n: number, roundSize?: number): CardsPackData => ({
  title: "Deck",
  ...(roundSize === undefined ? {} : { roundSize }),
  cards: Array.from({ length: n }, (_, i) => ({ front: `Card ${i}` })),
});

describe("roundSizeProblem", () => {
  it("names the three round engines and the smallest size", () => {
    expect(Object.keys(ROUND_ENGINES).sort()).toEqual([
      "lullshift.cards@1",
      "lullshift.quiz@1",
      "lullshift.teaser@1",
    ]);
    expect(ROUND_SIZE_MIN).toBe(3);
  });

  it("is quiet when roundSize is absent or at most the item count", () => {
    expect(roundSizeProblem("lullshift.cards@1", cards(12))).toBeNull();
    expect(roundSizeProblem("lullshift.cards@1", cards(12, 6))).toBeNull();
    expect(roundSizeProblem("lullshift.cards@1", cards(12, 12))).toBeNull();
  });

  it("explains a roundSize above the item count in plain words", () => {
    const quiz: QuizPackData = {
      title: "Q",
      roundSize: 9,
      questions: Array.from({ length: 4 }, () => ({
        prompt: "p",
        choices: ["a", "b"],
        answer: 0,
        explanation: "e",
      })),
    };
    expect(roundSizeProblem("lullshift.quiz@1", quiz)).toBe(
      "roundSize 9 is more than the 4 questions, so every round shows all of them (set roundSize to at most 4, or leave it out)",
    );
    expect(roundSizeProblem("lullshift.cards@1", cards(5, 8))).toMatch(/more than the 5 cards/);
    expect(
      roundSizeProblem("lullshift.teaser@1", { title: "T", roundSize: 4, teasers: [] }),
    ).toMatch(/more than the 0 teasers/);
  });

  it("leaves other engines and malformed data to the schema", () => {
    expect(roundSizeProblem("lullshift.reflection@1", { roundSize: 9, prompts: [] })).toBeNull();
    expect(roundSizeProblem("lullshift.cards@2", cards(2, 9))).toBeNull();
    for (const data of [null, "x", [], { roundSize: "9", cards: [] }, { roundSize: 9 }]) {
      expect(roundSizeProblem("lullshift.cards@1", data)).toBeNull();
    }
  });
});
