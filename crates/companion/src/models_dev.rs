//! The Companion model list from models.dev (owner decision 32, ADR-020 amendment).
//!
//! - Fetched only when the user presses "Load model list" or "Refresh"; never on launch.
//! - One fixed URL ([`MODELS_DEV_URL`]); it is not configurable from any command.
//! - The same hardened client as provider calls: rustls with the platform verifier, no
//!   redirects, https only, a total timeout and a size cap read incrementally.
//! - The answer is untrusted input: read as a stream (no full JSON tree), parsed leniently per
//!   entry, ids checked, text cleaned, counts capped (also in total), and only the few fields the
//!   UI needs are kept.
//! - Kept in memory only, for the app run. Nothing here writes to disk or logs.
//! - Single-flight: callers that arrive while a load is running share its result.

use std::io::Read;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use crate::error::{CompanionError, ErrorKind};
use crate::http::{HttpConfig, agent, classify};
use crate::url_check::check_base_url;

/// The only address the model list is ever fetched from.
pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";
/// Total time for one fetch, as for provider calls.
pub const CATALOG_TIMEOUT: Duration = Duration::from_secs(60);
/// Largest body accepted (the real file was about 6 MB on 2026-11-07).
pub const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
/// Most providers kept.
pub const MAX_PROVIDERS: usize = 1_000;
/// Most models kept in total (the real file had 8,941 on 2026-11-07, at most 639 per provider).
pub const MAX_TOTAL_MODELS: usize = 20_000;
/// Most models kept per provider.
pub const MAX_MODELS_PER_PROVIDER: usize = 5_000;
/// Longest provider or model display name kept, in characters.
pub const MAX_NAME_CHARS: usize = 120;
/// Longest model family kept, in characters.
pub const MAX_FAMILY_CHARS: usize = 60;
/// Longest provider id kept, in bytes.
pub const MAX_PROVIDER_ID_BYTES: usize = 100;
const MAX_ETAG_BYTES: usize = 256;
/// Larger token limits are treated as unknown.
pub(crate) const MAX_TOKEN_LIMIT: u64 = 1_000_000_000;
/// Larger prices (USD per million tokens) are treated as unknown.
const MAX_COST: f64 = 1_000_000.0;

// ---- errors ------------------------------------------------------------------------------

fn offline() -> CompanionError {
    CompanionError::new(
        ErrorKind::Offline,
        "Can't reach models.dev. Check your network connection and try again.",
    )
}

fn timeout() -> CompanionError {
    CompanionError::new(
        ErrorKind::Timeout,
        "models.dev took too long to answer and the request was stopped.",
    )
}

fn too_large() -> CompanionError {
    CompanionError::new(
        ErrorKind::InvalidResponse,
        "The model list from models.dev was larger than expected (over 16 MiB), so it was not loaded.",
    )
}

fn not_json() -> CompanionError {
    CompanionError::new(
        ErrorKind::InvalidResponse,
        "The model list from models.dev wasn't in the expected format, so it was not loaded.",
    )
}

fn http_status(code: u16) -> CompanionError {
    CompanionError::new(
        ErrorKind::ProviderError,
        format!("models.dev answered with HTTP status {code}. Try again later."),
    )
}

/// The error for reading the list before it was loaded in this app run.
pub fn not_loaded() -> CompanionError {
    CompanionError::not_found(
        "The model list isn't loaded. Press \"Load model list from models.dev\" first.",
    )
}

/// The error for a provider id the loaded list does not have.
pub fn unknown_provider() -> CompanionError {
    CompanionError::not_found("The loaded model list has no provider with that id.")
}

fn transport(e: &ureq::Error) -> CompanionError {
    match classify(e).kind {
        ErrorKind::Timeout => timeout(),
        _ => offline(),
    }
}

// ---- the slim model list -----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogModel {
    /// Passes [`valid_model_id`], so it can be saved as a profile's model as is.
    pub id: String,
    /// Plain text, one line, at most [`MAX_NAME_CHARS`]. `None` when models.dev gives none
    /// (the UI shows the id); the id is not copied here, to keep the list small.
    pub name: Option<String>,
    pub family: Option<String>,
    pub tool_call: bool,
    pub reasoning: bool,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, as models.dev gives it.
    pub release_date: Option<String>,
    pub context_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// USD per million input tokens; finite and not negative.
    pub cost_input: Option<f64>,
    /// USD per million output tokens; finite and not negative.
    pub cost_output: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalog {
    /// Ordered by provider id. Providers without a usable chat model are left out.
    pub providers: Vec<CatalogProvider>,
}

impl Catalog {
    pub fn provider(&self, id: &str) -> Option<&CatalogProvider> {
        self.providers.iter().find(|p| p.id == id)
    }
}

/// The model-name rule for provider profiles: 1 to 200 bytes of ASCII letters, digits and
/// `. _ : / @ + -`.
pub fn valid_model_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/@+-".contains(&b))
}

