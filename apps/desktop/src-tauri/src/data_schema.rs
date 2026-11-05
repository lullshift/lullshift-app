//! Host-side check of a content pack's data against its engine's data schema (ADR-016).
//!
//! The CLI and `pnpm check` validate packs with Ajv. The app checks again before it starts a
//! pack, and cannot use Ajv: Ajv compiles schemas with `new Function`, which the app's CSP
//! (`script-src 'self'`, no `unsafe-eval`) rightly forbids. So the host carries this small,
//! fail-closed JSON Schema 2020-12 subset instead:
//!
//! - [`check_schema`] refuses any keyword outside [`ASSERTIONS`] and [`ANNOTATIONS`], any `$ref`
//!   other than `#` or `#/$defs/<name>`, and any pattern it can't compile with the same meaning
//!   as JavaScript's `u` flag. An engine whose schema is refused can't run packs.
//! - [`validate`] reports problems in the same plain words as the shared TypeScript formatter
//!   (`friendlyErrors` in `@lullshift/agent-protocol`), e.g. `cards[3].front is longer than 200
//!   characters`.
//!
//! `tests/activities/check-engine-compat.mjs` refuses the same keywords, and a Rust test runs
//! every engine's fixtures through this validator, so the two can't drift apart unnoticed.

use std::collections::HashMap;

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
/// Deepest nesting of `$ref`s and sub-schemas followed while validating.
const MAX_DEPTH: usize = 64;
/// Compiled size limit for one pattern.
const REGEX_SIZE: usize = 1 << 20;
/// Most schema nodes visited (and array items hashed) while validating one file. Combinators
/// over `$ref` cycles can otherwise fan out exponentially.
const MAX_STEPS: u64 = 500_000;
/// Largest `multipleOf`; every whole number up to this is exact in an f64.
const MAX_MULTIPLE: u64 = 1 << 53;
/// JavaScript's `\s` (WhiteSpace and LineTerminator): no U+0085, with U+FEFF.
const JS_SPACE: &str = r"\x{9}-\x{d}\x{20}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}";
/// What a `u`-mode `.` must not match: JavaScript line terminators.
const JS_LINE_END: &str = r"\n\r\x{2028}\x{2029}";

/// Checks that `schema` uses only what this validator understands. The error names the
/// first problem in plain words.
pub fn check_schema(schema: &Value) -> Result<(), String> {
    let root = schema
        .as_object()
        .ok_or("the data schema must be an object")?;
    if let Some(s) = root.get("$schema")
        && s.as_str() != Some(DRAFT)
    {
        return Err(format!("$schema must be {DRAFT}"));
    }
    check_node(schema, schema, "#", true)
}

fn check_node(root: &Value, node: &Value, at: &str, is_root: bool) -> Result<(), String> {
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
        if ANNOTATIONS.contains(&k.as_str()) {
            continue;
        }
        if !ASSERTIONS.contains(&k.as_str()) {
            return Err(format!("{here}: the app doesn't support the keyword {k}"));
        }
        match k.as_str() {
            "properties" | "$defs" => {
                let map = v.as_object().ok_or(format!("{here} must be an object"))?;
                for (name, sub) in map {
                    check_node(root, sub, &format!("{here}/{name}"), false)?;
                }
            }
            "additionalProperties" | "items" | "not" | "if" | "then" | "else" => {
                check_node(root, v, &here, false)?;
            }
            "allOf" | "anyOf" | "oneOf" => {
                let list = v
                    .as_array()
                    .filter(|l| !l.is_empty())
                    .ok_or(format!("{here} must be a non-empty list"))?;
                for (i, sub) in list.iter().enumerate() {
                    check_node(root, sub, &format!("{here}/{i}"), false)?;
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

/// One step into the data: an object field or a list position.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Index(usize),
}

/// Data paths the way people write them: `cards[3].front`. Field names that aren't simple are
/// quoted, cut short and stripped of hidden characters, since they come from untrusted data.
fn path_text(path: &[Step]) -> String {
    let mut out = String::new();
    for step in path {
        match step {
            Step::Index(i) => out.push_str(&format!("[{i}]")),
            Step::Key(k) => {
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
                            if crate::activities::hidden_char(c) || c == '"' {
                                '?'
                            } else {
                                c
                            }
                        })
                        .collect();
                    out.push_str(&format!("[\"{clean}\"]"));
                }
            }
        }
    }
    out
}

