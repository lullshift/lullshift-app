//! Host-side check of a content pack's data against its engine's data schema (ADR-016).
//!
//! The CLI and `pnpm check` validate packs with Ajv. The app checks again before it starts a
//! pack, and cannot use Ajv: Ajv compiles schemas with `new Function`, which the app's CSP
//! (`script-src 'self'`, no `unsafe-eval`) rightly forbids. Since Sprint 12 the host validates
//! with the [`jsonschema`] crate (JSON Schema 2020-12; ADR-016 amendment, decision 13), wrapped
//! in three layers of its own because engine schemas may one day come from third parties:
//!
//! - [`check_schema`] refuses any keyword outside [`ASSERTIONS`] and [`ANNOTATIONS`], any `$ref`
//!   other than `#` or `#/$defs/<name>`, `$ref` cycles, schemas that unfold to more than
//!   [`MAX_UNFOLDED`] nodes, and any pattern it can't compile with the same meaning as
//!   JavaScript's `u` flag. An engine whose schema is refused can't run packs. The crate is built
//!   without its HTTP and file retrievers and runs offline, so no `$ref` is ever fetched.
//! - `pattern` is evaluated by this module's own translation ([`compile`]), registered as a
//!   custom keyword that replaces the crate's: the crate's ECMA translation lets `.` match `\r`,
//!   U+2028 and U+2029, which JavaScript doesn't (the shared corpus checks this).
//! - [`validate`] reports problems in the same plain words as the shared TypeScript formatter
//!   (`friendlyErrors` in `@lullshift/agent-protocol`), e.g. `cards[3].front is longer than 200
//!   characters`.
//!
//! `tests/activities/check-engine-compat.mjs` refuses the same keywords, and Rust tests run every
//! engine's fixtures and the shared Ajv/Rust corpus through this validator, so the two can't
//! drift apart unnoticed.

use std::collections::{HashMap, HashSet};

use jsonschema::error::{TypeKind, ValidationErrorKind};
use jsonschema::paths::{Location, LocationSegment};
use jsonschema::{Draft, Keyword, ValidationError, Validator};
use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value};

/// Keywords that constrain data. Keep in sync with `HOST_KEYWORDS` in the compat check.
pub const ASSERTIONS: &[&str] = &[
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "minProperties",
    "maxProperties",
    "items",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minLength",
    "maxLength",
    "pattern",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "if",
    "then",
    "else",
    "$ref",
    "$defs",
];

/// Keywords that only describe; ignored when validating.
pub const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
];

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";
/// Most problems reported for one file; the first few are what a creator needs.
const MAX_PROBLEMS: usize = 20;
/// Compiled size limit for one pattern.
const REGEX_SIZE: usize = 1 << 20;
/// Most schema nodes (plus `enum`, `const` and `required` entries) in a schema with every `$ref`
/// replaced by its target. Validation visits each of them at most once per data value, so with
/// data capped at 256 KiB this bounds the work without a budget inside the crate (which has
/// none). The largest bundled engine schema (quiz) unfolds to under 50.
pub const MAX_UNFOLDED: u64 = 2_000;
/// Deepest nesting of sub-schemas accepted (before `$ref`s are followed).
const MAX_SCHEMA_DEPTH: usize = 32;
/// Largest `multipleOf`; every whole number up to this is exact in an f64.
const MAX_MULTIPLE: u64 = 1 << 53;
/// JavaScript's `\s` (WhiteSpace and LineTerminator): no U+0085, with U+FEFF.
const JS_SPACE: &str = r"\x{9}-\x{d}\x{20}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}";
/// What a `u`-mode `.` must not match: JavaScript line terminators.
const JS_LINE_END: &str = r"\n\r\x{2028}\x{2029}";
/// What the custom `pattern` keyword reports for the shared plain-text pattern.
const PLAIN_TEXT_PROBLEM: &str =
    "contains a line break, tab or invisible character, which isn't allowed";
const FORM_PROBLEM: &str = "doesn't have the expected form";

/// Checks that `schema` uses only what this host accepts, then compiles it with the crate. The
/// error names the first problem in plain words.
pub fn check_schema(schema: &Value) -> Result<(), String> {
    prepare(schema).map(|_| ())
}

