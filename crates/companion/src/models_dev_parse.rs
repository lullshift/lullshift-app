//! Streaming reader for the models.dev document (security review MEDIUM-1).
//!
//! The body is never turned into a full `serde_json::Value` tree. Serde visitors walk it once,
//! keep only the few fields the UI needs, skip everything else with `IgnoredAny`, and stop
//! keeping entries once a cap is reached (providers, models per provider, models in total).
//! Strings are cut to a small size before they are copied. A body whose top level is not an
//! object is refused at its first byte.
//!
//! Leniency: a provider or model of the wrong JSON type, or with fields of the wrong type, is
//! skipped; only a body that is not valid JSON, or not an object at the top level, fails.

use std::collections::HashSet;
use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

use super::{
    CatalogModel, CatalogProvider, MAX_FAMILY_CHARS, MAX_MODELS_PER_PROVIDER, MAX_NAME_CHARS,
    MAX_PROVIDERS, MAX_TOKEN_LIMIT, clean_cost, clean_date, clean_text, valid_model_id,
    valid_provider_id,
};

/// Longest string copied out of the document, in bytes. Longer strings are cut (and marked, so a
/// cut id is never taken for a valid one).
const MAX_LEAF_BYTES: usize = 1024;
/// Longest object key kept, in bytes. Longer keys can't be valid ids and are ignored.
const MAX_KEY_BYTES: usize = 256;

fn cut(s: &str, max: usize) -> (&str, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], true)
}

/// A field value, reduced to what the parser can use.
#[derive(Debug)]
enum Leaf {
    /// The text, and whether it was cut.
    Str(String, bool),
    Bool(bool),
    UInt(u64),
    Num(f64),
    Null,
    /// An array or an object (skipped without keeping anything).
    Other,
}

impl Leaf {
    /// The whole string, if it was not cut.
    fn full_str(&self) -> Option<&str> {
        match self {
            Self::Str(s, false) => Some(s),
            _ => None,
        }
    }

    /// The string, cut or not (for display text that is cleaned and capped anyway).
    fn any_str(&self) -> Option<&str> {
        match self {
            Self::Str(s, _) => Some(s),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn as_u64(&self) -> Option<u64> {
        match self {
            Self::UInt(n) => Some(*n),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Self::UInt(n) => Some(*n as f64),
            Self::Num(n) => Some(*n),
            _ => None,
        }
    }
}

fn drain_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<(), A::Error> {
    while seq.next_element::<IgnoredAny>()?.is_some() {}
    Ok(())
}

fn drain_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<(), A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(())
}

impl<'de> de::Deserialize<'de> for Leaf {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Leaf;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Leaf, E> {
                Ok(Leaf::Bool(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Leaf, E> {
                Ok(Leaf::UInt(v))
            }
            #[allow(clippy::cast_precision_loss)]
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Leaf, E> {
                Ok(Leaf::Num(v as f64))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Leaf, E> {
                Ok(Leaf::Num(v))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Leaf, E> {
                let (s, was_cut) = cut(v, MAX_LEAF_BYTES);
                Ok(Leaf::Str(s.to_owned(), was_cut))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Leaf, E> {
                Ok(Leaf::Null)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Leaf, A::Error> {
                drain_seq(seq)?;
                Ok(Leaf::Other)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Leaf, A::Error> {
                drain_map(map)?;
                Ok(Leaf::Other)
            }
        }
        d.deserialize_any(V)
    }
}

/// An object key, kept only if short.
struct Key(Option<String>);

impl<'de> de::Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Key;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a key")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Key, E> {
                Ok(Key((v.len() <= MAX_KEY_BYTES).then(|| v.to_owned())))
            }
        }
        d.deserialize_str(V)
    }
}

/// Visitor methods for every JSON type other than the one expected: the value is skipped and
/// the result is `None`. `$seq` / `$map` say whether arrays or objects are also skipped here.
macro_rules! skip_scalars {
    () => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    };
}

macro_rules! skip_seq {
    () => {
        fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
            drain_seq(seq)?;
            Ok(None)
        }
    };
}

macro_rules! skip_map {
    () => {
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            drain_map(map)?;
            Ok(None)
        }
    };
}

/// An object from which a few named fields are picked (`limit`, `cost`).
struct Pick(&'static [&'static str]);

