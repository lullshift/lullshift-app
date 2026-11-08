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
];

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
];

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
    let w = words(text);
    let extra: &[&str] = if engine == EngineKind::Movement {
        REFUSED_MOVEMENT
    } else {
        &[]
    };
    REFUSED
        .iter()
        .chain(extra)
        .find(|entry| contains(&w, &parse(entry)))
        .copied()
}

/// True when any string anywhere in `data` (values only; the keys are fixed by the schema)
/// contains a refused phrase. Always false for engines that are not body engines.
pub fn has_refused_phrase(engine: EngineKind, data: &Value) -> bool {
    engine.is_body() && any_string(data, &mut |s| refused_phrase_in(engine, s).is_some())
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
}