fn with(path: &[Step], step: Step) -> Vec<Step> {
    let mut p = path.to_vec();
    p.push(step);
    p
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

/// True for the shared plain-text pattern (no control, invisible or direction characters).
fn is_plain_text_pattern(p: &str) -> bool {
    p.starts_with(r"^[^\u0000-\u001F")
}

fn number_text(v: &Value) -> String {
    v.to_string()
}

struct Validator<'a> {
    root: &'a Value,
    regexes: HashMap<&'a str, Option<Regex>>,
    steps: u64,
    exhausted: bool,
}

/// Checks `data` against a schema that passed [`check_schema`]. Returns plain-language
/// problems (at most 20), or nothing if the data is valid.
pub fn validate(schema: &Value, data: &Value) -> Vec<String> {
    let mut v = Validator {
        root: schema,
        regexes: HashMap::new(),
        steps: 0,
        exhausted: false,
    };
    let mut out = Vec::new();
    v.node(schema, data, &[], 0, &mut out);
    if v.exhausted {
        // Fail closed: a partial answer could be "valid".
        return vec!["the data is too complex to check".to_owned()];
    }
    out.dedup();
    out
}

impl<'a> Validator<'a> {
    /// Spends `n` steps of the work budget; false once it has run out.
    fn spend(&mut self, n: u64) -> bool {
        self.steps = self.steps.saturating_add(n);
        if self.steps > MAX_STEPS {
            self.exhausted = true;
        }
        !self.exhausted
    }

    fn report(&self, out: &mut Vec<String>, path: &[Step], message: &str) {
        if out.len() >= MAX_PROBLEMS {
            return;
        }
        let at = path_text(path);
        out.push(if at.is_empty() {
            format!("the data {message}")
        } else {
            format!("{at} {message}")
        });
    }

    fn passes(&mut self, schema: &'a Value, data: &Value, path: &[Step], depth: usize) -> bool {
        let mut scratch = Vec::new();
        self.node(schema, data, path, depth, &mut scratch);
        scratch.is_empty() && !self.exhausted
    }

    fn regex(&mut self, pattern: &'a str) -> Option<&Regex> {
        self.regexes
            .entry(pattern)
            .or_insert_with(|| compile(pattern).ok())
            .as_ref()
    }

    #[allow(clippy::too_many_lines)]
    fn node(
        &mut self,
        schema: &'a Value,
        data: &Value,
        path: &[Step],
        depth: usize,
        out: &mut Vec<String>,
    ) {
        if !self.spend(1) {
            return;
        }
        if depth > MAX_DEPTH {
            self.report(out, path, "is nested too deeply to check");
            return;
        }
        let s = match schema {
            Value::Bool(true) => return,
            Value::Bool(false) => return self.report(out, path, "isn't allowed"),
            Value::Object(s) => s,
            _ => return self.report(out, path, "can't be checked (broken schema)"),
        };

        if let Some(r) = s.get("$ref").and_then(Value::as_str) {
            match resolve(self.root, r) {
                Some(target) => self.node(target, data, path, depth + 1, out),
                None => self.report(out, path, "can't be checked (broken schema)"),
            }
        }

        if let Some(t) = s.get("type") {
            let types: Vec<&str> = match t {
                Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
                t => t.as_str().into_iter().collect(),
            };
            if !types.iter().any(|t| is_type(data, t)) {
                let words: Vec<&str> = types.iter().map(|t| type_word(t)).collect();
                self.report(out, path, &format!("should be {}", words.join(" or ")));
                // Every other check assumes the right type; one clear message is enough.
                return;
            }
        }

        if let Some(list) = s.get("enum").and_then(Value::as_array)
            && !list.iter().any(|x| json_eq(x, data))
        {
            let shown: Vec<String> = list.iter().take(10).map(Value::to_string).collect();
            let more = if list.len() > 10 { ", …" } else { "" };
            self.report(
                out,
                path,
                &format!("should be one of: {}{more}", shown.join(", ")),
            );
        }
        if let Some(c) = s.get("const")
            && !json_eq(c, data)
        {
            self.report(out, path, &format!("should be {c}"));
        }

        match data {
            Value::String(text) => self.string(s, text, path, out),
            Value::Number(_) => self.number(s, data, path, out),
            Value::Array(items) => self.array(s, items, path, depth, out),
            Value::Object(obj) => self.object(s, obj, path, depth, out),
            _ => {}
        }

        if let Some(all) = s.get("allOf").and_then(Value::as_array) {
            for sub in all {
                self.node(sub, data, path, depth + 1, out);
            }
        }
        if let Some(any) = s.get("anyOf").and_then(Value::as_array)
            && !any
                .iter()
                .any(|sub| self.passes(sub, data, path, depth + 1))
        {
            self.report(out, path, "doesn't match any of the allowed shapes");
        }
        if let Some(one) = s.get("oneOf").and_then(Value::as_array) {
            let n = one
                .iter()
                .filter(|sub| self.passes(sub, data, path, depth + 1))
                .count();
            if n == 0 {
                self.report(out, path, "doesn't match any of the allowed shapes");
            } else if n > 1 {
                self.report(out, path, "matches more than one allowed shape");
            }
        }
        if let Some(not) = s.get("not")
            && self.passes(not, data, path, depth + 1)
        {
            self.report(out, path, "has a form that isn't allowed");
        }
        if let Some(cond) = s.get("if") {
            let branch = if self.passes(cond, data, path, depth + 1) {
                s.get("then")
            } else {
                s.get("else")
            };
            if let Some(b) = branch {
                self.node(b, data, path, depth + 1, out);
            }
        }
    }

