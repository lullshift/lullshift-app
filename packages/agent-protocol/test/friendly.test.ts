// Plain-language schema errors (S07-A03). The wording matches the desktop host's Rust check
// (apps/desktop/src-tauri/src/data_schema.rs), so creators see the same message everywhere.
import { describe, expect, it } from "vitest";
import { createAjv, dataPath, friendlyErrors } from "../src/index";

const PLAIN = "^[^\\u0000-\\u001F\u200b]*$";
const schema = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  type: "object",
  additionalProperties: false,
  required: ["title", "cards"],
  properties: {
    title: { type: "string", minLength: 1, maxLength: 10, pattern: PLAIN },
    level: { enum: ["easy", "hard"] },
    count: { type: "integer", minimum: 1, maximum: 5 },
    cards: {
      type: "array",
      minItems: 1,
      maxItems: 2,
      items: {
        type: "object",
        additionalProperties: false,
        required: ["front"],
        properties: { front: { type: "string", maxLength: 5 } },
      },
    },
  },
};

const check = (data: unknown, file?: string) => {
  const validate = createAjv({ allErrors: true }).compile(schema);
  return validate(data) ? [] : friendlyErrors(validate.errors, file);
};

describe("friendlyErrors", () => {
  it("writes data paths the way people do", () => {
    expect(dataPath("")).toBe("");
    expect(dataPath("/cards/3/front")).toBe("cards[3].front");
    expect(dataPath("/a~1b/x y")).toBe('["a/b"]["x y"]');
    expect(dataPath("/we\u202eird")).toBe('["we?ird"]');
    expect(dataPath("/two\u2028lines\u2029")).toBe('["two?lines?"]');
  });

  it("says what is wrong, where, in plain words", () => {
    const lines = check(
      {
        title: "Line\nbreak!!",
        level: "medium",
        count: 9,
        cards: [{ front: "ok" }, { front: "too long" }, { front: "x" }],
        colour: "red",
      },
      "deck.json",
    );
    for (const expected of [
      "deck.json: title is longer than 10 characters",
      "deck.json: title contains a line break, tab or invisible character, which isn't allowed",
      'deck.json: level should be one of: "easy", "hard"',
      "deck.json: count should be at most 5",
      "deck.json: cards has more than 2 items",
      "deck.json: cards[1].front is longer than 5 characters",
      "deck.json: colour isn't a known field",
    ]) {
      expect(lines).toContain(expected);
    }
    expect(lines.join("\n")).not.toMatch(/must NOT|instancePath|\/cards\//);
  });

  it("names missing fields and wrong types", () => {
    expect(check({ cards: [{}] })).toEqual(["title is missing", "cards[0].front is missing"]);
    expect(check([])).toEqual(["the data should be an object"]);
    expect(check({ title: "", cards: [] })).toEqual([
      "title is empty",
      "cards needs at least 1 item",
    ]);
  });
});