impl<'de> DeserializeSeed<'de> for Pick {
    type Value = Option<Vec<Option<Leaf>>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Pick {
    type Value = Option<Vec<Option<Leaf>>>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object")
    }
    skip_scalars!();
    skip_seq!();
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out: Vec<Option<Leaf>> = self.0.iter().map(|_| None).collect();
        while let Some(Key(k)) = map.next_key()? {
            let slot = k
                .as_deref()
                .and_then(|k| self.0.iter().position(|n| *n == k))
                .filter(|&i| out[i].is_none());
            match slot {
                Some(i) => out[i] = Some(map.next_value()?),
                None => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Some(out))
    }
}

/// `modalities.output`: whether the list has "text". `None` if absent or not a list.
struct OutputHasText;

impl<'de> DeserializeSeed<'de> for OutputHasText {
    type Value = Option<bool>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OutputHasText {
    type Value = Option<bool>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list")
    }
    skip_scalars!();
    skip_map!();
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut text = false;
        while let Some(leaf) = seq.next_element::<Leaf>()? {
            text |= leaf.full_str() == Some("text");
        }
        Ok(Some(text))
    }
}

struct Modalities;

impl<'de> DeserializeSeed<'de> for Modalities {
    type Value = Option<bool>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Modalities {
    type Value = Option<bool>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object")
    }
    skip_scalars!();
    skip_seq!();
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out = None;
        while let Some(Key(k)) = map.next_key()? {
            if k.as_deref() == Some("output") && out.is_none() {
                out = map.next_value_seed(OutputHasText)?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(out)
    }
}

#[derive(Default)]
struct RawModel {
    id: Option<Leaf>,
    name: Option<Leaf>,
    family: Option<Leaf>,
    kind: Option<Leaf>,
    tool_call: Option<Leaf>,
    reasoning: Option<Leaf>,
    release_date: Option<Leaf>,
    /// Whether `modalities.output` includes text, if it is a list.
    output_text: Option<bool>,
    limit: Option<Vec<Option<Leaf>>>,
    cost: Option<Vec<Option<Leaf>>>,
}

fn at(v: Option<&Vec<Option<Leaf>>>, i: usize) -> Option<&Leaf> {
    v.and_then(|v| v.get(i)).and_then(Option::as_ref)
}

impl RawModel {
    /// The slim model, or `None` if it is not a usable chat model.
    fn finish(self, key: Option<&str>) -> Option<CatalogModel> {
        let id = match &self.id {
            Some(Leaf::Str(..)) => self.id.as_ref().and_then(Leaf::full_str)?,
            _ => key?,
        };
        if !valid_model_id(id) {
            return None;
        }
        // Not for chat: a `type` other than chat or text, or output modalities without text.
        match &self.kind {
            None | Some(Leaf::Null) => {}
            Some(Leaf::Str(t, false)) if t == "chat" || t == "text" => {}
            Some(_) => return None,
        }
        if self.output_text == Some(false) {
            return None;
        }
        let token = |i| {
            at(self.limit.as_ref(), i)
                .and_then(Leaf::as_u64)
                .filter(|n| (1..=MAX_TOKEN_LIMIT).contains(n))
        };
        let price = |i| {
            at(self.cost.as_ref(), i)
                .and_then(Leaf::as_f64)
                .and_then(clean_cost)
        };
        Some(CatalogModel {
            id: id.to_owned(),
            name: self
                .name
                .as_ref()
                .and_then(Leaf::any_str)
                .and_then(|n| clean_text(n, MAX_NAME_CHARS)),
            family: self
                .family
                .as_ref()
                .and_then(Leaf::any_str)
                .and_then(|f| clean_text(f, MAX_FAMILY_CHARS)),
            tool_call: self
                .tool_call
                .as_ref()
                .and_then(Leaf::as_bool)
                .unwrap_or(false),
            reasoning: self
                .reasoning
                .as_ref()
                .and_then(Leaf::as_bool)
                .unwrap_or(false),
            release_date: self
                .release_date
                .as_ref()
                .and_then(Leaf::full_str)
                .and_then(clean_date),
            context_tokens: token(0),
            output_tokens: token(1),
            cost_input: price(0),
            cost_output: price(1),
        })
    }
}

struct Model;

impl<'de> DeserializeSeed<'de> for Model {
    type Value = Option<RawModel>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Model {
    type Value = Option<RawModel>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a model object")
    }
    skip_scalars!();
    skip_seq!();
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut m = RawModel::default();
        while let Some(Key(k)) = map.next_key()? {
            let slot = match k.as_deref() {
                Some("id") => &mut m.id,
                Some("name") => &mut m.name,
                Some("family") => &mut m.family,
                Some("type") => &mut m.kind,
                Some("tool_call") => &mut m.tool_call,
                Some("reasoning") => &mut m.reasoning,
                Some("release_date") => &mut m.release_date,
                Some("modalities") if m.output_text.is_none() => {
                    m.output_text = map.next_value_seed(Modalities)?;
                    continue;
                }
                Some("limit") if m.limit.is_none() => {
                    m.limit = map.next_value_seed(Pick(&["context", "output"]))?;
                    continue;
                }
                Some("cost") if m.cost.is_none() => {
                    m.cost = map.next_value_seed(Pick(&["input", "output"]))?;
                    continue;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            if slot.is_none() {
                *slot = Some(map.next_value()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(Some(m))
    }
}

/// The `models` object of one provider. Counts each kept model against the total budget.
struct Models<'a> {
    budget: &'a mut usize,
}

impl<'de> DeserializeSeed<'de> for Models<'_> {
    type Value = Option<Vec<CatalogModel>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Models<'_> {
    type Value = Option<Vec<CatalogModel>>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object of models")
    }
    skip_scalars!();
    skip_seq!();
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        while let Some(Key(k)) = map.next_key()? {
            if out.len() >= MAX_MODELS_PER_PROVIDER || *self.budget == 0 {
                map.next_value::<IgnoredAny>()?;
                continue;
            }
            let raw = map.next_value_seed(Model)?;
            if let Some(m) = raw.and_then(|r| r.finish(k.as_deref()))
                && seen.insert(m.id.clone())
            {
                out.push(m);
                *self.budget -= 1;
            }
        }
        Ok(Some(out))
    }
}

struct Provider<'a> {
    key: &'a str,
    budget: &'a mut usize,
}