    fn string(
        &mut self,
        s: &'a Map<String, Value>,
        text: &str,
        path: &[Step],
        out: &mut Vec<String>,
    ) {
        let len = text.chars().count() as u64;
        if let Some(min) = s.get("minLength").and_then(Value::as_u64)
            && len < min
        {
            let m = if min == 1 {
                "is empty".to_owned()
            } else {
                format!("is shorter than {min} characters")
            };
            self.report(out, path, &m);
        }
        if let Some(max) = s.get("maxLength").and_then(Value::as_u64)
            && len > max
        {
            self.report(out, path, &format!("is longer than {max} characters"));
        }
        if let Some(p) = s.get("pattern").and_then(Value::as_str) {
            let matched = self.regex(p).map(|r| r.is_match(text));
            match matched {
                Some(true) => {}
                Some(false) if is_plain_text_pattern(p) => self.report(
                    out,
                    path,
                    "contains a line break, tab or invisible character, which isn't allowed",
                ),
                Some(false) => self.report(out, path, "doesn't have the expected form"),
                None => self.report(out, path, "can't be checked (broken schema)"),
            }
        }
    }

    fn number(&self, s: &Map<String, Value>, data: &Value, path: &[Step], out: &mut Vec<String>) {
        let Some(n) = data.as_f64() else { return };
        if let Some(v) = s.get("multipleOf")
            && let Some(m) = multiple_of(v)
        {
            // Exact: whole-number data by integer remainder, anything else by exact fmod.
            let ok = if let Some(i) = data.as_u64() {
                i % m == 0
            } else if let Some(i) = data.as_i64() {
                i % (m as i64) == 0
            } else {
                n % (m as f64) == 0.0
            };
            if !ok {
                self.report(
                    out,
                    path,
                    &format!("should be a multiple of {}", number_text(v)),
                );
            }
        }
        let num = |k: &str| s.get(k).and_then(|v| v.as_f64().map(|f| (f, v)));
        if let Some((m, v)) = num("minimum")
            && n < m
        {
            self.report(out, path, &format!("should be at least {}", number_text(v)));
        }
        if let Some((m, v)) = num("maximum")
            && n > m
        {
            self.report(out, path, &format!("should be at most {}", number_text(v)));
        }
        if let Some((m, v)) = num("exclusiveMinimum")
            && n <= m
        {
            self.report(
                out,
                path,
                &format!("should be more than {}", number_text(v)),
            );
        }
        if let Some((m, v)) = num("exclusiveMaximum")
            && n >= m
        {
            self.report(
                out,
                path,
                &format!("should be less than {}", number_text(v)),
            );
        }
    }

    fn array(
        &mut self,
        s: &'a Map<String, Value>,
        items: &[Value],
        path: &[Step],
        depth: usize,
        out: &mut Vec<String>,
    ) {
        let len = items.len() as u64;
        if let Some(min) = s.get("minItems").and_then(Value::as_u64)
            && len < min
        {
            let noun = if min == 1 { "item" } else { "items" };
            self.report(out, path, &format!("needs at least {min} {noun}"));
        }
        if let Some(max) = s.get("maxItems").and_then(Value::as_u64)
            && len > max
        {
            self.report(out, path, &format!("has more than {max} items"));
        }
        let too_many = s
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max);
        if s.get("uniqueItems") == Some(&Value::Bool(true)) && !too_many {
            let mut seen = std::collections::HashSet::new();
            for x in items {
                // Hashing costs about the size of the item; charge for it.
                if !self.spend(1 + canonical_size(x) / 64) {
                    return;
                }
                if !seen.insert(canonical(x)) {
                    self.report(out, path, "has duplicate items");
                    break;
                }
            }
        }
        if let Some(item) = s.get("items") {
            for (i, x) in items.iter().enumerate() {
                if out.len() >= MAX_PROBLEMS || self.exhausted {
                    break;
                }
                self.node(item, x, &with(path, Step::Index(i)), depth + 1, out);
            }
        }
    }

    fn object(
        &mut self,
        s: &'a Map<String, Value>,
        obj: &Map<String, Value>,
        path: &[Step],
        depth: usize,
        out: &mut Vec<String>,
    ) {
        let len = obj.len() as u64;
        if let Some(min) = s.get("minProperties").and_then(Value::as_u64)
            && len < min
        {
            self.report(out, path, &format!("needs at least {min} fields"));
        }
        if let Some(max) = s.get("maxProperties").and_then(Value::as_u64)
            && len > max
        {
            self.report(out, path, &format!("has more than {max} fields"));
        }
        if let Some(req) = s.get("required").and_then(Value::as_array) {
            for name in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(name) {
                    self.report(out, &with(path, Step::Key(name.to_owned())), "is missing");
                }
            }
        }
        let props = s.get("properties").and_then(Value::as_object);
        for (k, v) in obj {
            if out.len() >= MAX_PROBLEMS {
                break;
            }
            let here = with(path, Step::Key(k.clone()));
            if let Some(sub) = props.and_then(|p| p.get(k)) {
                self.node(sub, v, &here, depth + 1, out);
            } else if let Some(extra) = s.get("additionalProperties") {
                if extra == &Value::Bool(false) {
                    self.report(out, &here, "isn't a known field");
                } else {
                    self.node(extra, v, &here, depth + 1, out);
                }
            }
        }
    }
}