pub(crate) fn valid_provider_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_PROVIDER_ID_BYTES
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Characters dropped from catalog names on top of `agent_core::text::is_invisible`: variation
/// selectors and the blank braille pattern. They stay local to this list on purpose: the shared
/// set mirrors the published JSON schemas, and variation selectors are part of normal emoji in
/// Companion replies.
fn hidden_in_names(c: char) -> bool {
    matches!(c,
        '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}' | '\u{2800}')
}

/// Combining diacritical marks (the ranges the marketplace manifest checks use).
fn combining_mark(c: char) -> bool {
    matches!(c,
        '\u{300}'..='\u{36F}' | '\u{483}'..='\u{489}' | '\u{591}'..='\u{5BD}'
        | '\u{610}'..='\u{61A}' | '\u{64B}'..='\u{65F}' | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

/// Most combining marks kept in a row, so a name can't stack marks over other text.
const MAX_COMBINING_RUN: usize = 2;

/// One line of plain text: controls and invisible characters removed, runs of whitespace made
/// one space, runs of combining marks cut to two, cut to `max` characters. `None` if nothing
/// is left.
pub fn clean_text(s: &str, max: usize) -> Option<String> {
    let mut out = String::new();
    let mut space = false;
    let mut marks = 0;
    for c in s.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
            marks = 0;
            continue;
        }
        if c.is_control() || agent_core::text::is_invisible(c) || hidden_in_names(c) {
            continue;
        }
        if combining_mark(c) {
            marks += 1;
            if marks > MAX_COMBINING_RUN || (out.is_empty() && !space) {
                continue;
            }
        } else {
            marks = 0;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(c);
    }
    let cut: String = out.chars().take(max).collect();
    let cut = cut.trim_end().to_owned();
    (!cut.is_empty()).then_some(cut)
}

/// A price kept only if it is finite, not negative and not absurd.
pub fn clean_cost(v: f64) -> Option<f64> {
    (v.is_finite() && (0.0..=MAX_COST).contains(&v)).then_some(v)
}

pub(crate) fn clean_date(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('-').collect();
    let lens: Vec<usize> = parts.iter().map(|p| p.len()).collect();
    let shape_ok = matches!(lens.as_slice(), [4] | [4, 2] | [4, 2, 2]);
    (shape_ok && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))).then(|| s.to_owned())
}

/// Reads the models.dev document as a stream (see `models_dev_parse.rs`): no full JSON tree is
/// built, and at most [`MAX_TOTAL_MODELS`] models are kept. Fails only when the body is not
/// JSON or its top level is not an object; a bad provider or model is skipped.
pub fn parse_catalog(bytes: &[u8]) -> Result<Catalog, CompanionError> {
    use serde::de::DeserializeSeed;
    let mut budget = MAX_TOTAL_MODELS;
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let mut providers = parse::Root {
        budget: &mut budget,
    }
    .deserialize(&mut de)
    .map_err(|_| not_json())?;
    de.end().map_err(|_| not_json())?;
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Catalog { providers })
}

// ---- fetching ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct CatalogLimits {
    pub timeout: Duration,
    pub max_bytes: usize,
}

impl Default for CatalogLimits {
    fn default() -> Self {
        Self {
            timeout: CATALOG_TIMEOUT,
            max_bytes: MAX_CATALOG_BYTES,
        }
    }
}

enum Fetched {
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

fn safe_etag(v: &str) -> Option<String> {
    (!v.is_empty() && v.len() <= MAX_ETAG_BYTES && v.bytes().all(|b| (0x20..0x7f).contains(&b)))
        .then(|| v.to_owned())
}

fn fetch(limits: &CatalogLimits, url: &str, etag: Option<&str>) -> Result<Fetched, CompanionError> {
    // The production URL is https; a loopback http URL exists only in tests.
    let loopback = check_base_url(url)
        .map_err(|_| CompanionError::internal())?
        .loopback;
    let cfg = HttpConfig {
        timeout: limits.timeout,
        ..HttpConfig::default()
    };
    let agent = agent(&cfg, loopback);
    let mut req = agent.get(url).header("accept", "application/json");
    if let Some(tag) = etag {
        req = req.header("if-none-match", tag);
    }
    let mut resp = req.call().map_err(|e| transport(&e))?;
    let status = resp.status().as_u16();
    if status == 304 {
        return Ok(Fetched::NotModified);
    }
    if !(200..300).contains(&status) {
        return Err(http_status(status));
    }
    let etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .and_then(safe_etag);
    if resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > limits.max_bytes)
    {
        return Err(too_large());
    }
    // Read at most cap + 1 bytes: one extra byte proves the body was too large.
    let limit = u64::try_from(limits.max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut reader = resp.body_mut().as_reader().take(limit);
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(timeout());
            }
            Err(_) => return Err(offline()),
        }
        if bytes.len() > limits.max_bytes {
            return Err(too_large());
        }
    }
    Ok(Fetched::Body { bytes, etag })
}

// ---- the in-memory cache -----------------------------------------------------------------

