//! The refused-phrase check for body sets (ADR-028 section 2a, S13-T01b).
//!
//! After the schemas, every string the model wrote in a movement, breathing or pause set is
//! matched, case-folded and word by word, against [`REFUSED`] (and, for movement only,
//! [`REFUSED_MOVEMENT`]). A hit refuses the whole set. The list errs on the side of refusing: a
//! refused gentle set costs one retry, a harmful one reaching the user costs far more. It is
//! never loosened to let a set through; if ordinary gentle wording in a built-in pack is hit, the
//! phrase is made more specific instead (a test runs every built-in body pack through this check).

use serde_json::Value;

use super::EngineKind;

/// Phrases refused in every body set. Each entry is lower case, words separated by single
/// spaces; punctuation and hyphens in the text count as word breaks, apostrophes are ignored. A
/// trailing `*` on the last word matches any word that starts that way ("diagnos*" matches
/// "diagnose" and "diagnosis"); everything else is a whole word, so "treat" does not match
/// "treaty".
///
/// Some entries were made more specific after the built-in packs showed that ordinary gentle
/// wording used them: "as far as feels easy" and "feet on the floor" are fine, "as far as you can"
/// and "sit on the floor" are not; "lift your hands" is fine, "lift weights" is not; "gentle
/// squats" (a built-in energiser move) is fine, "deep squat" is not.
pub const REFUSED: &[&str] = &[
    // Breath holds and forced breathing.
    "hold your breath",
    "holding your breath",
    "hold the breath",
    "hold that breath",
    "breath hold*",
    "hyperventilat*",
    "breath of fire",
    "rapid breathing",
    "fast breathing",
    "breathe rapidly",
    "breathe as fast",
    // Pushing into pain or to the limit.
    "as far as you can",
    "as far as you are able",
    "as far as youre able",
    "as far as possible",
    "as far as it goes",
    "as far as it will go",
    "as much as you can",
    "as hard as you can",
    "as deep as you can",
    "as deeply as you can",
    "until it hurts",
    "until you feel pain",
    "through the pain",
    "through pain",
    "push through",
    "push harder",
    "push yourself",
    "force yourself",
    "no pain no gain",
    "feel the burn",
    "to the limit",
    "to your limit",
    "to failure",
    // Jumping, running, lifting, weights, strenuous moves.
    "jump*",
    "hop",
    "hopping",
    "run in place",
    "running",
    "sprint*",
    "jog",
    "jogging",
    "burpee*",
    "lunge*",
    "deep squat*",
    "full squat*",
    "as low as you can",
    "plank*",
    "push up*",
    "pushup*",
    "sit up*",
    "situp*",
    "handstand*",
    "headstand*",
    "backbend*",
    "somersault*",
    "cartwheel*",
    "lift weight*",
    "lifting weight*",
    "lift heavy",
    "lifting heavy",
    "lift yourself",
    "lift your body",
    "lift your whole body",
    "weights",
    "dumbbell*",
    "barbell*",
    "kettlebell*",
    "resistance band*",
    "heavy object*",
    // Floor work.
    "lie down",
    "lying down",
    "lay down",
    "lie on the floor",
    "lying on the floor",
    "sit on the floor",
    "sitting on the floor",
    "kneel*",
    "all fours",
    "get down on",
    "floor exercise*",
    "floor work",
    "on your back",
    "on your stomach",
    "on your belly",
    // Neck circles and rolls.
    "neck roll*",
    "neck circle*",
    "roll your neck",
    "rolling your neck",
    "circle your neck",
    "rotate your neck",
    "head roll*",
    "head circle*",
    "roll your head",
    "rolling your head",
    "circle your head",
    "rotate your head in",
    // Medical, therapy and health claims.
    "cure*",
    "treat",
    "treats",
    "treated",
    "treating",
    "treatment*",
    "therap*",
    "diagnos*",
    "heal",
    "heals",
    "healed",
    "healing",
    "medical",
    "medication*",
    "medicine*",
    "prescri*",
    "symptom*",
    "disorder*",
    "disease*",
    "clinical*",
    "blood pressure",
    "heart rate",
    "scientifically proven",
    // More pushing, holding and not breathing.
    "hold breath",
    "hold in",
    "hold it",
    "as long as you can",
    "until you cant",
    "ignore the pain",
    "even if it hurts",
    "through the discomfort",
    "dont breathe",
    "stop breathing",
    // More health claims.
    "reliev*",
    "anxiety",
    "depress*",
    "insomnia",
    "immun*",
    "cortisol",
    "circulation",
    "detox*",
    "stress relief",
    // Frequent non-English words that are plain ASCII (the ASCII rule cannot see them), so a
    // routine in another language is refused. This list is only a net; the instruction asks for
    // plain English and `looks_english` catches sentences with no common English word.
    "nefes*",
    "saniye*",
    "springen*",
    "zehnmal",
    "sie",
    "und",
    "nicht",
    "bitte",
    "respire*",
    "respira*",
    "secondes",
    "segundos",
    "sekunden",
    "saltar",
    "sauter",
];

