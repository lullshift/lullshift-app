//! `.lullshift/marketplace.json` parsing (ADR-002, ADR-015). Strict: unknown fields, wrong types
//! and every unsafe URL, ref, path or name are refused. Mirrors
//! `reference/MARKETPLACE_V1.schema.json`.

use serde::Deserialize;

use crate::error::{Error, IndexError, show};
use crate::manifest::{text_ok as plain_text_ok, valid_id};
use crate::validate::{GitRef, GitUrl, RelPath, check_marketplace_name};

/// Where the index lives in a marketplace repository.
pub const INDEX_PATH: &str = ".lullshift/marketplace.json";
/// Largest index accepted.
pub const INDEX_MAX_BYTES: usize = 1024 * 1024;
const ENTRIES_MAX: usize = 1000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawIndex {
    schema_version: u32,
    name: String,
    description: Option<String>,
    owner: Option<String>,
    activities: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    id: String,
    source: RawSource,
    #[serde(default)]
    featured: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawSource {
    Git(RawGit),
    Local(RawLocal),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGit {
    git: String,
    r#ref: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLocal {
    path: String,
}

/// Where an entry's folder is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntrySource {
    /// A folder in the marketplace repository itself.
    Local { path: RelPath },
    /// A folder (or the root, if `path` is None) of another repository.
    Git {
        url: GitUrl,
        git_ref: Option<GitRef>,
        path: Option<RelPath>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: String,
    pub source: EntrySource,
    pub featured: bool,
}

/// A validated marketplace index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceIndex {
    pub name: String,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub entries: Vec<IndexEntry>,
}

impl MarketplaceIndex {
    pub fn entry(&self, id: &str) -> Option<&IndexEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Parses and validates an index. `builtin` is true only when loading the app's own
/// marketplace; any other index may not use the name `lullshift`.
pub fn parse_index(bytes: &[u8], builtin: bool) -> Result<MarketplaceIndex, Error> {
    if bytes.len() > INDEX_MAX_BYTES {
        return Err(IndexError::TooLarge.into());
    }
    let raw: RawIndex = serde_json::from_slice(bytes).map_err(|e| {
        // serde_json messages can quote the offending value (for example a URL with a
        // token), so only the kind of problem and its position are kept.
        let what = match e.classify() {
            serde_json::error::Category::Data => "unexpected shape, type or field",
            serde_json::error::Category::Eof => "unexpected end",
            _ => "not valid JSON",
        };
        Error::Index(IndexError::Parse(format!(
            "{what} at line {}, column {}",
            e.line(),
            e.column()
        )))
    })?;
    if raw.schema_version != 1 {
        return Err(IndexError::UnsupportedSchemaVersion.into());
    }
    check_marketplace_name(&raw.name, builtin).map_err(|e| Error::Index(IndexError::Name(e)))?;
    let text_ok =
        |t: &Option<String>, max: usize| t.as_deref().is_none_or(|t| plain_text_ok(t, max));
    if !text_ok(&raw.description, 500) {
        return Err(IndexError::BadText("description").into());
    }
    if !text_ok(&raw.owner, 100) {
        return Err(IndexError::BadText("owner").into());
    }
    if raw.activities.len() > ENTRIES_MAX {
        return Err(IndexError::TooManyEntries.into());
    }
    let mut entries: Vec<IndexEntry> = Vec::with_capacity(raw.activities.len());
    for e in raw.activities {
        if !valid_id(&e.id) {
            return Err(IndexError::BadId(show(&e.id)).into());
        }
        if entries.iter().any(|x| x.id == e.id) {
            return Err(IndexError::DuplicateId(e.id).into());
        }
        let entry_err = |reason: &dyn std::fmt::Display| {
            Error::Index(IndexError::Entry {
                id: e.id.clone(),
                reason: reason.to_string(),
            })
        };
        let source = match e.source {
            RawSource::Local(l) => EntrySource::Local {
                path: RelPath::parse(&l.path).map_err(|x| entry_err(&x))?,
            },
            RawSource::Git(g) => EntrySource::Git {
                url: GitUrl::parse(&g.git).map_err(|x| entry_err(&x))?,
                git_ref: g
                    .r#ref
                    .as_deref()
                    .map(GitRef::parse)
                    .transpose()
                    .map_err(|x| entry_err(&x))?,
                path: g
                    .path
                    .as_deref()
                    .map(RelPath::parse)
                    .transpose()
                    .map_err(|x| entry_err(&x))?,
            },
        };
        entries.push(IndexEntry {
            id: e.id,
            source,
            featured: e.featured,
        });
    }
    Ok(MarketplaceIndex {
        name: raw.name,
        description: raw.description,
        owner: raw.owner,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures(sub: &str) -> Vec<(String, Vec<u8>)> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/marketplace")
            .join(sub);
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(Result::ok)
            .filter(|f| f.path().extension().is_some_and(|x| x == "json"))
            .map(|f| {
                (
                    f.file_name().to_string_lossy().into_owned(),
                    std::fs::read(f.path()).expect("read"),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// The schema fixtures are the shared contract: Rust must agree with Ajv on every one.
    #[test]
    fn schema_valid_fixtures_parse() {
        let all = fixtures("valid");
        assert!(all.len() >= 3);
        for (name, bytes) in all {
            // The fixtures include the built-in index shape, which may use the reserved name.
            parse_index(&bytes, true).unwrap_or_else(|e| panic!("valid/{name}: {e}"));
        }
    }

    #[test]
    fn schema_invalid_fixtures_are_refused() {
        let all = fixtures("invalid");
        assert!(all.len() >= 10);
        for (name, bytes) in all {
            assert!(
                parse_index(&bytes, false).is_err(),
                "invalid/{name} was accepted"
            );
        }
    }

    /// Valid for the schema (which cannot know who loads the index) but refused at run time.
    #[test]
    fn runtime_invalid_fixtures_are_refused() {
        let all = fixtures("runtime-invalid");
        assert!(!all.is_empty());
        for (name, bytes) in all {
            assert!(
                parse_index(&bytes, false).is_err(),
                "runtime-invalid/{name} was accepted"
            );
        }
    }

    #[test]
    fn lullshift_is_reserved_for_the_builtin_index() {
        let (_, bytes) = fixtures("runtime-invalid")
            .into_iter()
            .find(|(n, _)| n.contains("reserved"))
            .expect("fixture");
        assert_eq!(
            parse_index(&bytes, false).err(),
            Some(Error::Index(IndexError::Name(
                crate::error::NameError::Reserved
            )))
        );
        assert!(
            parse_index(&bytes, true).is_ok(),
            "the built-in index may use it"
        );
    }

    #[test]
    fn the_real_builtin_index_parses() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.lullshift/marketplace.json");
        let bytes = std::fs::read(path).expect("read");
        let index = parse_index(&bytes, true).expect("builtin index");
        assert!(index.entries.len() > 50);
        assert!(
            parse_index(&bytes, false).is_err(),
            "third parties can't claim the name"
        );
    }

    #[test]
    fn duplicates_unknown_fields_and_bad_ids_are_refused() {
        let ok = r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"path":"x"}}]}"#;
        assert!(parse_index(ok.as_bytes(), false).is_ok());
        for bad in [
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"path":"x"}},{"id":"a.b","source":{"path":"y"}}]}"#,
            r#"{"schemaVersion":1,"name":"acme","extra":1,"activities":[]}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"path":"x"},"extra":true}]}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"path":"x","extra":1}}]}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"path":"x","ref":"main"}}]}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"A.b","source":{"path":"x"}}]}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"git":"o/r","extra":1}}]}"#,
            r#"{"schemaVersion":2,"name":"acme","activities":[]}"#,
            r#"{"schemaVersion":1,"name":"acme","owner":"a\u202eb","activities":[]}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":"no"}"#,
            "not json",
            "",
        ] {
            assert!(parse_index(bad.as_bytes(), false).is_err(), "{bad}");
        }
        let huge = vec![b' '; INDEX_MAX_BYTES + 1];
        assert_eq!(
            parse_index(&huge, false).err(),
            Some(Error::Index(IndexError::TooLarge))
        );
    }

    fn index_with(description: &str) -> Vec<u8> {
        serde_json::json!({
            "schemaVersion": 1,
            "name": "acme",
            "description": description,
            "owner": description,
            "activities": [],
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn invisible_separators_fillers_and_mark_stacks_are_refused() {
        assert!(parse_index(&index_with("Plain, with caf\u{e9} and e\u{301}"), false).is_ok());
        // Up to four marks in a row are fine (real scripts stack a few).
        assert!(parse_index(&index_with("a\u{300}\u{301}\u{302}\u{303}"), false).is_ok());
        for c in [
            '\u{2028}',
            '\u{2029}',
            '\u{3164}',
            '\u{115F}',
            '\u{1160}',
            '\u{FFA0}',
            '\u{FFF9}',
            '\u{FFFA}',
            '\u{FFFB}',
            '\u{202E}',
            '\u{200B}',
            '\u{FEFF}',
            '\u{E0041}',
        ] {
            let text = format!("before{c}after");
            assert_eq!(
                parse_index(&index_with(&text), false).err(),
                Some(Error::Index(IndexError::BadText("description"))),
                "U+{:04X}",
                c as u32
            );
        }
        let zalgo = format!("a{}", "\u{300}".repeat(5));
        assert!(parse_index(&index_with(&zalgo), false).is_err());
        let spread = format!("a{}b{}", "\u{300}".repeat(3), "\u{301}".repeat(3));
        assert!(parse_index(&index_with(&spread), false).is_ok());
    }
    #[test]
    fn error_text_does_not_echo_urls_with_credentials() {
        let bad = r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"git":"https://u:SECRET@github.com/o/r"}}]}"#;
        let e = parse_index(bad.as_bytes(), false).expect_err("refused");
        assert!(!e.to_string().contains("SECRET"), "{e}");
        assert!(!format!("{e:?}").contains("SECRET"), "{e:?}");
        // serde would quote these values in its own message.
        for quoting in [
            r#"{"schemaVersion":1,"name":"acme","activities":"https://u:SECRET@x.example.com/o/r"}"#,
            r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"git":"x"},"https://u:SECRET@x.example.com/":1}]}"#,
            r#"{"schemaVersion":"https://u:SECRET@x.example.com/","name":"acme","activities":[]}"#,
        ] {
            let e = parse_index(quoting.as_bytes(), false).expect_err("refused");
            assert!(!format!("{e} {e:?}").contains("SECRET"), "{e:?}");
        }
    }
}