/// [`check_schema`], returning the compiled validator.
fn prepare(schema: &Value) -> Result<Validator, String> {
    let root = schema
        .as_object()
        .ok_or("the data schema must be an object")?;
    if let Some(s) = root.get("$schema")
        && s.as_str() != Some(DRAFT)
    {
        return Err(format!("$schema must be {DRAFT}"));
    }
    check_node(schema, schema, "#", true, 0)?;
    let mut sizes = HashMap::new();
    let mut visiting = HashSet::new();
    let unfolded = unfolded_size(schema, schema, &mut sizes, &mut visiting)?;
    if unfolded > MAX_UNFOLDED {
        return Err(format!(
            "the data schema is too large to check (more than {MAX_UNFOLDED} parts once its references are expanded)"
        ));
    }
    build(schema)
}

fn check_node(
    root: &Value,
    node: &Value,
    at: &str,
    is_root: bool,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!("{at} is nested too deeply"));
    }
    let obj = match node {
        Value::Bool(_) => return Ok(()),
        Value::Object(o) => o,
        _ => return Err(format!("{at} isn't a schema")),
    };
    for (k, v) in obj {
        let here = format!("{at}/{k}");
        if (k == "$schema" || k == "$id") && !is_root {
            return Err(format!("{here}: only the top of the schema may set {k}"));
        }
        if k == "$id" && !v.as_str().is_some_and(|id| id.starts_with("urn:")) {
            // A URL base would make relative references resolve against the network.
            return Err(format!("{here}: $id must be a urn: name"));
        }
        if ANNOTATIONS.contains(&k.as_str()) {
            continue;
        }
        if !ASSERTIONS.contains(&k.as_str()) {
            return Err(format!("{here}: the app doesn't support the keyword {k}"));
        }
        match k.as_str() {
            "properties" | "$defs" => {
                let map = v.as_object().ok_or(format!("{here} must be an object"))?;
                if k == "$defs" && !is_root {
                    return Err(format!("{here}: only the top of the schema may set $defs"));
                }
                for (name, sub) in map {
                    check_node(root, sub, &format!("{here}/{name}"), false, depth + 1)?;
                }
            }
            "additionalProperties" | "items" | "not" | "if" | "then" | "else" => {
                check_node(root, v, &here, false, depth + 1)?;
            }
            "allOf" | "anyOf" | "oneOf" => {
                let list = v
                    .as_array()
                    .filter(|l| !l.is_empty())
                    .ok_or(format!("{here} must be a non-empty list"))?;
                for (i, sub) in list.iter().enumerate() {
                    check_node(root, sub, &format!("{here}/{i}"), false, depth + 1)?;
                }
            }
            "type" => {
                let ok = |t: &Value| {
                    t.as_str().is_some_and(|t| {
                        [
                            "string", "number", "integer", "boolean", "array", "object", "null",
                        ]
                        .contains(&t)
                    })
                };
                let valid = match v {
                    Value::Array(ts) => !ts.is_empty() && ts.iter().all(ok),
                    t => ok(t),
                };
                if !valid {
                    return Err(format!("{here} isn't a known type"));
                }
            }
            "enum" => {
                if !v.as_array().is_some_and(|l| !l.is_empty()) {
                    return Err(format!("{here} must be a non-empty list"));
                }
            }
            "required" => {
                if !v.as_array().is_some_and(|l| l.iter().all(Value::is_string)) {
                    return Err(format!("{here} must be a list of names"));
                }
            }
            "minProperties" | "maxProperties" | "minItems" | "maxItems" | "minLength"
            | "maxLength" => {
                if v.as_u64().is_none() {
                    return Err(format!("{here} must be a whole number of 0 or more"));
                }
            }
            "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
                if !v.is_number() {
                    return Err(format!("{here} must be a number"));
                }
            }
            "multipleOf" => {
                if multiple_of(v).is_none() {
                    return Err(format!("{here} must be a whole number above 0"));
                }
            }
            "uniqueItems" => {
                if !v.is_boolean() {
                    return Err(format!("{here} must be true or false"));
                }
            }
            "pattern" => {
                let p = v.as_str().ok_or(format!("{here} must be text"))?;
                compile(p).map_err(|e| format!("{here}: {e}"))?;
            }
            "$ref" => {
                let r = v.as_str().ok_or(format!("{here} must be text"))?;
                if resolve(root, r).is_none() {
                    return Err(format!(
                        "{here}: only #/$defs/<name> references are supported"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Size of `node` with every `$ref` replaced by its target, counting `enum`, `const` and
/// `required` entries too. Refuses `$ref` cycles. Memoised per node, so shared `$defs` are cheap
/// to measure even when they are used many times.
fn unfolded_size(
    root: &Value,
    node: &Value,
    sizes: &mut HashMap<*const Value, u64>,
    visiting: &mut HashSet<*const Value>,
) -> Result<u64, String> {
    let key: *const Value = node;
    if let Some(n) = sizes.get(&key) {
        return Ok(*n);
    }
    let Value::Object(obj) = node else {
        return Ok(1);
    };
    if !visiting.insert(key) {
        return Err(
            "the data schema refers to itself in a loop, which the app doesn't support".to_owned(),
        );
    }
    let mut total: u64 = 1;
    for (k, v) in obj {
        let add = match k.as_str() {
            "properties" => {
                let mut sum = 0u64;
                for sub in v.as_object().into_iter().flat_map(Map::values) {
                    sum = sum.saturating_add(unfolded_size(root, sub, sizes, visiting)?);
                }
                sum
            }
            "additionalProperties" | "items" | "not" | "if" | "then" | "else" => {
                unfolded_size(root, v, sizes, visiting)?
            }
            "allOf" | "anyOf" | "oneOf" => {
                let mut sum = 0u64;
                for sub in v.as_array().into_iter().flatten() {
                    sum = sum.saturating_add(unfolded_size(root, sub, sizes, visiting)?);
                }
                sum
            }
            "$ref" => match v.as_str().and_then(|r| resolve(root, r)) {
                Some(target) => unfolded_size(root, target, sizes, visiting)?,
                None => 0,
            },
            "enum" => v
                .as_array()
                .map_or(0, |l| l.iter().map(value_size).sum::<u64>()),
            "const" => value_size(v),
            "required" => v.as_array().map_or(0, |l| l.len() as u64),
            _ => 0,
        };
        total = total.saturating_add(add);
        if total > MAX_UNFOLDED {
            // No need to measure further; also keeps this walk itself cheap.
            break;
        }
    }
    visiting.remove(&key);
    sizes.insert(key, total);
    Ok(total)
}

/// Rough count of the values inside `v`.
fn value_size(v: &Value) -> u64 {
    match v {
        Value::Array(l) => 1 + l.iter().map(value_size).sum::<u64>(),
        Value::Object(o) => 1 + o.values().map(value_size).sum::<u64>(),
        _ => 1,
    }
}

/// A `multipleOf` the validator can check exactly: a whole number from 1 to 2^53.
fn multiple_of(v: &Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return (1..=MAX_MULTIPLE).contains(&n).then_some(n);
    }
    let f = v.as_f64()?;
    (f.fract() == 0.0 && f >= 1.0 && f <= MAX_MULTIPLE as f64).then_some(f as u64)
}

fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    if reference == "#" {
        return Some(root);
    }
    let name = reference.strip_prefix("#/$defs/")?;
    if name.is_empty() || name.contains('/') || name.contains('~') || name.contains('%') {
        return None;
    }
    root.get("$defs")?.get(name)
}

/// Compiles a JSON Schema pattern so it means what it means in JavaScript with the `u` flag.
/// Rust's `\d`, `\w`, `\b` are Unicode-aware and JavaScript's aren't, so the ASCII classes are
/// spelled out; word boundaries, lookaround and backreferences are refused.
pub fn compile(pattern: &str) -> Result<Regex, String> {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let next = chars
                    .next()
                    .ok_or("the pattern ends with a lone backslash")?;
                if next == 's' || next == 'S' {
                    if next == 's' && in_class {
                        out.push_str(JS_SPACE);
                    } else if next == 's' {
                        out.push_str(&format!("[{JS_SPACE}]"));
                    } else {
                        out.push_str(&format!("[^{JS_SPACE}]"));
                    }
                    continue;
                }
                let ascii = match next {
                    'd' => Some("[:digit:]"),
                    'D' => Some("[:^digit:]"),
                    'w' => Some("[:word:]"),
                    'W' => Some("[:^word:]"),
                    _ => None,
                };
                if let Some(cls) = ascii {
                    if in_class {
                        out.push_str(cls);
                    } else {
                        out.push('[');
                        out.push_str(cls);
                        out.push(']');
                    }
                } else if matches!(next, 'b' | 'B' | 'k') || next.is_ascii_digit() && next != '0' {
                    return Err(format!(
                        "the pattern uses \\{next}, which the app doesn't support"
                    ));
                } else {
                    out.push('\\');
                    out.push(next);
                }
            }
            '[' if !in_class && chars.peek() == Some(&']') => {
                // JavaScript (u mode): `[]` is an empty class that matches nothing. Rust would
                // read the `]` as a literal.
                chars.next();
                out.push_str("[^\\s\\S]");
            }
            '[' if !in_class && {
                let mut ahead = chars.clone();
                ahead.next() == Some('^') && ahead.next() == Some(']')
            } =>
            {
                // `[^]` matches any character, newlines included.
                chars.next();
                chars.next();
                out.push_str("[\\s\\S]");
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                // A leading `^` or `]` belongs to the class syntax, not its contents.
                if chars.peek() == Some(&'^') {
                    out.push('^');
                    chars.next();
                }
            }
            '[' => out.push_str("\\["),
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '(' if !in_class && chars.peek() == Some(&'?') => {
                let mut ahead = chars.clone();
                ahead.next();
                let kind = ahead.next();
                let kind2 = ahead.next();
                let allowed = kind == Some(':')
                    || (kind == Some('<') && kind2.is_some_and(|k| k != '=' && k != '!'));
                if !allowed {
                    return Err(
                        "the pattern uses lookaround or a flag group, which the app doesn't support"
                            .into(),
                    );
                }
                if kind == Some('<') {
                    // A named group: JavaScript `(?<n>…)` is Rust `(?P<n>…)`.
                    chars.next();
                    out.push_str("(?P");
                } else {
                    out.push('(');
                }
            }
            // In JavaScript `&&`, `--` and `~~` mean nothing special in a `u` class; in Rust
            // they are class set operators.
            '&' | '-' | '~' if in_class && chars.peek() == Some(&c) => {
                out.push('\\');
                out.push(c);
            }
            '.' if !in_class => out.push_str(&format!("[^{JS_LINE_END}]")),
            _ => out.push(c),
        }
    }
    RegexBuilder::new(&out)
        .size_limit(REGEX_SIZE)
        .dfa_size_limit(REGEX_SIZE)
        .build()
        .map_err(|_| "the pattern isn't one the app can check".to_owned())
}