/// Common English function words and everyday routine words. A model-written string of
/// [`ENGLISH_MIN_WORDS`] or more words must contain at least one of them, which no sentence in
/// plain English lacks in practice; text with none is taken to be another language and refused.
const ENGLISH_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "to", "of", "in", "on", "at", "for", "with", "from", "as", "by",
    "is", "are", "be", "it", "its", "you", "your", "yours", "that", "this", "then", "each", "if",
    "while", "when", "let", "your", "one", "both", "into", "out", "up", "down", "back", "again",
    "slowly", "gently", "softly", "easy", "easily", "like", "can", "may", "now", "next", "feel",
    "take", "keep", "bring", "turn", "lift", "rest", "relax", "breathe", "notice", "left", "right",
    "side", "over", "until", "about", "any", "all", "no", "not", "only", "just", "more", "less",
];

/// Strings with at least this many words must contain an [`ENGLISH_WORDS`] entry.
const ENGLISH_MIN_WORDS: usize = 5;

/// Extra phrases refused in movement sets only: closing the eyes while standing is unsafe, and a
/// movement set may be done standing. (Pause and breathing sets may ask for closed eyes: they are
/// seated, and the pack text says "if you like".)
pub const REFUSED_MOVEMENT: &[&str] = &[
    "close your eyes",
    "closing your eyes",
    "shut your eyes",
    "eyes closed",
    "with your eyes shut",
    "keep your eyes closed",
    "eyes shut",
    "close the eyes",
    "close both eyes",
];

/// Extra phrases refused in breathing sets only. The host owns the phase kinds and timings, so a
/// cue never needs the word "hold" (a word starting that way is refused, "holding" included) or a
/// count; there is no exception for the cue of a host `hold` phase.
pub const REFUSED_BREATHING: &[&str] = &["hold*", "count of"];
// Narrowed once: "hold on to" is allowed (the built-in tempo intro says "nothing to hold on to").

/// Maps one character of the allowed typographic punctuation to its ASCII form.
fn fold_char(c: char) -> Option<&'static str> {
    match c {
        '\u{2018}' | '\u{2019}' => Some("'"),
        '\u{201C}' | '\u{201D}' => Some("\""),
        '\u{2013}' | '\u{2014}' => Some("-"),
        '\u{2026}' => Some("..."),
        _ => None,
    }
}

/// Folds model-written text to plain printable ASCII: curly quotes, en/em dashes and the
/// ellipsis are mapped to their ASCII forms. Returns `None` when the text holds any other
/// character outside printable ASCII (non-English letters, combining marks, fullwidth or
/// look-alike letters, control characters, emoji). Body sets are written in plain English with
/// basic Latin letters, so such text is refused outright rather than matched.
pub fn fold_ascii(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if let Some(f) = fold_char(c) {
            out.push_str(f);
        } else if (' '..='~').contains(&c) {
            out.push(c);
        } else {
            return None;
        }
    }
    Some(out)
}

