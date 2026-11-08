//! Reply checks, in the order of ADR-028 section 5. A reply that fails any step is refused as a
//! whole; the only thing a failure reports is *which kind* of problem it was.

use serde_json::Value;

use super::{CheckedRequest, EngineKind, MAX_REPLY_BYTES, body};

/// The kind of problem a reply had. Each maps to one fixed sentence for the retry
/// ([`Problem::sentence`]); no reply text and no checker text is ever carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    NotJson,
    TooLong,
    /// Failed the generation schema, the engine schema or the pack text checks.
    Shape,
    /// Quiz: repeated choices or an answer that isn't one of them.
    Choices,
    /// A body set contained a refused phrase.
    Phrase,
}

impl Problem {
    /// The sentence sent with the one retry.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::NotJson => "The reply was not valid JSON.",
            Self::TooLong => "The reply was too long.",
            Self::Shape => "A field was missing or too long.",
            Self::Choices => {
                "A question had repeated choices or an answer that was not one of them."
            }
            Self::Phrase => "A step asked for something the activity doesn't allow.",
        }
    }
}

/// The first top-level JSON object in `text`: the object starting at the first `{`, which may
/// sit inside a Markdown code fence or have plain text before and after it. Anything after the
/// first object is ignored. Refused (`None`): no object, a `[` before the first `{` (a top-level
/// array), text that isn't valid JSON from the first `{`, nesting deeper than serde_json's limit,
/// and any text over [`MAX_REPLY_BYTES`].
pub fn extract_json_object(text: &str) -> Option<Value> {
    if text.len() > MAX_REPLY_BYTES {
        return None;
    }
    let start = text.find(['{', '['])?;
    if text.as_bytes()[start] != b'{' {
        return None;
    }
    let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Value>();
    match stream.next()? {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

/// Quiz-only checks: choices unique after trimming and case folding, and `answer` within
/// `choices`. `None` when the set is fine.
pub fn quiz_problem(data: &Value) -> Option<Problem> {
    let questions = data.get("questions")?.as_array()?;
    for q in questions {
        let choices: Vec<String> = q
            .get("choices")
            .and_then(Value::as_array)
            .map(|c| {
                c.iter()
                    .map(|s| s.as_str().unwrap_or("").trim().to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let answer = q.get("answer").and_then(Value::as_u64);
        let in_range = answer.is_some_and(|a| usize::try_from(a).is_ok_and(|a| a < choices.len()));
        let mut seen = std::collections::HashSet::new();
        let unique = choices.iter().all(|c| seen.insert(c.clone()));
        if !in_range || !unique || choices.len() < 2 {
            return Some(Problem::Choices);
        }
    }
    None
}

/// Runs every check on one reply and returns the data to store. `cut` says the transport cut
/// the reply at the byte cap. `engine_check` is the host's engine-schema gate and pack text
/// checks; its messages are dropped here, only pass or fail is used.
pub(super) fn check_reply(
    req: &CheckedRequest,
    text: &str,
    cut: bool,
    engine_check: &dyn Fn(&Value) -> Result<(), Vec<String>>,
) -> Result<Value, Problem> {
    if cut || text.len() > MAX_REPLY_BYTES {
        return Err(Problem::TooLong);
    }
    let value = extract_json_object(text).ok_or(Problem::NotJson)?;
    // 1. The generation schema, through the host's fail-closed gate (marketplace::data_schema).
    let schema = req
        .engine
        .generation_schema_for(req.size, req.breathing.as_ref());
    if !marketplace::data_schema::validate(&schema, &value).is_empty() {
        return Err(Problem::Shape);
    }
    // 2. Host fields.
    let generated = value.clone();
    let mut data = value;
    req.engine
        .fill_host_fields(&mut data, req.breathing.as_ref());
    // 3. The engine's own gate and the pack text checks.
    if engine_check(&data).is_err() {
        return Err(Problem::Shape);
    }
    // 4. Profile extras.
    if req.engine == EngineKind::Quiz
        && let Some(p) = quiz_problem(&data)
    {
        return Err(p);
    }
    // 5. Refused phrases, over everything the model wrote (not the host's own text).
    if body::has_refused_phrase(req.engine, &generated) {
        return Err(Problem::Phrase);
    }
    Ok(data)
}