/// True for the shared plain-text pattern (no control, invisible or direction characters).
fn is_plain_text_pattern(p: &str) -> bool {
    p.starts_with(r"^[^\u0000-\u001F")
}

/// `pattern` with JavaScript `u`-flag meaning, replacing the crate's own keyword.
struct JsPattern {
    regex: Regex,
    problem: &'static str,
}

impl<'i> Keyword<'i> for JsPattern {
    fn validate(&self, instance: &'i Value) -> Result<(), ValidationError<'i>> {
        if Keyword::is_valid(self, instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(self.problem))
        }
    }

    fn is_valid(&self, instance: &'i Value) -> bool {
        instance.as_str().is_none_or(|t| self.regex.is_match(t))
    }
}

fn js_pattern<'a>(
    _parent: &'a Map<String, Value>,
    value: &'a Value,
    _path: Location,
) -> Result<Box<dyn for<'i> Keyword<'i>>, ValidationError<'a>> {
    let p = value
        .as_str()
        .ok_or_else(|| ValidationError::custom("pattern must be text"))?;
    let regex = compile(p).map_err(ValidationError::custom)?;
    let problem = if is_plain_text_pattern(p) {
        PLAIN_TEXT_PROBLEM
    } else {
        FORM_PROBLEM
    };
    Ok(Box::new(JsPattern { regex, problem }))
}