/// Splits text into lower-case words: letters and digits form words, apostrophes are dropped
/// ("don't" is "dont"), everything else (spaces, hyphens, punctuation) breaks a word.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            cur.extend(c.to_lowercase());
        } else if c == '\'' || c == '\u{2019}' || c == '\u{2018}' || c == '`' {
            // dropped, word continues
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

struct Phrase {
    words: Vec<String>,
    prefix_last: bool,
}

fn parse(entry: &str) -> Phrase {
    let prefix_last = entry.ends_with('*');
    Phrase {
        words: words(entry.trim_end_matches('*')),
        prefix_last,
    }
}

fn contains(text: &[String], phrase: &Phrase) -> bool {
    let n = phrase.words.len();
    if n == 0 || text.len() < n {
        return false;
    }
    text.windows(n).any(|w| {
        w.iter().zip(&phrase.words).enumerate().all(|(i, (t, p))| {
            if i + 1 == n && phrase.prefix_last {
                t.starts_with(p.as_str())
            } else {
                t == p
            }
        })
    })
}

/// The first refused phrase `text` contains, as written in the list, for engine `engine`.
pub fn refused_phrase_in(engine: EngineKind, text: &str) -> Option<&'static str> {
    // Fold the allowed punctuation; any other character stays as it is and is refused by
    // `has_refused_phrase` (the ASCII rule) before this matters.
    let folded: String = text
        .chars()
        .map(|c| fold_char(c).map_or_else(|| c.to_string(), str::to_owned))
        .collect();
    let mut w = words(&folded);
    if engine == EngineKind::Breathing {
        // "nothing to hold on to" (a built-in intro) is not a hold; narrowed to exactly that.
        let mut i = 0;
        while i + 2 < w.len() {
            if w[i].starts_with("hold") && w[i + 1] == "on" && w[i + 2] == "to" {
                w[i].clear();
            }
            i += 1;
        }
        w.retain(|x| !x.is_empty());
    }
    let extra: &[&str] = match engine {
        EngineKind::Movement => REFUSED_MOVEMENT,
        EngineKind::Breathing => REFUSED_BREATHING,
        _ => &[],
    };
    let found = REFUSED
        .iter()
        .chain(extra)
        .find(|entry| contains(&w, &parse(entry)))
        .copied();
    found.or_else(|| (!looks_english(&w)).then_some("not plain English"))
}

/// False for a run of [`ENGLISH_MIN_WORDS`] or more words with no common English word.
fn looks_english(words: &[String]) -> bool {
    let letters = words
        .iter()
        .filter(|w| w.chars().any(|c| c.is_alphabetic()))
        .count();
    letters < ENGLISH_MIN_WORDS || words.iter().any(|w| ENGLISH_WORDS.contains(&w.as_str()))
}

/// True when any string anywhere in `data` (values only; the keys are fixed by the schema)
/// is not plain ASCII (after the typographic fold) or contains a refused phrase. `data` must be
/// model-written text only: host-written fields are not subject to the rule (see
/// [`body_problem`] for filled data). Always false for engines that are not body engines.
pub fn has_refused_phrase(engine: EngineKind, data: &Value) -> bool {
    engine.is_body()
        && any_string(data, &mut |s| match fold_ascii(s) {
            None => true,
            Some(f) => refused_phrase_in(engine, &f).is_some(),
        })
}

/// The model-written part of host-filled body data: movement and pause lose the host `wayOut`,
/// `closing` and `restSeconds`; breathing keeps its title, intro, closing and cues (the model
/// writes the closing) and loses the host phase kinds, timings and rounds.
fn model_written(engine: EngineKind, data: &Value) -> Value {
    let Some(obj) = data.as_object() else {
        return Value::Null;
    };
    let mut out = obj.clone();
    match engine {
        EngineKind::Movement => {
            for k in ["wayOut", "closing", "restSeconds"] {
                out.remove(k);
            }
        }
        EngineKind::Pause => {
            out.remove("closing");
        }
        EngineKind::Breathing => {
            out.remove("rounds");
            if let Some(Value::Array(phases)) = out.get_mut("phases") {
                for p in phases.iter_mut() {
                    let cue = p.get("cue").cloned();
                    *p = match cue {
                        Some(c) => serde_json::json!({ "cue": c }),
                        None => serde_json::json!({}),
                    };
                }
            }
        }
        _ => {}
    }
    Value::Object(out)
}