impl<'de> DeserializeSeed<'de> for Provider<'_> {
    type Value = Option<CatalogProvider>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Provider<'_> {
    type Value = Option<CatalogProvider>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a provider object")
    }
    skip_scalars!();
    skip_seq!();
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut id: Option<Leaf> = None;
        let mut name: Option<Leaf> = None;
        let mut models: Option<Vec<CatalogModel>> = None;
        while let Some(Key(k)) = map.next_key()? {
            match k.as_deref() {
                Some("id") if id.is_none() => id = Some(map.next_value()?),
                Some("name") if name.is_none() => name = Some(map.next_value()?),
                Some("models") if models.is_none() => {
                    models = map.next_value_seed(Models {
                        budget: &mut *self.budget,
                    })?;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        let id = match &id {
            Some(Leaf::Str(..)) => id.as_ref().and_then(Leaf::full_str),
            _ => Some(self.key),
        };
        let (Some(id), Some(models)) = (id, models.filter(|m| !m.is_empty())) else {
            return Ok(None);
        };
        if !valid_provider_id(id) {
            return Ok(None);
        }
        Ok(Some(CatalogProvider {
            id: id.to_owned(),
            name: name
                .as_ref()
                .and_then(Leaf::any_str)
                .and_then(|n| clean_text(n, MAX_NAME_CHARS))
                .unwrap_or_else(|| id.to_owned()),
            models,
        }))
    }
}

/// The top level: an object of providers. `deserialize_map` makes serde_json refuse anything
/// else at the first byte.
pub(super) struct Root<'a> {
    pub(super) budget: &'a mut usize,
}

impl<'de> DeserializeSeed<'de> for Root<'_> {
    type Value = Vec<CatalogProvider>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Root<'_> {
    type Value = Vec<CatalogProvider>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object of providers")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut providers: Vec<CatalogProvider> = Vec::new();
        let mut seen = HashSet::new();
        while let Some(Key(k)) = map.next_key()? {
            let Some(key) = k.filter(|_| providers.len() < MAX_PROVIDERS && *self.budget > 0)
            else {
                map.next_value::<IgnoredAny>()?;
                continue;
            };
            let p = map.next_value_seed(Provider {
                key: &key,
                budget: &mut *self.budget,
            })?;
            if let Some(p) = p
                && seen.insert(p.id.clone())
            {
                providers.push(p);
            }
        }
        Ok(providers)
    }
}