/// Compiles a schema with the crate: draft 2020-12, no retrieval of any kind, no format
/// assertions, and this module's `pattern`.
fn build(schema: &Value) -> Result<Validator, String> {
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .offline()
        .should_validate_formats(false)
        .with_keyword("pattern", js_pattern)
        .build(schema)
        .map_err(|_| "the data schema can't be compiled".to_owned())
}

/// Data paths the way people write them: `cards[3].front`. Field names that aren't simple are
/// quoted, cut short and stripped of hidden characters, since they come from untrusted data.
fn path_text<'a>(segments: impl IntoIterator<Item = LocationSegment<'a>>) -> String {
    let mut out = String::new();
    for step in segments {
        match step {
            LocationSegment::Index(i) => out.push_str(&format!("[{i}]")),
            LocationSegment::Property(k) => push_key(&mut out, &k),
        }
    }
    out
}

fn push_key(out: &mut String, k: &str) {
    let simple = k.len() <= 64
        && k.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if simple {
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(k);
    } else {
        let clean: String = k
            .chars()
            .take(40)
            .map(|c| {
                if crate::manifest::hidden_char(c) || c == '"' {
                    '?'
                } else {
                    c
                }
            })
            .collect();
        out.push_str(&format!("[\"{clean}\"]"));
    }
}