/// The load-time re-check of stored, host-filled body data (ADR-028 section 3): the generation
/// caps (at most 60 seconds, 10 repetitions and 8 moves or steps) and the phrase and ASCII check
/// over the model-written fields only. True means the data must not be used. A non-object, or
/// anything that is not a body engine, is never a problem here (the engine schema decides).
pub fn body_problem(engine: EngineKind, data: &Value) -> bool {
    if !engine.is_body() {
        return false;
    }
    let (list, max_items) = match engine {
        EngineKind::Movement => ("moves", 8),
        EngineKind::Pause => ("steps", 8),
        _ => ("", 0),
    };
    if !list.is_empty() {
        let items = data.get(list).and_then(Value::as_array);
        if items.is_none_or(|a| a.len() > max_items) {
            return true;
        }
        for item in items.into_iter().flatten() {
            let over = |key: &str, max: u32| {
                item.get(key)
                    .is_some_and(|v| v.as_u64().is_none_or(|n| n > u64::from(max)))
            };
            if over("seconds", super::profiles::BODY_MAX_SECONDS)
                || over("reps", super::profiles::MOVEMENT_MAX_REPS)
            {
                return true;
            }
        }
    }
    has_refused_phrase(engine, &model_written(engine, data))
}

fn any_string(v: &Value, hit: &mut dyn FnMut(&str) -> bool) -> bool {
    match v {
        Value::String(s) => hit(s),
        Value::Array(a) => a.iter().any(|x| any_string(x, hit)),
        Value::Object(o) => o.values().any(|x| any_string(x, hit)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A sentence that contains `entry`: a trailing `*` becomes a plausible ending.
    fn sample(entry: &str) -> String {
        entry.replace('*', "ing")
    }

    #[test]
    fn every_phrase_on_the_list_refuses() {
        for e in REFUSED {
            for engine in [
                EngineKind::Movement,
                EngineKind::Breathing,
                EngineKind::Pause,
            ] {
                let text = format!("Please {} now.", sample(e));
                // This very entry matches (not just some other one), and the check refuses.
                assert!(contains(&words(&text), &parse(e)), "{e}");
                assert!(
                    refused_phrase_in(engine, &text).is_some(),
                    "{e} in {engine:?}"
                );
                // Case-folded and inside a longer sentence.
                let loud = format!("Then, {}!", sample(e).to_uppercase());
                assert!(refused_phrase_in(engine, &loud).is_some(), "{e} upper");
            }
        }
        for e in REFUSED_BREATHING {
            assert!(
                refused_phrase_in(EngineKind::Breathing, &format!("Now {}", sample(e))).is_some(),
                "{e}"
            );
            assert!(
                refused_phrase_in(EngineKind::Pause, &format!("Now {}", sample(e))).is_none(),
                "{e}"
            );
        }
        for e in REFUSED_MOVEMENT {
            assert!(
                refused_phrase_in(EngineKind::Movement, &format!("Now {e} please")).is_some(),
                "{e}"
            );
            // Seated pauses and breathing may ask for closed eyes.
            assert!(
                refused_phrase_in(EngineKind::Pause, &format!("Now {e} please")).is_none(),
                "{e}"
            );
            assert!(
                refused_phrase_in(EngineKind::Breathing, &format!("Now {e} please")).is_none(),
                "{e}"
            );
        }
    }

    #[test]
    fn matching_is_by_whole_word_and_ignores_punctuation_and_case() {
        let m = EngineKind::Movement;
        assert!(refused_phrase_in(m, "Neck-roll, slowly").is_some());
        assert!(refused_phrase_in(m, "HOLD  your\nbreath").is_some());
        assert!(refused_phrase_in(m, "Don't jump").is_some());
        // Whole words: no hit inside longer words or unrelated phrases.
        assert!(refused_phrase_in(m, "Hold your breathing steady").is_none());
        assert!(refused_phrase_in(m, "A healthy posture").is_none());
        assert!(refused_phrase_in(m, "Hopeful and calm").is_none());
        assert!(refused_phrase_in(m, "The treaty of Verdun").is_none());
        assert!(refused_phrase_in(m, "Shift your weight gently").is_none());
        assert!(refused_phrase_in(m, "Close the window").is_none());
        assert!(refused_phrase_in(m, "Medicine ball").is_some());
    }

    #[test]
    fn gentle_wording_from_the_built_in_packs_is_not_refused() {
        for t in [
            "Feel your feet on the floor and your weight in the chair.",
            "Turn your head toward one shoulder as far as feels easy.",
            "Lift both shoulders toward your ears, then let them drop.",
            "Lift your hands away from the keys and let them rest in your lap.",
            "Close your eyes if you like and let them rest in the dark.",
        ] {
            assert_eq!(refused_phrase_in(EngineKind::Pause, t), None, "{t}");
        }
        assert_eq!(
            refused_phrase_in(
                EngineKind::Movement,
                "Lift both shoulders toward your ears, then let them drop."
            ),
            None
        );
    }

    #[test]
    fn every_string_in_the_set_is_checked_and_other_engines_are_not() {
        let deep = json!({"moves": [{"name": "Ok", "steps": ["Fine", "Now jump up"]}]});
        assert!(has_refused_phrase(EngineKind::Movement, &deep));
        assert!(!has_refused_phrase(EngineKind::Quiz, &deep));
        let cue = json!({"phases": [{"cue": "Heal"}]});
        assert!(has_refused_phrase(EngineKind::Breathing, &cue));
        assert!(!has_refused_phrase(
            EngineKind::Breathing,
            &json!({"title": "Calm"})
        ));
    }

    const BODY: [EngineKind; 3] = [
        EngineKind::Movement,
        EngineKind::Breathing,
        EngineKind::Pause,
    ];

    #[test]
    fn non_ascii_and_look_alike_text_is_refused_in_every_body_engine() {
        for text in [
            "Nefesini 30 saniye tut",
            "Springen Sie zehnmal",
            "Now j\u{00fa}mp",
            "Now ju\u{0301}mp",
            "Now \u{ff4a}\u{ff55}\u{ff4d}\u{ff50}",
            "Now \u{1d423}\u{1d42e}\u{1d426}\u{1d429}",
            "Now \u{0458}ump",
            "Sway \u{1f642}",
            "Sway\tgently",
        ] {
            for e in BODY {
                let data = json!({"title": "Calm", "steps": [text]});
                assert!(has_refused_phrase(e, &data), "{e:?}: {text}");
            }
        }
    }

    #[test]
    fn typographic_punctuation_is_folded_and_accepted() {
        for e in BODY {
            for t in [
                "Don\u{2019}t lock your knees",
                "\u{201C}Easy\u{201D} does it \u{2013} slowly\u{2026} then rest \u{2014} fine",
                "It\u{2018}s fine",
            ] {
                assert!(!has_refused_phrase(e, &json!({"title": t})), "{t}");
            }
            // The fold applies before the phrase check.
            assert!(has_refused_phrase(
                e,
                &json!({"title": "Don\u{2019}t breathe"})
            ));
            assert!(has_refused_phrase(
                e,
                &json!({"title": "Hold it\u{2026} now"})
            ));
        }
        assert_eq!(
            fold_ascii("a\u{2019}b\u{201C}c\u{2013}d\u{2026}").as_deref(),
            Some("a'b\"c-d...")
        );
        assert_eq!(fold_ascii("caf\u{e9}"), None);
    }

    #[test]
    fn the_second_round_of_english_phrases_is_refused() {
        for t in [
            "Hold breath for a while",
            "Hold in the air",
            "Hold it there",
            "Stay as long as you can",
            "Go until you can\u{2019}t",
            "Go until you cant",
            "Ignore the pain",
            "Even if it hurts",
            "Through the discomfort",
            "Don't breathe",
            "Stop breathing",
            "Relieves tension",
            "Eases anxiety",
            "Fights depression",
            "Cures insomnia",
            "Boosts immunity",
            "Lowers cortisol",
            "Improves circulation",
            "A detox for you",
            "Fast stress relief",
        ] {
            for e in BODY {
                assert!(has_refused_phrase(e, &json!({"title": t})), "{e:?}: {t}");
            }
        }
        // "can't" with a straight or curly apostrophe is the same phrase once apostrophes drop.
        assert!(has_refused_phrase(
            EngineKind::Pause,
            &json!({"title": "until you can't"})
        ));
        for t in ["Eyes shut", "Close the eyes", "Close both eyes"] {
            assert!(refused_phrase_in(EngineKind::Movement, t).is_some(), "{t}");
            assert!(refused_phrase_in(EngineKind::Pause, t).is_none(), "{t}");
        }
    }

    #[test]
    fn breathing_refuses_hold_and_count_of_everywhere() {
        for t in ["Hold", "Holding steady", "A count of four", "hold."] {
            assert!(
                has_refused_phrase(EngineKind::Breathing, &json!({"title": t, "closing": t})),
                "{t}"
            );
            assert!(
                has_refused_phrase(
                    EngineKind::Breathing,
                    &json!({"title": "Calm", "phases": [{"cue": t}]})
                ),
                "{t}"
            );
        }
        assert!(!has_refused_phrase(
            EngineKind::Pause,
            &json!({"title": "Hold the door"})
        ));
    }

    #[test]
    fn body_problem_applies_the_caps_and_ignores_host_fields() {
        let ok = json!({"title": "Moves", "wayOut": "Stop at any time.",
            "closing": "That is the routine.", "restSeconds": 15,
            "moves": [{"name": "Shrug", "steps": ["Lift and drop."], "reps": 5}]});
        assert!(!body_problem(EngineKind::Movement, &ok));
        // Host text is not checked.
        let mut host = ok.clone();
        host["closing"] = json!("Caf\u{e9} hold it jump");
        assert!(!body_problem(EngineKind::Movement, &host));
        let mut bad = ok.clone();
        bad["moves"][0]["steps"] = json!(["Jump up"]);
        assert!(body_problem(EngineKind::Movement, &bad));
        let mut bad = ok.clone();
        bad["moves"][0]["name"] = json!("Nefesini tut");
        assert!(body_problem(EngineKind::Movement, &bad));
        let mut bad = ok.clone();
        bad["moves"][0]["reps"] = json!(11);
        assert!(body_problem(EngineKind::Movement, &bad));
        let mut bad = ok.clone();
        bad["moves"][0].as_object_mut().unwrap().remove("reps");
        bad["moves"][0]["seconds"] = json!(61);
        assert!(body_problem(EngineKind::Movement, &bad));
        let mut bad = ok.clone();
        bad["moves"] = json!(vec![ok["moves"][0].clone(); 9]);
        assert!(body_problem(EngineKind::Movement, &bad));
        let pause = json!({"title": "Rest", "closing": "x hold it", "steps": [{"text": "Sit.", "seconds": 60}]});
        assert!(!body_problem(EngineKind::Pause, &pause));
        let mut bad = pause.clone();
        bad["steps"][0]["seconds"] = json!(90);
        assert!(body_problem(EngineKind::Pause, &bad));
        // Breathing: the host `hold` kind is fine, a model-written cue is checked.
        let br = json!({"title": "Calm", "rounds": 4, "phases": [
            {"kind": "inhale", "seconds": 4, "cue": "In"},
            {"kind": "hold", "seconds": 2, "cue": "Pause"}]});
        assert!(!body_problem(EngineKind::Breathing, &br));
        let mut bad = br.clone();
        bad["phases"][1]["cue"] = json!("Hold");
        assert!(body_problem(EngineKind::Breathing, &bad));
        assert!(!body_problem(EngineKind::Quiz, &json!({"x": "jump"})));
    }
}