fn is_type(v: &Value, t: &str) -> bool {
    match t {
        "string" => v.is_string(),
        "number" => v.is_number(),
        "integer" => {
            v.as_i64().is_some()
                || v.as_u64().is_some()
                || v.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        "boolean" => v.is_boolean(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "null" => v.is_null(),
        _ => false,
    }
}

/// A text form of `v` that is equal exactly when [`json_eq`] says so (numbers by value,
/// object fields in sorted order).
fn canonical(v: &Value) -> String {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            format!("n{:x}", if f == 0.0 { 0 } else { f.to_bits() })
        }
        Value::String(t) => format!("{t:?}"),
        Value::Array(l) => format!(
            "[{}]",
            l.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{k:?}:{}", canonical(&o[k])))
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        other => other.to_string(),
    }
}

/// Rough count of the values inside `v`, to charge hashing against the work budget.
fn canonical_size(v: &Value) -> u64 {
    match v {
        Value::Array(l) => 1 + l.iter().map(canonical_size).sum::<u64>(),
        Value::Object(o) => 1 + o.values().map(canonical_size).sum::<u64>(),
        _ => 1,
    }
}

/// JSON equality as JSON Schema defines it: 1 and 1.0 are the same number.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
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
            validate(&s, &json!({"cards": [{}]})),
            vec!["title is missing", "cards[0].front is missing"]
        );
        assert_eq!(
            validate(&s, &json!([])),
            vec!["the data should be an object"]
        );
    }

    #[test]
    fn unsupported_or_broken_schemas_are_refused() {
        for bad in [
            json!([]),
            json!({"type": "object", "format": "uri"}),
            json!({"properties": {"a": {"patternProperties": {}}}}),
            json!({"$ref": "https://example.com/schema.json"}),
            json!({"$ref": "#/$defs/missing"}),
            json!({"properties": {"a": {"$id": "urn:x"}}}),
            json!({"$schema": "http://json-schema.org/draft-07/schema#"}),
            json!({"type": "text"}),
            json!({"pattern": "(?=a)b"}),
            json!({"pattern": "\\bword"}),
            json!({"pattern": "(a)\\1"}),
            json!({"maxLength": -1}),
            json!({"anyOf": []}),
        ] {
            assert!(check_schema(&bad).is_err(), "{bad}");
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
    fn runaway_schemas_fail_closed_quickly() {
        let s = json!({"$defs": {"a": {"anyOf": [{"$ref": "#/$defs/a"}, {"$ref": "#/$defs/a"}]}},
                       "$ref": "#/$defs/a"});
        check_schema(&s).expect("schema");
        let started = std::time::Instant::now();
        let problems = validate(&s, &json!(1));
        assert_eq!(problems, vec!["the data is too complex to check"]);
        assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
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
        assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
        // Already over maxItems: report that alone and skip the duplicate scan.
        let capped = json!({"type": "array", "maxItems": 1, "uniqueItems": true});
        assert_eq!(
            validate(&capped, &json!([1, 1])),
            vec!["the data has more than 1 items"]
        );
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
    fn self_references_stop_at_a_depth_limit() {
        let s = json!({"$ref": "#"});
        check_schema(&s).expect("schema");
        assert_eq!(
            validate(&s, &json!(1)),
            vec!["the data is nested too deeply to check"]
        );
    }
}
