# Content style guide

How Lullshift's bundled content is written: engine text, content packs, help and previews (S07-T04). Third-party creators are encouraged to follow it too. It sits beside ADR-003 (play, not curriculum) and the activity contract (`.claude/skills/activity-contract/SKILL.md`).

## Who it is for

A developer waiting for an AI agent, for a few seconds or a few minutes. They may be interrupted at any moment and come back later. Content should be easy to drop and easy to pick up.

## Tone

- Calm, warm and plain. Short sentences, everyday words.
- Talk to the person as "you". Invite, don't instruct: "Try rolling your shoulders" rather than "Roll your shoulders now".
- No pressure: no streaks, no "don't break your chain", no guilt for stopping, no countdown to failure.
- No fake urgency, exclamation marks only rarely, no emoji in text.
- Humour is welcome when it is gentle and never at someone's expense.
- Neutral and inclusive. No assumptions about body, age, gender, culture, religion, family or ability.

## Health and body content

Breathing, stretching, movement and rest content is **general wellbeing play, not advice**.

- No medical, therapeutic or health-outcome claims. Not "reduces anxiety", "lowers blood pressure", "fixes back pain", "improves focus by 30%". Say what to do, not what it will cure.
- Always offer a way out: "Stop if anything feels uncomfortable", "Skip any that don't suit you".
- Movements must be doable seated or standing, in a small space, in work clothes, without equipment. Avoid anything that needs lying down, balance on one foot, or holding the breath for long.
- Breathing patterns stay gentle: no breath holds longer than 4 seconds, no hyperventilation patterns, nothing presented as treatment.
- Never mention weight, calories, diets or body shape.

## Play, not curriculum

Quizzes, teasers and typing drills are for fun.

- No courses, levels to unlock, progress tracks, grades, certificates or "mastery".
- Scores are private, local and optional to look at. Never compare the person with others.
- Wrong answers get a kind explanation, not a red mark of shame.
- Facts must be accurate and stable over time. Avoid fast-changing facts (prices, records, current office holders) and contested topics (politics, religion, health science disputes).

## Reflection prompts

- Prompts are open, light and safe at work: "What went better than expected today?", not "What are you most afraid of?".
- Never ask for personal data, names of colleagues, or details about health, finances or relationships.
- Anything the person types stays on their device, inside that pack's saved state, and can be cleared. Say so on screen.
- No prompts about productivity metrics or performance of other people (no employee surveillance).

## Originality and licensing

- All text is original to this repository and licensed Apache-2.0 with the code. No copied quotes, poems, lyrics, puzzles or trivia sets from books, sites or games.
- Quotes from real people are avoided (attribution and accuracy are hard to guarantee).
- No branded names, characters or trademarks, and no recreations of branded games or puzzles.
- Art is original and simple (SVG shapes). List any third-party asset and its license in the pack's README; bundled packs use none.

## Writing for screens and screen readers

- Plain text only. No markup, no ASCII art, no special Unicode symbols that screen readers read badly (arrows, decorative bullets). Write "Arrow keys", not arrow symbols.
- One idea per card or step. Keep each under about 200 characters where the schema allows more.
- Spell out numbers and units when read aloud would be ambiguous ("4 seconds", not "4s").
- Don't rely on colour alone to carry meaning.

## Help and descriptions

- Every pack has `help` in its manifest: a one-line summary, the goal (often "nothing to finish"), 2 to 5 how-to-play steps and any tips (including the health way out where relevant).
- The description says what it is in one or two sentences, not marketing ("A gentle five-minute stretch for your shoulders and neck").

## Checklist before adding a pack

- [ ] Original text; no medical claims; a way out for any body content.
- [ ] Fits the engine's data schema (`pnpm lullshift validate <folder>` passes with no warnings).
- [ ] `pnpm lullshift simulate <folder>` passes.
- [ ] Help, description and preview present; preview is original art.
- [ ] Listed in `.lullshift/marketplace.json`.