/// The list as last confirmed by models.dev in this app run.
#[derive(Debug)]
pub struct LoadedCatalog {
    pub catalog: Arc<Catalog>,
    pub etag: Option<String>,
    /// When models.dev sent this list (a 200).
    pub loaded_at: SystemTime,
    /// When models.dev last sent or confirmed (304) this list. Equal to `loaded_at` until a
    /// refresh finds no change.
    pub checked_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct LoadResult {
    pub loaded: Arc<LoadedCatalog>,
    /// True when models.dev answered 304 and the list in memory was kept.
    pub unchanged: bool,
    /// True when nothing was sent because the list was already in memory.
    pub from_memory: bool,
}

#[derive(Default)]
struct CacheState {
    loaded: Option<Arc<LoadedCatalog>>,
    in_flight: bool,
    /// Counts finished loads, so a waiter knows when the one it joined is done.
    generation: u64,
    last: Option<Result<LoadResult, CompanionError>>,
}

/// The model list for one app run. Memory only.
pub struct CatalogCache {
    url: String,
    limits: CatalogLimits,
    state: Mutex<CacheState>,
    done: Condvar,
}

impl Default for CatalogCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogCache {
    /// The production cache: [`MODELS_DEV_URL`] and the default limits.
    pub fn new() -> Self {
        Self::build(MODELS_DEV_URL, CatalogLimits::default())
    }

    fn build(url: &str, limits: CatalogLimits) -> Self {
        Self {
            url: url.to_owned(),
            limits,
            state: Mutex::new(CacheState::default()),
            done: Condvar::new(),
        }
    }

    /// For tests only: another URL (a loopback mock) and limits. Not compiled into release
    /// builds, so no command can point the cache anywhere else.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_url(url: &str, limits: CatalogLimits) -> Self {
        Self::build(url, limits)
    }

    fn lock(&self) -> Result<MutexGuard<'_, CacheState>, CompanionError> {
        self.state.lock().map_err(|_| CompanionError::internal())
    }

    /// The list in memory, if any. Never touches the network.
    pub fn cached(&self) -> Result<Option<Arc<LoadedCatalog>>, CompanionError> {
        Ok(self.lock()?.loaded.clone())
    }

    /// Returns the list, fetching it if it is not in memory or `refresh` is set. A refresh sends
    /// the stored ETag; a 304 keeps the list. A failed refresh keeps the old list in memory.
    pub fn load(&self, refresh: bool) -> Result<LoadResult, CompanionError> {
        let mut st = self.lock()?;
        if !refresh && let Some(l) = &st.loaded {
            return Ok(LoadResult {
                loaded: Arc::clone(l),
                unchanged: false,
                from_memory: true,
            });
        }
        if st.in_flight {
            let joined = st.generation;
            while st.in_flight && st.generation == joined {
                st = self.done.wait(st).map_err(|_| CompanionError::internal())?;
            }
            return st
                .last
                .clone()
                .unwrap_or_else(|| Err(CompanionError::internal()));
        }
        st.in_flight = true;
        let previous = st.loaded.clone();
        drop(st);

        let mut flight = Flight {
            cache: self,
            finished: false,
        };
        let etag = previous
            .as_ref()
            .filter(|_| refresh)
            .and_then(|p| p.etag.clone());
        let result = match fetch(&self.limits, &self.url, etag.as_deref()) {
            Ok(Fetched::NotModified) => match previous {
                Some(p) if etag.is_some() => Ok(LoadResult {
                    loaded: Arc::new(LoadedCatalog {
                        catalog: Arc::clone(&p.catalog),
                        etag: p.etag.clone(),
                        loaded_at: p.loaded_at,
                        checked_at: SystemTime::now(),
                    }),
                    unchanged: true,
                    from_memory: false,
                }),
                // A 304 to a request that had no ETag is not a valid answer.
                _ => Err(not_json()),
            },
            Ok(Fetched::Body { bytes, etag }) => {
                let now = SystemTime::now();
                parse_catalog(&bytes).map(|catalog| LoadResult {
                    loaded: Arc::new(LoadedCatalog {
                        catalog: Arc::new(catalog),
                        etag,
                        loaded_at: now,
                        checked_at: now,
                    }),
                    unchanged: false,
                    from_memory: false,
                })
            }
            Err(e) => Err(e),
        };
        flight.finish(result.clone());
        result
    }
}

/// Ends a load: stores its result and wakes the callers that joined it, also if the loading
/// thread unwinds.
struct Flight<'a> {
    cache: &'a CatalogCache,
    finished: bool,
}

impl Flight<'_> {
    fn finish(&mut self, result: Result<LoadResult, CompanionError>) {
        self.finished = true;
        let mut st = match self.cache.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Ok(r) = &result {
            st.loaded = Some(Arc::clone(&r.loaded));
        }
        st.last = Some(result);
        st.in_flight = false;
        st.generation = st.generation.wrapping_add(1);
        drop(st);
        self.cache.done.notify_all();
    }
}

impl Drop for Flight<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(Err(CompanionError::internal()));
        }
    }
}

#[path = "models_dev_parse.rs"]
mod parse;

#[cfg(test)]
#[path = "models_dev_tests.rs"]
mod tests;