fn type_word(t: &str) -> &'static str {
    match t {
        "string" => "text",
        "number" => "a number",
        "integer" => "a whole number",
        "boolean" => "true or false",
        "array" => "a list",
        "object" => "an object",
        "null" => "null",
        _ => "of another type",
    }
}

fn say(at: &str, message: &str) -> String {
    if at.is_empty() {
        format!("the data {message}")
    } else {
        format!("{at} {message}")
    }
}

/// Type names in the order the schema lists them (Ajv's order), falling back to the crate's.
fn type_names(schema: &Value, err: &ValidationError<'_>, kind: &TypeKind) -> Vec<&'static str> {
    let listed = schema
        .pointer(err.schema_path().as_str())
        .and_then(|t| match t {
            Value::Array(ts) => Some(ts.iter().filter_map(Value::as_str).collect::<Vec<_>>()),
            Value::String(t) => Some(vec![t.as_str()]),
            _ => None,
        });
    match listed {
        Some(names) if !names.is_empty() => names.into_iter().map(type_word).collect(),
        _ => match kind {
            TypeKind::Single(t) => vec![type_word(t.as_str())],
            TypeKind::Multiple(set) => set.iter().map(|t| type_word(t.as_str())).collect(),
        },
    }
}

/// One crate error in plain words (more than one line for several unknown fields).
fn friendly(schema: &Value, err: &ValidationError<'_>) -> Vec<String> {
    let at = path_text(err.instance_path().segments());
    let one = |m: &str| vec![say(&at, m)];
    match err.kind() {
        ValidationErrorKind::Required { property } => {
            let mut p = at.clone();
            push_key(&mut p, property.as_str().unwrap_or_default());
            vec![format!("{p} is missing")]
        }
        ValidationErrorKind::AdditionalProperties { unexpected } => unexpected
            .iter()
            .map(|name| {
                let mut p = at.clone();
                push_key(&mut p, name);
                format!("{p} isn't a known field")
            })
            .collect(),
        ValidationErrorKind::Type { kind } => one(&format!(
            "should be {}",
            type_names(schema, err, kind).join(" or ")
        )),
        ValidationErrorKind::MinLength { limit } => {
            if *limit == 1 {
                one("is empty")
            } else {
                one(&format!("is shorter than {limit} characters"))
            }
        }
        ValidationErrorKind::MaxLength { limit } => {
            one(&format!("is longer than {limit} characters"))
        }
        ValidationErrorKind::Custom { message, .. }
            if message == PLAIN_TEXT_PROBLEM || message == FORM_PROBLEM =>
        {
            one(message)
        }
        ValidationErrorKind::MinItems { limit } => {
            let noun = if *limit == 1 { "item" } else { "items" };
            one(&format!("needs at least {limit} {noun}"))
        }
        ValidationErrorKind::MaxItems { limit } => one(&format!("has more than {limit} items")),
        ValidationErrorKind::UniqueItems => one("has duplicate items"),
        ValidationErrorKind::MinProperties { limit } => {
            one(&format!("needs at least {limit} fields"))
        }
        ValidationErrorKind::MaxProperties { limit } => {
            one(&format!("has more than {limit} fields"))
        }
        ValidationErrorKind::Enum { options } => {
            let list = options.as_array().map(Vec::as_slice).unwrap_or_default();
            let shown: Vec<String> = list.iter().take(10).map(Value::to_string).collect();
            let more = if list.len() > 10 { ", …" } else { "" };
            one(&format!("should be one of: {}{more}", shown.join(", ")))
        }
        ValidationErrorKind::Constant { expected_value } => {
            one(&format!("should be {expected_value}"))
        }
        ValidationErrorKind::Minimum { limit } => one(&format!("should be at least {limit}")),
        ValidationErrorKind::Maximum { limit } => one(&format!("should be at most {limit}")),
        ValidationErrorKind::ExclusiveMinimum { limit } => {
            one(&format!("should be more than {limit}"))
        }
        ValidationErrorKind::ExclusiveMaximum { limit } => {
            one(&format!("should be less than {limit}"))
        }
        ValidationErrorKind::MultipleOf { multiple_of } => {
            one(&format!("should be a multiple of {multiple_of}"))
        }
        ValidationErrorKind::AnyOf { .. } | ValidationErrorKind::OneOfNotValid { .. } => {
            one("doesn't match any of the allowed shapes")
        }
        ValidationErrorKind::OneOfMultipleValid { .. } => {
            one("matches more than one allowed shape")
        }
        ValidationErrorKind::Not { .. } => one("has a form that isn't allowed"),
        ValidationErrorKind::FalseSchema => one("isn't allowed"),
        // Anything else can't come from a schema `check_schema` accepted.
        _ => one("can't be checked (broken schema)"),
    }
}

/// Checks `data` against a schema that passed [`check_schema`]. Returns plain-language
/// problems (at most 20), or nothing if the data is valid. Fails closed: a schema that can't
/// be compiled reports a problem rather than passing the data.
pub fn validate(schema: &Value, data: &Value) -> Vec<String> {
    let validator = match prepare(schema) {
        Ok(v) => v,
        Err(why) => return vec![format!("can't be checked: {why}")],
    };
    let mut out: Vec<String> = Vec::new();
    for err in validator.iter_errors(data) {
        for line in friendly(schema, &err) {
            if !out.contains(&line) {
                out.push(line);
            }
        }
        if out.len() >= MAX_PROBLEMS {
            out.truncate(MAX_PROBLEMS);
            break;
        }
    }
    // The crate and this host must never disagree on "valid".
    if out.is_empty() && !validator.is_valid(data) {
        out.push("the data can't be checked".to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deck_schema() -> Value {
        json!({
            "$schema": DRAFT,
            "type": "object",
            "additionalProperties": false,
            "required": ["title", "cards"],
            "properties": {
                "title": { "type": "string", "minLength": 1, "maxLength": 10,
                           "pattern": "^[^\\u0000-\\u001F\u{200b}]*$" },
                "level": { "enum": ["easy", "hard"] },
                "count": { "type": "integer", "minimum": 1, "maximum": 5 },
                "cards": { "type": "array", "minItems": 1, "maxItems": 2,
                           "items": { "$ref": "#/$defs/card" } }
            },
            "$defs": {
                "card": { "type": "object", "additionalProperties": false,
                          "required": ["front"],
                          "properties": { "front": { "type": "string", "maxLength": 5 } } }
            }
        })
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    #[test]
    fn valid_data_passes() {
        let s = deck_schema();
        check_schema(&s).expect("schema");
        let ok = json!({"title": "Hi", "level": "easy", "count": 3, "cards": [{"front": "a"}]});
        assert_eq!(validate(&s, &ok), Vec::<String>::new());
        assert!(
            validate(
                &s,
                &json!({"title": "Hi", "count": 2.0, "cards": [{"front": "a"}]})
            )
            .is_empty()
        );
    }

    #[test]
    fn problems_are_reported_in_plain_words_with_paths() {
        let s = deck_schema();
        let bad = json!({
            "title": "Line\nbreak!!",
            "level": "medium",
            "count": 9,
            "cards": [{"front": "ok"}, {"front": "too long"}, {"front": "x"}],
            "colour": "red",
            "we\u{202e}ird key": 1
        });
        let problems = validate(&s, &bad);
        for expected in [
            "title is longer than 10 characters",
            "title contains a line break, tab or invisible character, which isn't allowed",
            "level should be one of: \"easy\", \"hard\"",
            "count should be at most 5",
            "cards has more than 2 items",
            "cards[1].front is longer than 5 characters",
            "colour isn't a known field",
            "[\"we?ird key\"] isn't a known field",
        ] {
            assert!(
                problems.iter().any(|p| p == expected),
                "{expected} in {problems:?}"
            );
        }
        assert_eq!(
            sorted(validate(&s, &json!({"cards": [{}]}))),
            vec!["cards[0].front is missing", "title is missing"]
        );
        assert_eq!(
            validate(&s, &json!([])),
            vec!["the data should be an object"]
        );
        let either = json!({"type": ["string", "null"]});
        assert_eq!(
            validate(&either, &json!(1)),
            vec!["the data should be text or null"]
        );
        assert_eq!(
            validate(&json!({"multipleOf": 5}), &json!(7)),
            vec!["the data should be a multiple of 5"]
        );
        assert_eq!(
            validate(&json!({"const": "a"}), &json!("b")),
            vec!["the data should be \"a\""]
        );
        assert_eq!(
            validate(&json!({"type": "string", "pattern": "^a$"}), &json!("b")),
            vec!["the data doesn't have the expected form"]
        );
    }

    #[test]
    fn unsupported_or_broken_schemas_are_refused() {
        for bad in [
            json!([]),
            json!({"type": "object", "format": "uri"}),
            json!({"properties": {"a": {"patternProperties": {}}}}),
            json!({"$ref": "https://example.com/schema.json"}),
            json!({"$ref": "file:///etc/passwd"}),
            json!({"$ref": "other.json#/$defs/a"}),
            json!({"$ref": "#/$defs/missing"}),
            json!({"$dynamicRef": "#a"}),
            json!({"$id": "https://example.com/s.json"}),
            json!({"properties": {"a": {"$id": "urn:x"}}}),
            json!({"properties": {"a": {"$defs": {}}}}),
            json!({"$schema": "http://json-schema.org/draft-07/schema#"}),
            json!({"type": "text"}),
            json!({"pattern": "(?=a)b"}),
            json!({"pattern": "\\bword"}),
            json!({"pattern": "(a)\\1"}),
            json!({"pattern": "(?i)a"}),
            json!({"maxLength": -1}),
            json!({"anyOf": []}),
        ] {
            assert!(check_schema(&bad).is_err(), "{bad}");
            assert!(
                !validate(&bad, &json!("x")).is_empty(),
                "fails closed: {bad}"
            );
        }
    }

    #[test]
    fn patterns_keep_their_javascript_meaning() {
        let digits = compile("^\\d+$").expect("compile");
        assert!(digits.is_match("123"));
        assert!(
            !digits.is_match("١٢٣"),
            "\\d is ASCII-only, as in JavaScript"
        );
        let word = compile("^[\\w-]+$").expect("compile");
        assert!(word.is_match("a_b-1"));
        assert!(!word.is_match("é"));
        assert!(
            compile("^#[0-9a-fA-F]{6}$")
                .expect("hex")
                .is_match("#A0b1C2")
        );
        assert!(compile("^(?<n>a)$").is_ok());
        assert!(compile("[a&&b]").expect("set").is_match("&"));
        // The crate's own `pattern` is replaced: `.` must not match line terminators.
        let dot = json!({"type": "string", "pattern": "^.$"});
        for bad in ["\r", "\u{2028}", "\u{2029}", "\n"] {
            assert!(!validate(&dot, &json!(bad)).is_empty(), "{bad:?}");
        }
        assert!(validate(&dot, &json!("\u{85}")).is_empty());
    }

    #[test]
    fn combinators_and_conditionals_work() {
        let s = json!({
            "anyOf": [{"type": "string"}, {"type": "integer"}],
            "not": {"const": 0}
        });
        check_schema(&s).expect("schema");
        assert!(validate(&s, &json!("x")).is_empty());
        assert_eq!(
            validate(&s, &json!(true)),
            vec!["the data doesn't match any of the allowed shapes"]
        );
        assert_eq!(
            validate(&s, &json!(0)),
            vec!["the data has a form that isn't allowed"]
        );
        let one = json!({"oneOf": [{"type": "integer"}, {"minimum": 0}]});
        assert_eq!(
            validate(&one, &json!(1)),
            vec!["the data matches more than one allowed shape"]
        );
        let cond = json!({
            "type": "object",
            "if": {"properties": {"kind": {"const": "timed"}}, "required": ["kind"]},
            "then": {"required": ["seconds"]}
        });
        check_schema(&cond).expect("schema");
        assert_eq!(
            validate(&cond, &json!({"kind": "timed"})),
            vec!["seconds is missing"]
        );
        assert!(validate(&cond, &json!({"kind": "free"})).is_empty());
        assert_eq!(
            validate(&json!(false), &json!(1)),
            vec!["can't be checked: the data schema must be an object"]
        );
        assert_eq!(
            validate(&json!({"properties": {"a": false}}), &json!({"a": 1})),
            vec!["a isn't allowed"]
        );
    }

    #[test]
    fn dot_and_whitespace_match_javascript() {
        let dot = compile("^.{1,5}$").expect("dot");
        for bad in ["a\rb", "a\nb", "a\u{2028}b", "a\u{2029}b", ""] {
            assert!(!dot.is_match(bad), "{bad:?}");
        }
        assert!(dot.is_match("a\u{85}b"), "U+0085 is not a line end in JS");
        assert!(dot.is_match("\u{1F600}"), "an emoji is one character");
        assert!(compile("^[.]$").expect("class").is_match("."));
        assert!(!compile("^[.]$").expect("class").is_match("a"));
        assert!(!compile("^a\\.b$").expect("esc").is_match("axb"));

        let space = compile("^\\s$").expect("space");
        assert!(!space.is_match("\u{85}"), "U+0085 is not JS whitespace");
        assert!(space.is_match("\u{feff}"), "U+FEFF is JS whitespace");
        for ok in [" ", "\t", "\u{a0}", "\u{2028}", "\u{3000}"] {
            assert!(space.is_match(ok), "{ok:?}");
        }
        assert!(!space.is_match("\u{200b}"));
        let non = compile("^\\S$").expect("non-space");
        assert!(non.is_match("\u{85}") && !non.is_match("\u{feff}") && non.is_match("a"));
        let in_class = compile("^[\\s]$").expect("class");
        assert!(in_class.is_match("\u{feff}") && !in_class.is_match("\u{85}"));
        let neg = compile("^[\\S]$").expect("class");
        assert!(neg.is_match("\u{85}") && !neg.is_match("\u{feff}"));
        let not_space = compile("^[^\\s]$").expect("class");
        assert!(not_space.is_match("\u{85}") && !not_space.is_match("\u{feff}"));
    }

    #[test]
    fn reference_cycles_are_refused() {
        for s in [
            json!({"$ref": "#"}),
            json!({"$defs": {"a": {"anyOf": [{"$ref": "#/$defs/a"}, {"$ref": "#/$defs/a"}]}},
                   "$ref": "#/$defs/a"}),
            json!({"$defs": {"a": {"items": {"$ref": "#/$defs/b"}}, "b": {"not": {"$ref": "#/$defs/a"}}},
                   "$ref": "#/$defs/a"}),
        ] {
            let err = check_schema(&s).expect_err("cycle");
            assert!(err.contains("loop"), "{err}");
        }
    }

    #[test]
    fn exponential_schemas_are_refused_before_validation() {
        // 2^28 paths through shared definitions: the crate has no work budget, so the host
        // refuses the schema by its unfolded size instead of running it.
        let mut defs = Map::new();
        for i in 0..28 {
            let next = format!("#/$defs/l{}", i + 1);
            defs.insert(
                format!("l{i}"),
                json!({"anyOf": [{"$ref": next}, {"$ref": next}]}),
            );
        }
        defs.insert("l28".into(), json!({"type": "string"}));
        let s = json!({"$defs": defs, "$ref": "#/$defs/l0"});
        let started = std::time::Instant::now();
        let err = check_schema(&s).expect_err("too large");
        assert!(err.contains("too large"), "{err}");
        assert!(!validate(&s, &json!(1)).is_empty());
        assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
        // A large enum counts too.
        let big: Vec<Value> = (0..3_000).map(|i| json!(i)).collect();
        assert!(check_schema(&json!({"enum": big})).is_err());
        // Deep nesting is refused before anything is unfolded.
        let mut deep = json!({"type": "string"});
        for _ in 0..40 {
            deep = json!({"items": deep});
        }
        assert!(check_schema(&deep).is_err());
    }

    #[test]
    fn unique_items_is_not_quadratic() {
        let s = json!({"type": "array", "uniqueItems": true});
        let big: Vec<Value> = (0..100_000).map(|i| json!({"n": i, "t": "x"})).collect();
        let started = std::time::Instant::now();
        assert!(validate(&s, &Value::Array(big.clone())).is_empty());
        let mut dup = big;
        dup.push(json!({"t": "x", "n": 7.0}));
        assert_eq!(
            validate(&s, &Value::Array(dup)),
            vec!["the data has duplicate items"]
        );
        assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
        assert!(validate(&s, &json!([1, "1", [1], {"a": 1}])).is_empty());
        assert!(!validate(&s, &json!([{"a": 1, "b": 2}, {"b": 2, "a": 1}])).is_empty());
    }

    #[test]
    fn multiple_of_is_exact() {
        let s = json!({"multipleOf": 1});
        check_schema(&s).expect("schema");
        assert!(!validate(&s, &json!(1.0000000001)).is_empty());
        assert!(validate(&s, &json!(3)).is_empty());
        assert!(validate(&s, &json!(3.0)).is_empty());
        let five = json!({"multipleOf": 5});
        assert!(validate(&five, &json!(-10)).is_empty());
        assert!(!validate(&five, &json!(11)).is_empty());
        for bad in [json!(0.5), json!(0), json!(-2), json!(1e300), json!("2")] {
            assert!(check_schema(&json!({"multipleOf": bad})).is_err(), "{bad}");
        }
    }

    #[test]
    fn many_problems_are_capped() {
        let s = json!({"type": "array", "items": {"type": "string"}});
        let data: Vec<Value> = (0..100).map(|i| json!(i)).collect();
        let problems = validate(&s, &Value::Array(data));
        assert_eq!(problems.len(), MAX_PROBLEMS);
        assert_eq!(problems[0], "[0] should be text");
    }
}
