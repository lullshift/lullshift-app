//! Marketplace commands (Sprint 08, ADR-015). The contract with the UI is
//! `docs/MARKETPLACE_COMMANDS.md`.
//!
//! Everything a marketplace repository, its index and its activities contain is untrusted. This
//! module only orchestrates: the `marketplace` crate fetches, validates and writes, the store
//! records, and `activities::installed` decides what may run. Rules kept here:
//!
//! - Git work runs on blocking threads, one operation per marketplace at a time (`busy`).
//! - Nothing is installed or updated without a single-use, 10-minute token that a preview
//!   produced; the token holds the already validated files in memory.
//! - Third-party code (`kind: app` from any marketplace but the built-in one) is installed, or
//!   updated to a new commit, only when the command carries `acceptCodeRisk: true`: the user
//!   ticked the consent box under the review's risk warning (owner decision 27.5). Without it
//!   the command refuses and the token is used up. The consent is recorded with its commit.
//! - Errors reach the UI as `{ kind, message }` with fixed, friendly text. `git` stderr, URLs
//!   with userinfo and raw parser messages never get that far (the crate does not keep them).
//! - The app never logs URLs, credentials or task data.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use local_store::{
    ActivityKind, AddressChange, BUILTIN_MARKETPLACE_ID, CodeConsent, InstalledActivity,
    MarketplaceRecord, NewInstalledActivity, SavedState, StoreError,
};
use marketplace::manifest::{parse_manifest, plain};
use marketplace::validate::{check_marketplace_name, windows_reserved};
use marketplace::{
    CodeRiskStatus, Commit, EntrySource, Error, Git, GitFailureKind, GitRef, GitUrl, IndexError,
    InstalledSummary, Kind, MarketplaceRef, Os, PackError, PrepareRequest, PreparedPack, UrlError,
    code_risk_status, diff_update, install, needs_code_consent, remove_install,
};
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::Core;
use crate::activities::installed::{BUILTIN_ID, Problem, install_root, verify_for_enable};
use crate::activities::{available_engines, bundled_listed};

/// How long a preview's token stays valid.
pub const TOKEN_TTL: Duration = Duration::from_secs(10 * 60);
/// Most previews held at once; the oldest is dropped first.
const MAX_PENDING: usize = 8;
/// Longest plain-text description or owner kept from an index.
const META_MAX: usize = 500;

// ---- errors ------------------------------------------------------------------------------

/// What every marketplace command returns on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: &'static str,
    pub message: String,
    /// Only for `moved`: where the repository's server says it went (decision 18.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moved: Option<MovedDto>,
}

/// A redirect Lullshift did not follow. `url` is https with no userinfo, already checked by the
/// same rules as a typed address; the UI shows it as text and sends it back only if the user
/// accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedDto {
    pub url: String,
    pub same_host: bool,
}

impl CommandError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            moved: None,
        }
    }

    fn moved(to: &GitUrl, same_host: bool) -> Self {
        let shown = to.display();
        let message = if same_host {
            format!(
                "This marketplace moved to {shown} on the same server. Lullshift didn't follow the redirect. Use the new address?"
            )
        } else {
            format!(
                "This repository now points to a different server: {shown}. Lullshift didn't follow it. If you trust that address, add it or use Change address."
            )
        };
        Self {
            kind: "moved",
            message,
            moved: Some(MovedDto {
                url: shown.to_owned(),
                same_host,
            }),
        }
    }

    fn internal() -> Self {
        Self::new("internal", "Something went wrong inside Lullshift.")
    }

    fn busy() -> Self {
        Self::new(
            "busy",
            "Another operation on this marketplace is still running. Try again in a moment.",
        )
    }
}

type Res<T> = Result<T, CommandError>;

fn from_git_kind(kind: GitFailureKind) -> CommandError {
    let (code, message) = match kind {
        GitFailureKind::Offline => (
            "offline",
            "Can't reach the repository. Check your network connection.",
        ),
        GitFailureKind::AuthDenied => (
            "authDenied",
            "Access was denied. For a private repository, set up Git credentials on this computer first.",
        ),
        GitFailureKind::NotFound => ("notFound", "The repository, branch or tag wasn't found."),
        GitFailureKind::Timeout => ("timeout", "Git took too long and was stopped."),
        GitFailureKind::GitMissing => (
            "gitMissing",
            "Git isn't installed, or isn't on your PATH. Install Git to use marketplaces.",
        ),
        // Decision 28: the built-in Git reads public https marketplaces only.
        GitFailureKind::NeedsSystemGit => (
            "needsGit",
            "Install Git to use this marketplace. Lullshift's built-in Git only reads public https marketplaces; this one asks for a sign-in, uses ssh, or is only reachable through a proxy.",
        ),
        GitFailureKind::OutputTooLarge => (
            "invalidPack",
            "The repository returned more data than Lullshift allows.",
        ),
        GitFailureKind::Failed => ("internal", "Git couldn't complete the request."),
    };
    CommandError::new(code, message)
}

/// Maps a crate error to the contract. The crate's own messages are fixed text or sanitized
/// names (never `git` stderr or URLs); index problems get a fixed sentence because an index
/// entry can carry any text.
impl From<Error> for CommandError {
    fn from(e: Error) -> Self {
        match e {
            Error::Url(UrlError::PrivateNotConfirmed) => private_host_unconfirmed(),
            Error::Url(u) => Self::new("invalidUrl", u.to_string()),
            Error::Ref(_) | Error::Path(_) | Error::Name(_) => Self::new(
                "invalidIndex",
                "The marketplace lists a branch, path or name that isn't allowed.",
            ),
            Error::Git(k) => from_git_kind(k),
            Error::Index(IndexError::Missing) => Self::new(
                "invalidIndex",
                "This repository has no .lullshift/marketplace.json, so it isn't a Lullshift marketplace.",
            ),
            Error::Index(IndexError::Name(_)) => Self::new(
                "invalidIndex",
                "This marketplace uses the name 'lullshift', which is reserved for the built-in one.",
            ),
            Error::Index(_) => Self::new(
                "invalidIndex",
                "The marketplace index isn't valid, so nothing was added or changed.",
            ),
            Error::Pack(PackError::NotFound) => Self::new(
                "notFound",
                "The activity's folder wasn't found in the repository.",
            ),
            Error::Pack(PackError::Unreadable | PackError::Inconsistent) => Self::internal(),
            Error::Pack(p) => Self::new(
                "invalidPack",
                format!("This activity can't be installed: {p}."),
            ),
            Error::Io { .. } => Self::new(
                "internal",
                "Lullshift couldn't read or write files for this marketplace.",
            ),
            Error::NotACommit => Self::new("notFound", "The repository didn't return a commit."),
            Error::Moved { to, same_host } => Self::moved(&to, same_host),
        }
    }
}

impl From<StoreError> for CommandError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::AlreadyExists => {
                Self::new("alreadyExists", "That is already in your library.")
            }
            StoreError::NotFound => Self::new("notFound", "That wasn't found."),
            StoreError::BuiltinProtected => Self::new(
                "builtinProtected",
                "Built-in items can't be removed. You can turn individual activities off instead.",
            ),
            _ => Self::new("internal", "Lullshift couldn't read or save its data."),
        }
    }
}

/// The address is (or resolves to) this computer or a private network and the user hasn't
/// confirmed that (decision 17). The UI shows the confirmation checkbox on this kind.
fn private_host_unconfirmed() -> CommandError {
    CommandError::new(
        "privateHost",
        "This address is on your own computer or local network. If that's what you expect, tick \"This marketplace is on my own computer or network\" and try again.",
    )
}

/// Third-party code without the user's consent (owner decision 27.5). The UI only sends the
/// install or update once the box is ticked, so this means the flag was missing.
fn code_risk_not_accepted() -> CommandError {
    CommandError::new(
        "codeRiskNotAccepted",
        "This is someone else's program. It is installed only after you read the warning and tick \"I understand and want to install this program\". Nothing was installed; review it again.",
    )
}

fn already_in_library() -> CommandError {
    CommandError::new(
        "alreadyExists",
        "An activity with this id is already in your library. Use update to change an installed one.",
    )
}

fn with_store<T>(
    core: &Core,
    f: impl FnOnce(&mut local_store::Store) -> Result<T, StoreError>,
) -> Res<T> {
    let mut guard = core.store.lock().map_err(|_| CommandError::internal())?;
    let store = guard.as_mut().ok_or_else(|| {
        CommandError::new("internal", "Lullshift's local storage isn't available.")
    })?;
    f(store).map_err(CommandError::from)
}

// ---- state -------------------------------------------------------------------------------

enum PendingKind {
    Install,
    Update { old_commit: String },
}

struct Pending {
    kind: PendingKind,
    marketplace_id: String,
    source_path: String,
    pack: PreparedPack,
    created: Instant,
}

/// In-memory state of the marketplace manager.
#[derive(Default)]
pub struct MarketState {
    git: Mutex<Option<Arc<Git>>>,
    busy: Mutex<HashSet<String>>,
    pending: Mutex<Vec<(String, Pending)>>,
}

/// Holds one marketplace's "operation running" slot; released on drop.
struct BusyGuard<'a> {
    state: &'a MarketState,
    id: String,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut busy) = self.state.busy.lock() {
            busy.remove(&self.id);
        }
    }
}

/// The busy-set key for writes to one activity id, across marketplaces. Marketplace names can't
/// contain `:`, so it can't collide with one.
fn activity_key(activity_id: &str) -> String {
    format!("activity:{activity_id}")
}

/// Starts an operation on a marketplace, or `busy` if one is already running.
fn begin<'a>(core: &'a Core, id: &str) -> Res<BusyGuard<'a>> {
    let mut busy = core
        .market
        .busy
        .lock()
        .map_err(|_| CommandError::internal())?;
    if !busy.insert(id.to_owned()) {
        return Err(CommandError::busy());
    }
    Ok(BusyGuard {
        state: &core.market,
        id: id.to_owned(),
    })
}

fn make_git(app_dir: &Path) -> Result<Git, Error> {
    #[cfg(test)]
    return Git::for_tests(app_dir);
    #[cfg(not(test))]
    return Git::new(app_dir);
}

fn git(core: &Core) -> Res<Arc<Git>> {
    let mut slot = core
        .market
        .git
        .lock()
        .map_err(|_| CommandError::internal())?;
    if let Some(g) = slot.as_ref() {
        return Ok(Arc::clone(g));
    }
    let g = Arc::new(make_git(&core.app_dir)?);
    *slot = Some(Arc::clone(&g));
    Ok(g)
}

fn random_hex(bytes: usize) -> Res<String> {
    let mut buf = vec![0_u8; bytes];
    getrandom::fill(&mut buf).map_err(|_| CommandError::internal())?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn put_pending(core: &Core, pending: Pending) -> Res<String> {
    let token = random_hex(16)?;
    let mut list = core
        .market
        .pending
        .lock()
        .map_err(|_| CommandError::internal())?;
    list.retain(|(_, p)| p.created.elapsed() <= TOKEN_TTL);
    while list.len() >= MAX_PENDING {
        list.remove(0);
    }
    list.push((token.clone(), pending));
    Ok(token)
}

/// Removes and returns the preview for `token` if it is still valid at `now`. A token works
/// once; an unknown, used or expired token is `notFound`.
fn take_pending(core: &Core, token: &str, now: Instant) -> Res<Pending> {
    let mut list = core
        .market
        .pending
        .lock()
        .map_err(|_| CommandError::internal())?;
    let taken = list
        .iter()
        .position(|(t, _)| t == token)
        .map(|i| list.remove(i).1);
    match taken {
        Some(p) if now.saturating_duration_since(p.created) <= TOKEN_TTL => Ok(p),
        _ => Err(CommandError::new(
            "notFound",
            "This review has expired or was already used. Open the activity again.",
        )),
    }
}

// ---- paths and sidecar -------------------------------------------------------------------

fn valid_market_id(id: &str) -> bool {
    check_marketplace_name(id, true).is_ok() && !windows_reserved(id)
}

fn markets_dir(core: &Core) -> PathBuf {
    core.app_dir.join("marketplaces")
}

fn cache_dir(core: &Core, id: &str) -> PathBuf {
    markets_dir(core).join(id)
}

/// What is remembered beside a marketplace's cache: the address `git` is given (the store only
/// keeps the display form, which drops an ssh user name), and the index's own description and
/// owner so listing needs no `git`. Re-validated on every read.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Sidecar {
    git_url: Option<String>,
    description: Option<String>,
    owner: Option<String>,
}

const SIDECAR: &str = "source.json";

fn read_sidecar(core: &Core, id: &str) -> Sidecar {
    let Ok(bytes) = std::fs::read(cache_dir(core, id).join(SIDECAR)) else {
        return Sidecar::default();
    };
    if bytes.len() > 16 * 1024 {
        return Sidecar::default();
    }
    let mut s: Sidecar = serde_json::from_slice(&bytes).unwrap_or_default();
    let clean = |t: Option<String>| t.filter(|t| plain(t, META_MAX));
    s.description = clean(s.description);
    s.owner = clean(s.owner);
    s
}

fn write_sidecar(dir: &Path, s: &Sidecar) -> Res<()> {
    let text = serde_json::to_vec(s).map_err(|_| CommandError::internal())?;
    std::fs::write(dir.join(SIDECAR), text).map_err(|_| {
        CommandError::new(
            "internal",
            "Lullshift couldn't write the marketplace cache.",
        )
    })
}

#[cfg(test)]
fn parse_git_url(text: &str) -> Result<GitUrl, marketplace::UrlError> {
    // Tests build marketplaces from local repositories.
    if let Some(rest) = text.strip_prefix("file://") {
        let path = if cfg!(windows) {
            rest.strip_prefix('/').unwrap_or(rest)
        } else {
            rest
        };
        return Ok(GitUrl::for_test_file(Path::new(path)));
    }
    GitUrl::parse(text)
}

#[cfg(not(test))]
fn parse_git_url(text: &str) -> Result<GitUrl, marketplace::UrlError> {
    GitUrl::parse(text)
}

/// The address `git` is given for a marketplace: the sidecar's, or else the stored display form
/// (which works for https and for ssh without a user name). Always re-validated. The sidecar's
/// address must name the same host as the stored one, and the private-network confirmation
/// comes only from the store (decision 17), never from the sidecar.
fn git_url_of(core: &Core, rec: &MarketplaceRecord) -> Res<GitUrl> {
    let stored = parse_git_url(&rec.url).ok();
    let from_sidecar = read_sidecar(core, &rec.id)
        .git_url
        .and_then(|u| parse_git_url(&u).ok())
        .filter(|u| stored.as_ref().is_none_or(|s| s.display() == u.display()));
    let url = from_sidecar.or(stored).ok_or_else(|| {
        CommandError::new(
            "invalidUrl",
            "Lullshift can't tell where this marketplace lives. Remove it and add it again.",
        )
    })?;
    Ok(if rec.private_host {
        url.confirm_private()
    } else {
        url
    })
}

// ---- DTOs --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceDto {
    id: String,
    url: String,
    builtin: bool,
    description: Option<String>,
    owner: Option<String>,
    last_fetched_commit: Option<String>,
    last_fetched_at: Option<String>,
    /// The user confirmed it is on their own computer or network (decision 17).
    private_host: bool,
}

fn rfc3339(t: OffsetDateTime) -> Option<String> {
    t.format(&Rfc3339).ok()
}

fn market_dto(core: &Core, rec: &MarketplaceRecord) -> MarketplaceDto {
    let (description, owner) = if rec.builtin {
        (
            Some("Activities that ship with Lullshift.".to_owned()),
            Some("Lullshift".to_owned()),
        )
    } else {
        let s = read_sidecar(core, &rec.id);
        (s.description, s.owner)
    };
    MarketplaceDto {
        id: rec.id.clone(),
        url: rec.url.clone(),
        builtin: rec.builtin,
        description,
        owner,
        last_fetched_commit: rec.last_fetched_commit.clone(),
        last_fetched_at: rec.last_fetched_at.and_then(rfc3339),
        private_host: rec.private_host,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMark {
    commit: String,
    version: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    activity_id: String,
    name: Option<String>,
    description: Option<String>,
    kind: Option<&'static str>,
    external: bool,
    featured: bool,
    installed: Option<InstalledMark>,
    /// Code from a third-party marketplace: installing it shows the risk warning and asks for
    /// consent (owner decision 27.5). Known up front only for same-repository entries.
    third_party_code: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreviewDto {
    token: String,
    marketplace_id: String,
    activity_id: String,
    name: String,
    version: String,
    description: String,
    source_url: String,
    source_path: Option<String>,
    commit: String,
    kind: &'static str,
    engine: Option<String>,
    permissions: Vec<String>,
    size_bytes: u64,
    file_count: usize,
    third_party: bool,
    /// Third-party code only: what the risk warning says about this computer (owner decision
    /// 27.5). Installing or updating it needs `acceptCodeRisk: true`. Null otherwise.
    code_risk: Option<CodeRiskDto>,
    /// Saved progress from an earlier install of this id exists and this install will use it.
    has_saved_state: bool,
    /// Kept saved progress for this id: `none`, `own` (same marketplace and source, used),
    /// `unknown` (no recorded source, kept and used), or `foreign` (recorded as another
    /// marketplace's or source's: deleted at install unless the user chooses to keep it).
    saved_state: &'static str,
}

/// What the review's risk warning needs to know about this computer. Never a guarantee.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRiskDto {
    /// `windows`, `macos` or `linux`.
    platform: &'static str,
    /// `windowsNotReadBack`, `unverified`, `blockFailed` or `notChecked`.
    status: CodeRiskStatus,
}

fn platform_str(os: Os) -> &'static str {
    match os {
        Os::Windows => "windows",
        Os::MacOs => "macos",
        Os::Linux => "linux",
    }
}

fn code_risk_of(pack: &PreparedPack) -> Option<CodeRiskDto> {
    let p = &pack.preview;
    needs_code_consent(p.kind, p.third_party).then(|| CodeRiskDto {
        platform: platform_str(Os::current()),
        status: code_risk_status(),
    })
}

fn saved_state_str(s: SavedState) -> &'static str {
    match s {
        SavedState::None => "none",
        SavedState::Own => "own",
        SavedState::Unknown => "unknown",
        SavedState::Foreign => "foreign",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FromDto {
    commit: String,
    version: String,
    permissions: Vec<String>,
}

/// Where an update comes from, when that differs from where the install came from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceChangedDto {
    old: String,
    new: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePreviewDto {
    #[serde(flatten)]
    preview: InstallPreviewDto,
    from: FromDto,
    added_permissions: Vec<String>,
    removed_permissions: Vec<String>,
    kind_changed: bool,
    engine_changed: bool,
    source_changed: Option<SourceChangedDto>,
    unchanged: bool,
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::App => "app",
        Kind::Content => "content",
    }
}

fn kind_of(k: ActivityKind) -> Kind {
    match k {
        ActivityKind::App => Kind::App,
        ActivityKind::Content => Kind::Content,
    }
}

fn preview_dto(
    token: String,
    marketplace_id: &str,
    source_path: &str,
    pack: &PreparedPack,
    saved: SavedState,
) -> InstallPreviewDto {
    let p = &pack.preview;
    // The pack was validated, so its manifest parses; the description is plain text.
    let description = pack
        .files
        .iter()
        .find(|f| f.path == "manifest.json")
        .and_then(|f| std::str::from_utf8(&f.bytes).ok())
        .and_then(parse_manifest)
        .map(|m| m.description)
        .unwrap_or_default();
    InstallPreviewDto {
        token,
        marketplace_id: marketplace_id.to_owned(),
        activity_id: p.activity_id.clone(),
        name: p.name.clone(),
        version: p.version.clone(),
        description,
        source_url: p.source_url.clone(),
        source_path: (!source_path.is_empty()).then(|| source_path.to_owned()),
        commit: p.commit.clone(),
        kind: kind_str(p.kind),
        engine: p.engine.clone(),
        permissions: p.permissions.clone(),
        size_bytes: p.size_bytes,
        file_count: p.file_count,
        third_party: p.third_party,
        code_risk: code_risk_of(pack),
        has_saved_state: matches!(saved, SavedState::Own | SavedState::Unknown),
        saved_state: saved_state_str(saved),
    }
}

// ---- operations --------------------------------------------------------------------------

/// All marketplaces, the built-in one first.
pub fn list(core: &Core) -> Res<Vec<MarketplaceDto>> {
    let records = with_store(core, |s| s.marketplaces())?;
    Ok(records.iter().map(|r| market_dto(core, r)).collect())
}

/// Adds a marketplace: fetch, validate the index, use its `name` as the id.
pub fn add(core: &Core, url: &GitUrl) -> Res<MarketplaceDto> {
    let git = git(core)?;
    let root = markets_dir(core);
    std::fs::create_dir_all(&root).map_err(|_| {
        CommandError::new(
            "internal",
            "Lullshift couldn't create its marketplace cache.",
        )
    })?;
    let staging = root.join(format!(".adding-{}", random_hex(8)?));
    let result = add_from(core, &git, url, &root, &staging);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn add_from(
    core: &Core,
    git: &Git,
    url: &GitUrl,
    root: &Path,
    staging: &Path,
) -> Res<MarketplaceDto> {
    let repo = staging.join("repo.git");
    let commit = git.fetch(&repo, url, None)?;
    // `lullshift` is refused here: only the built-in index may use it.
    let index = git.read_index(&repo, &commit, false)?;
    let id = index.name.clone();
    if !valid_market_id(&id) {
        return Err(CommandError::new(
            "invalidIndex",
            "The marketplace's name can't be used on this system.",
        ));
    }
    let _guard = begin(core, &id)?;
    write_sidecar(
        staging,
        &Sidecar {
            git_url: Some(url.as_str().to_owned()),
            description: index.description.clone(),
            owner: index.owner.clone(),
        },
    )?;
    let now = OffsetDateTime::now_utc();
    let commit_text = commit.to_string();
    // An id that already exists is left exactly as it was; anything this call created is undone
    // if a later step fails.
    if let Err(e) = with_store(core, |s| {
        s.add_marketplace_with_host(&id, url.display(), url.private_confirmed(), now)
    }) {
        // AlreadyExists is either "already added" or, for a name that was removed while its
        // installs stayed, "that name is tied to another address" (review M1).
        let active = with_store(core, |s| s.marketplace(&id)).ok().flatten();
        return Err(match active {
            _ if e.kind != "alreadyExists" => e,
            // Already added from another address: moving it is an explicit step (18.4).
            Some(rec) if rec.url != url.display() => CommandError::new(
                "alreadyExists",
                "A marketplace with this name is already added from a different address. To move it to this address, use Change address on it.",
            ),
            Some(_) => e,
            None => CommandError::new(
                "alreadyExists",
                "A marketplace with this name was removed earlier, and activities installed from it still come from a different address. Uninstall them first, or add it again from its original address and then use Change address.",
            ),
        });
    }
    if let Err(e) = with_store(core, |s| s.record_fetch(&id, &commit_text, now)) {
        let _ = with_store(core, |s| s.remove_marketplace(&id, now));
        return Err(e);
    }
    let final_dir = root.join(&id);
    // A cache left behind by a marketplace that is no longer in the store is stale.
    let _ = std::fs::remove_dir_all(&final_dir);
    if std::fs::rename(staging, &final_dir).is_err() {
        let _ = with_store(core, |s| s.remove_marketplace(&id, now));
        return Err(CommandError::new(
            "internal",
            "Lullshift couldn't save the marketplace cache.",
        ));
    }
    let rec = with_store(core, |s| s.marketplace(&id))?.ok_or_else(CommandError::internal)?;
    Ok(market_dto(core, &rec))
}

/// Removes a marketplace and its cache. Installed activities stay.
pub fn remove(core: &Core, id: &str) -> Res<()> {
    if id == BUILTIN_MARKETPLACE_ID {
        return Err(StoreError::BuiltinProtected.into());
    }
    if !valid_market_id(id) {
        return Err(StoreError::NotFound.into());
    }
    let _guard = begin(core, id)?;
    with_store(core, |s| {
        s.remove_marketplace(id, OffsetDateTime::now_utc())
    })?;
    let _ = std::fs::remove_dir_all(cache_dir(core, id));
    Ok(())
}

fn active_market(core: &Core, id: &str) -> Res<MarketplaceRecord> {
    if !valid_market_id(id) {
        return Err(StoreError::NotFound.into());
    }
    with_store(core, |s| s.marketplace(id))?.ok_or_else(|| StoreError::NotFound.into())
}

/// Fetches the marketplace again. The built-in one has nothing to fetch.
#[cfg(test)]
pub fn refresh(core: &Core, id: &str) -> Res<MarketplaceDto> {
    refresh_confirming(core, id, false)
}

/// Like [`refresh`]; with `confirm_private` the user ticked "This marketplace is on my own
/// computer or network" after a refresh said its name now resolves to a private address
/// (decision 18.8). The confirmation is stored only once that fetch succeeded.
pub fn refresh_confirming(core: &Core, id: &str, confirm_private: bool) -> Res<MarketplaceDto> {
    let rec = active_market(core, id)?;
    if rec.builtin {
        return Ok(market_dto(core, &rec));
    }
    let _guard = begin(core, id)?;
    let git = git(core)?;
    let url = git_url_of(core, &rec)?;
    let newly_confirmed = confirm_private && !url.private_confirmed();
    let url = if confirm_private {
        url.confirm_private()
    } else {
        url
    };
    let repo = cache_dir(core, id).join("repo.git");
    let commit = git.fetch(&repo, &url, None)?;
    let index = git.read_index(&repo, &commit, false)?;
    if index.name != id {
        return Err(CommandError::new(
            "invalidIndex",
            "This marketplace now has a different name. Remove it and add it again if you trust the change.",
        ));
    }
    write_sidecar(
        &cache_dir(core, id),
        &Sidecar {
            git_url: Some(url.as_str().to_owned()),
            description: index.description,
            owner: index.owner,
        },
    )?;
    let now = OffsetDateTime::now_utc();
    let commit_text = commit.to_string();
    with_store(core, |s| s.record_fetch(id, &commit_text, now))?;
    if newly_confirmed {
        with_store(core, |s| s.set_marketplace_private(id, true))?;
    }
    let rec = active_market(core, id)?;
    Ok(market_dto(core, &rec))
}

/// What a change of address did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressChangedDto {
    marketplace: MarketplaceDto,
    /// Installed activities whose pinned commit wasn't found at the new address. They stay as
    /// they are (files, pin, saved progress) and are marked in Manage.
    not_found: Vec<String>,
}

/// Moves a marketplace to an address the user typed and confirmed after seeing the old and new
/// ones (decision 18.4). Network, only because the user asked:
///
/// 1. The new address is fetched into a staging cache with every usual check (private hosts,
///    resolved addresses, redirects reported, size cap). Its index must use the same name, so
///    a change can't swap in a different marketplace.
/// 2. Each commit that an install from this marketplace's own repository is pinned to is looked
///    up at the new address. Found: the install now names the new address as its source.
///    Missing: it stays exactly as it is, with a "not found at the new address" note.
///    A network or access failure stops the change and nothing is changed.
/// 3. The store is updated in one transaction, then the new cache replaces the old one.
///
/// Installed files, pins, enabled state and saved progress are never touched.
pub fn change_address(core: &Core, id: &str, new_url: &GitUrl) -> Res<AddressChangedDto> {
    if id == BUILTIN_MARKETPLACE_ID {
        return Err(StoreError::BuiltinProtected.into());
    }
    let rec = active_market(core, id)?;
    let old_url = git_url_of(core, &rec)?;
    if old_url.display() == new_url.display()
        && old_url.private_confirmed() == new_url.private_confirmed()
    {
        return Err(CommandError::new(
            "invalidUrl",
            "That is already this marketplace's address.",
        ));
    }
    let _guard = begin(core, id)?;
    let git = git(core)?;
    let root = markets_dir(core);
    let staging = root.join(format!(".changing-{}", random_hex(8)?));
    let result = change_into(core, &git, &rec, new_url, &staging);
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn change_into(
    core: &Core,
    git: &Git,
    rec: &MarketplaceRecord,
    new_url: &GitUrl,
    staging: &Path,
) -> Res<AddressChangedDto> {
    let id = rec.id.as_str();
    let repo = staging.join("repo.git");
    let commit = git.fetch(&repo, new_url, None)?;
    let index = git.read_index(&repo, &commit, false)?;
    if index.name != id {
        return Err(CommandError::new(
            "invalidIndex",
            "The repository at the new address is a different marketplace (it has another name), so the address wasn't changed.",
        ));
    }

    // Installs from the marketplace's own repository; external sources don't move with it.
    let installs: Vec<InstalledActivity> = with_store(core, |s| s.installed_activities())?
        .into_iter()
        .filter(|i| i.marketplace_id == id && i.source_url == rec.url)
        .collect();
    let mut found = Vec::new();
    let mut not_found = Vec::new();
    let mut checked: Vec<(String, bool)> = Vec::new();
    for (n, install) in installs.iter().enumerate() {
        let exists = match checked.iter().find(|(c, _)| *c == install.commit_sha) {
            Some((_, e)) => *e,
            None => {
                let pin = Commit::parse(&install.commit_sha).ok_or_else(CommandError::internal)?;
                let probe = staging.join(format!("pin-{n}")).join("repo.git");
                let exists = match git.fetch(&probe, new_url, Some(&GitRef::Commit(pin))) {
                    Ok(_) => true,
                    Err(Error::Git(GitFailureKind::NotFound) | Error::NotACommit) => false,
                    Err(e) => return Err(e.into()),
                };
                let _ = std::fs::remove_dir_all(staging.join(format!("pin-{n}")));
                checked.push((install.commit_sha.clone(), exists));
                exists
            }
        };
        if exists {
            found.push(install.activity_id.clone());
        } else {
            not_found.push(install.activity_id.clone());
        }
    }

    write_sidecar(
        staging,
        &Sidecar {
            git_url: Some(new_url.as_str().to_owned()),
            description: index.description.clone(),
            owner: index.owner.clone(),
        },
    )?;
    let commit_text = commit.to_string();
    with_store(core, |s| {
        s.change_marketplace_address(&AddressChange {
            id,
            new_url: new_url.display(),
            private_host: new_url.private_confirmed(),
            commit: &commit_text,
            found: &found,
            missing: &not_found,
            at: OffsetDateTime::now_utc(),
        })
    })?;
    // The new cache replaces the old one. If this fails the store already points at the new
    // address and the next Refresh fetches it again; the old cache is only a cache.
    let final_dir = cache_dir(core, id);
    let _ = std::fs::remove_dir_all(&final_dir);
    let _ = std::fs::create_dir_all(&final_dir);
    let _ = std::fs::rename(&repo, final_dir.join("repo.git"));
    let _ = std::fs::rename(staging.join(SIDECAR), final_dir.join(SIDECAR));
    refresh_library(core);
    let rec = active_market(core, id)?;
    Ok(AddressChangedDto {
        marketplace: market_dto(core, &rec),
        not_found,
    })
}

/// The entries of a marketplace as of its last fetch. No network.
pub fn entries(core: &Core, id: &str) -> Res<Vec<EntryDto>> {
    let rec = active_market(core, id)?;
    if rec.builtin {
        return Ok(bundled_listed()
            .into_iter()
            .map(|(m, kind)| EntryDto {
                activity_id: m.id,
                name: Some(m.name),
                description: (!m.description.is_empty()).then_some(m.description),
                kind: Some(kind),
                external: false,
                featured: false,
                installed: None,
                third_party_code: false,
            })
            .collect());
    }
    let commit = rec
        .last_fetched_commit
        .as_deref()
        .and_then(Commit::parse)
        .ok_or_else(|| {
            CommandError::new(
                "notFound",
                "This marketplace hasn't been fetched yet. Refresh it first.",
            )
        })?;
    let git = git(core)?;
    let repo = cache_dir(core, id).join("repo.git");
    let index = git.read_index(&repo, &commit, false)?;
    let local: Vec<&marketplace::RelPath> = index
        .entries
        .iter()
        .filter_map(|e| match &e.source {
            EntrySource::Local { path } => Some(path),
            EntrySource::Git { .. } => None,
        })
        .collect();
    let mut manifests = git.read_manifests(&repo, &commit, &local)?.into_iter();
    let installs = with_store(core, |s| s.installed_activities())?;
    Ok(index
        .entries
        .iter()
        .map(|e| {
            let manifest = match &e.source {
                EntrySource::Local { .. } => manifests.next().flatten(),
                EntrySource::Git { .. } => None,
            };
            let kind = manifest
                .as_ref()
                .map(|m| if m.kind == "app" { "app" } else { "content" });
            EntryDto {
                activity_id: e.id.clone(),
                name: manifest.as_ref().map(|m| m.name.clone()),
                description: manifest
                    .as_ref()
                    .map(|m| m.description.clone())
                    .filter(|d| !d.is_empty()),
                kind,
                external: matches!(e.source, EntrySource::Git { .. }),
                featured: e.featured,
                installed: installs
                    .iter()
                    .find(|i| i.activity_id == e.id && i.marketplace_id == id)
                    .map(|i| InstalledMark {
                        commit: i.commit_sha.clone(),
                        version: i.manifest_version.clone(),
                        enabled: i.enabled,
                    }),
                third_party_code: kind == Some("app"),
            }
        })
        .collect())
}

/// Resolves and validates one entry at the marketplace's last fetched commit. Fetches an
/// external source if the entry has one; writes nothing into the install folder.
fn prepare(
    core: &Core,
    git: &Git,
    rec: &MarketplaceRecord,
    activity_id: &str,
) -> Res<(PreparedPack, String)> {
    let commit = rec
        .last_fetched_commit
        .as_deref()
        .and_then(Commit::parse)
        .ok_or_else(|| {
            CommandError::new(
                "notFound",
                "This marketplace hasn't been fetched yet. Refresh it first.",
            )
        })?;
    let url = git_url_of(core, rec)?;
    let cache = cache_dir(core, &rec.id);
    let repo = cache.join("repo.git");
    let index = git.read_index(&repo, &commit, false)?;
    let entry = index.entry(activity_id).ok_or_else(|| {
        CommandError::new("notFound", "This marketplace doesn't list that activity.")
    })?;
    let external_root = cache.join("external");
    let resolved = git
        .resolve_entry(
            &MarketplaceRef {
                repo: &repo,
                commit: &commit,
                url: &url,
                external_cache_root: &external_root,
            },
            entry,
        )
        .map_err(|e| match e {
            // An activity's own source repository is never confirmed as private: only the
            // marketplace the user added can be (decision 17).
            Error::Url(UrlError::PrivateNotConfirmed | UrlError::SpecialAddress) => {
                CommandError::new(
                    "invalidIndex",
                    "This activity's source repository is on a private network address, so Lullshift won't fetch it.",
                )
            }
            // Only the marketplace's own address can be changed by the user.
            Error::Moved { to, .. } => CommandError::new(
                "invalidIndex",
                format!(
                    "This activity's source repository moved to {}. Lullshift didn't follow it; the marketplace needs to list the new address.",
                    to.display()
                ),
            ),
            other => other.into(),
        })?;
    let source_path = resolved
        .path
        .as_ref()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_default();
    let pack = git.prepare_pack(
        &resolved,
        &PrepareRequest {
            marketplace: &rec.id,
            builtin: false,
            entry_id: activity_id,
            engines: &available_engines(),
        },
    )?;
    Ok((pack, source_path))
}

fn shadowed_by_bundled(id: &str) -> bool {
    crate::activities::catalog().contains_key(id)
}

/// Reviews an install: fetches and validates, installs nothing, and returns a token.
pub fn preview_install(
    core: &Core,
    marketplace_id: &str,
    activity_id: &str,
) -> Res<InstallPreviewDto> {
    if marketplace_id == BUILTIN_ID {
        // The built-in marketplace's activities are the bundled ones.
        return Err(already_in_library());
    }
    let rec = active_market(core, marketplace_id)?;
    if shadowed_by_bundled(activity_id) {
        return Err(already_in_library());
    }
    let _guard = begin(core, marketplace_id)?;
    if with_store(core, |s| s.installed_activity(activity_id))?.is_some() {
        return Err(already_in_library());
    }
    let git = git(core)?;
    let (pack, source_path) = prepare(core, &git, &rec, activity_id)?;
    // Foreign state is deleted at install unless the user keeps it in the review (18.6).
    let saved = with_store(core, |s| {
        s.saved_state_for(activity_id, &rec.id, &pack.preview.source_url)
    })?;
    let token = put_pending(
        core,
        Pending {
            kind: PendingKind::Install,
            marketplace_id: rec.id.clone(),
            source_path: source_path.clone(),
            pack: pack.clone(),
            created: Instant::now(),
        },
    )?;
    Ok(preview_dto(token, &rec.id, &source_path, &pack, saved))
}

/// Refuses third-party code unless the user accepted the risk for this review, and returns the
/// consent to record (owner decision 27.5). Content packs and built-in code need none.
fn code_consent_for(pending: &Pending, accept_code_risk: bool) -> Res<Option<CodeConsent>> {
    let p = &pending.pack.preview;
    if !needs_code_consent(p.kind, p.third_party) {
        return Ok(None);
    }
    if !accept_code_risk {
        return Err(code_risk_not_accepted());
    }
    Ok(Some(CodeConsent {
        at: OffsetDateTime::now_utc(),
        commit: p.commit.clone(),
    }))
}

fn record_of(pending: &Pending, code_consent: Option<CodeConsent>) -> NewInstalledActivity {
    let p = &pending.pack.preview;
    NewInstalledActivity {
        activity_id: p.activity_id.clone(),
        marketplace_id: pending.marketplace_id.clone(),
        source_url: p.source_url.clone(),
        source_path: pending.source_path.clone(),
        commit_sha: p.commit.clone(),
        manifest_version: p.version.clone(),
        kind: match p.kind {
            Kind::App => ActivityKind::App,
            Kind::Content => ActivityKind::Content,
        },
        engine: p.engine.clone(),
        permissions: p.permissions.clone(),
        digest: p.digest.clone(),
        size_bytes: p.size_bytes,
        code_consent,
    }
}

/// Rebuilds what the library shows after an install, update, uninstall or switch.
fn refresh_library(core: &Core) {
    core.library.invalidate();
    let _ = core.library.snapshot(core);
}

/// Installs a previewed activity. `keep_saved_state` is the user's answer when the review said
/// kept progress belongs to another marketplace or source (`savedState: "foreign"`): `true`
/// keeps it for this install, `false` (the default) deletes it. It changes nothing otherwise.
/// `accept_code_risk` is the consent box under the third-party code warning; third-party code is
/// refused without it (and the token is used up), and anything else ignores it.
pub fn install_pending(
    core: &Core,
    token: &str,
    keep_saved_state: bool,
    accept_code_risk: bool,
    now: Instant,
) -> Res<()> {
    let pending = take_pending(core, token, now)?;
    if !matches!(pending.kind, PendingKind::Install) {
        return Err(CommandError::new(
            "notFound",
            "This review is for an update, not an install.",
        ));
    }
    let _guard = begin(core, &pending.marketplace_id)?;
    active_market(core, &pending.marketplace_id)?;
    let p = &pending.pack.preview;
    // One install or update of an id at a time, whichever marketplace it comes from.
    let _activity_guard = begin(core, &activity_key(&p.activity_id))?;
    if shadowed_by_bundled(&p.activity_id)
        || with_store(core, |s| s.installed_activity(&p.activity_id))?.is_some()
    {
        return Err(already_in_library());
    }
    let consent = code_consent_for(&pending, accept_code_risk)?;
    let root = install_root(core);
    install(&root, &pending.pack)?;
    // Saved state kept from a different marketplace or source is deleted here, not handed to
    // this code (review L7), unless the user chose to keep it in the review (decision 18.6).
    let source_url = p.source_url.clone();
    with_store(core, |s| {
        s.claim_saved_state(
            &p.activity_id,
            &pending.marketplace_id,
            &source_url,
            keep_saved_state,
        )
    })
    .inspect_err(|_| {
        let _ = remove_install(&root, &p.activity_id, &p.commit);
    })?;
    let recorded = with_store(core, |s| {
        s.put_installed_activity(&record_of(&pending, consent), OffsetDateTime::now_utc())
    });
    if let Err(e) = recorded {
        let _ = remove_install(&root, &p.activity_id, &p.commit);
        return Err(e);
    }
    refresh_library(core);
    Ok(())
}

fn summary_of(rec: &InstalledActivity) -> InstalledSummary {
    InstalledSummary {
        version: rec.manifest_version.clone(),
        commit: rec.commit_sha.clone(),
        source_url: rec.source_url.clone(),
        kind: kind_of(rec.kind),
        engine: rec.engine.clone(),
        permissions: rec.permissions.clone(),
    }
}

/// Reviews an update from the marketplace's last fetched index (the UI refreshes first).
pub fn preview_update(core: &Core, activity_id: &str) -> Res<UpdatePreviewDto> {
    let Some(rec) = with_store(core, |s| s.installed_activity(activity_id))? else {
        return Err(if shadowed_by_bundled(activity_id) {
            StoreError::BuiltinProtected.into()
        } else {
            StoreError::NotFound.into()
        });
    };
    let market = active_market(core, &rec.marketplace_id).map_err(|e| {
        if e.kind == "notFound" {
            CommandError::new(
                "notFound",
                "The marketplace this came from has been removed, so it can't be updated.",
            )
        } else {
            e
        }
    })?;
    let _guard = begin(core, &market.id)?;
    let git = git(core)?;
    let (pack, source_path) = prepare(core, &git, &market, activity_id)?;
    let diff = diff_update(&summary_of(&rec), &pack.preview);
    let token = put_pending(
        core,
        Pending {
            kind: PendingKind::Update {
                old_commit: rec.commit_sha.clone(),
            },
            marketplace_id: market.id.clone(),
            source_path: source_path.clone(),
            pack: pack.clone(),
            created: Instant::now(),
        },
    )?;
    Ok(UpdatePreviewDto {
        preview: preview_dto(token, &market.id, &source_path, &pack, SavedState::None),
        from: FromDto {
            commit: rec.commit_sha.clone(),
            version: rec.manifest_version.clone(),
            permissions: rec.permissions.clone(),
        },
        added_permissions: diff.added_permissions,
        removed_permissions: diff.removed_permissions,
        kind_changed: diff.kind_change.is_some(),
        engine_changed: diff.engine_change.is_some(),
        source_changed: diff
            .source_changed
            .map(|(old, new)| SourceChangedDto { old, new }),
        unchanged: diff.unchanged,
    })
}

/// Applies a previewed update: new commit folder first, then the record, then the old folder.
/// An update that changes the pinned commit of third-party code needs `accept_code_risk` again,
/// like an install; the new consent replaces the old one in the record.
pub fn update_pending(core: &Core, token: &str, accept_code_risk: bool, now: Instant) -> Res<()> {
    let pending = take_pending(core, token, now)?;
    let PendingKind::Update { old_commit } = &pending.kind else {
        return Err(CommandError::new(
            "notFound",
            "This review is for an install, not an update.",
        ));
    };
    let _guard = begin(core, &pending.marketplace_id)?;
    active_market(core, &pending.marketplace_id)?;
    let p = &pending.pack.preview;
    let _activity_guard = begin(core, &activity_key(&p.activity_id))?;
    let current = with_store(core, |s| s.installed_activity(&p.activity_id))?;
    let Some(current) = current
        .filter(|c| c.commit_sha == *old_commit && c.marketplace_id == pending.marketplace_id)
    else {
        return Err(CommandError::new(
            "notFound",
            "This activity changed since the review. Review the update again.",
        ));
    };
    if p.commit == current.commit_sha {
        return Ok(());
    }
    let consent = code_consent_for(&pending, accept_code_risk)?;
    let root = install_root(core);
    install(&root, &pending.pack)?;
    let recorded = with_store(core, |s| {
        s.put_installed_activity(&record_of(&pending, consent), OffsetDateTime::now_utc())
    });
    if let Err(e) = recorded {
        let _ = remove_install(&root, &p.activity_id, &p.commit);
        return Err(e);
    }
    // The old commit's folder goes only after the record points at the new one.
    let _ = remove_install(&root, &p.activity_id, &current.commit_sha);
    refresh_library(core);
    Ok(())
}

/// Uninstalls an installed activity. Saved state goes only if asked.
pub fn uninstall(core: &Core, activity_id: &str, delete_saved_state: bool) -> Res<()> {
    let Some(rec) = with_store(core, |s| s.installed_activity(activity_id))? else {
        return Err(if shadowed_by_bundled(activity_id) {
            StoreError::BuiltinProtected.into()
        } else {
            StoreError::NotFound.into()
        });
    };
    let _guard = begin(core, &rec.marketplace_id)?;
    let _activity_guard = begin(core, &activity_key(activity_id))?;
    with_store(core, |s| {
        s.remove_installed_activity(activity_id, OffsetDateTime::now_utc())
    })?;
    if delete_saved_state {
        with_store(core, |s| s.clear_activity_state(activity_id))?;
    }
    let root = install_root(core);
    let removed = remove_install(&root, activity_id, &rec.commit_sha);
    refresh_library(core);
    // Leftovers of an interrupted update, if any.
    if let Ok(dir) = marketplace::install_dir(&root, activity_id, &rec.commit_sha)
        && let Some(parent) = dir.parent()
    {
        let _ = std::fs::remove_dir_all(parent);
    }
    removed.map_err(|_| {
        CommandError::new(
            "internal",
            "The activity was removed, but some of its files couldn't be deleted.",
        )
    })
}

/// Turns an activity on or off: per activity for bundled ones, in the store for installed ones.
pub fn set_enabled(core: &Core, activity_id: &str, enabled: bool) -> Res<()> {
    let now = OffsetDateTime::now_utc();
    if bundled_listed().iter().any(|(m, _)| m.id == activity_id) {
        with_store(core, |s| s.set_builtin_disabled(activity_id, !enabled, now))?;
        refresh_library(core);
        return Ok(());
    }
    if shadowed_by_bundled(activity_id) {
        // An engine: hidden, never an item of its own.
        return Err(StoreError::NotFound.into());
    }
    let Some(rec) = with_store(core, |s| s.installed_activity(activity_id))? else {
        return Err(StoreError::NotFound.into());
    };
    if enabled {
        // Turning on re-checks the files; a copy that changed stays off.
        match verify_for_enable(core, &rec) {
            Ok(()) => {}
            Err(Problem::Tampered) => {
                return Err(CommandError::new(
                    "tampered",
                    "This activity's files have changed since it was installed, so it can't be turned on. Uninstall it and install it again.",
                ));
            }
            Err(Problem::Unreadable) => {
                return Err(CommandError::new(
                    "unreadable",
                    "This activity's files can't be read right now. Another program may be using them. Try again in a moment.",
                ));
            }
            Err(Problem::MissingEngine) => {
                // Allowed: it stays listed as unusable until Lullshift provides the engine.
            }
        }
    }
    with_store(core, |s| s.set_installed_enabled(activity_id, enabled, now))?;
    refresh_library(core);
    Ok(())
}

// ---- commands ----------------------------------------------------------------------------

async fn run<T: Send + 'static>(
    core: &Arc<Core>,
    f: impl FnOnce(&Core) -> Res<T> + Send + 'static,
) -> Res<T> {
    let core = Arc::clone(core);
    tauri::async_runtime::spawn_blocking(move || f(&core))
        .await
        .map_err(|_| CommandError::internal())?
}

#[tauri::command]
pub async fn marketplace_list(core: tauri::State<'_, Arc<Core>>) -> Res<Vec<MarketplaceDto>> {
    run(&core, list).await
}

/// Which Git marketplaces use (decision 28, ADR-023).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBackendDto {
    /// "system": the Git installed on this computer. "builtin": Lullshift's built-in Git,
    /// public https marketplaces only.
    pub backend: &'static str,
}

/// The Git in use, found once per run (installing Git takes effect after a restart).
pub fn git_backend(core: &Core) -> Res<GitBackendDto> {
    let git = git(core)?;
    Ok(GitBackendDto {
        backend: match git.backend() {
            marketplace::Backend::System => "system",
            marketplace::Backend::Builtin => "builtin",
        },
    })
}

#[tauri::command]
pub async fn marketplace_git_backend(core: tauri::State<'_, Arc<Core>>) -> Res<GitBackendDto> {
    run(&core, git_backend).await
}

#[tauri::command]
pub async fn marketplace_add(
    core: tauri::State<'_, Arc<Core>>,
    url: String,
    confirm_private: Option<bool>,
) -> Res<MarketplaceDto> {
    let url = user_url(&url, confirm_private.unwrap_or(false))?;
    run(&core, move |c| add(c, &url)).await
}

/// Parses an address the user typed. `confirm_private` is the "This marketplace is on my own
/// computer or network" checkbox (decision 17); it is recorded only when it was ticked.
fn user_url(text: &str, confirm_private: bool) -> Res<GitUrl> {
    let url = parse_git_url(text.trim()).map_err(|e| CommandError::from(Error::Url(e)))?;
    Ok(if confirm_private {
        url.confirm_private()
    } else {
        url
    })
}

#[tauri::command]
pub async fn marketplace_remove(core: tauri::State<'_, Arc<Core>>, id: String) -> Res<()> {
    run(&core, move |c| remove(c, &id)).await
}

#[tauri::command]
pub async fn marketplace_refresh(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    confirm_private: Option<bool>,
) -> Res<MarketplaceDto> {
    run(&core, move |c| {
        refresh_confirming(c, &id, confirm_private.unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn marketplace_change_address(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    url: String,
    confirm_private: Option<bool>,
) -> Res<AddressChangedDto> {
    let url = user_url(&url, confirm_private.unwrap_or(false))?;
    run(&core, move |c| change_address(c, &id, &url)).await
}

#[tauri::command]
pub async fn marketplace_entries(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Res<Vec<EntryDto>> {
    run(&core, move |c| entries(c, &id)).await
}

#[tauri::command]
pub async fn marketplace_preview_install(
    core: tauri::State<'_, Arc<Core>>,
    marketplace_id: String,
    activity_id: String,
) -> Res<InstallPreviewDto> {
    run(&core, move |c| {
        preview_install(c, &marketplace_id, &activity_id)
    })
    .await
}

/// `accept_code_risk` must be `Some(true)` for third-party code; missing counts as no.
#[tauri::command]
pub async fn marketplace_install(
    core: tauri::State<'_, Arc<Core>>,
    token: String,
    keep_saved_state: Option<bool>,
    accept_code_risk: Option<bool>,
) -> Res<()> {
    let keep = keep_saved_state.unwrap_or(false);
    let accept = accept_code_risk == Some(true);
    run(&core, move |c| {
        install_pending(c, &token, keep, accept, Instant::now())
    })
    .await
}

#[tauri::command]
pub async fn marketplace_preview_update(
    core: tauri::State<'_, Arc<Core>>,
    activity_id: String,
) -> Res<UpdatePreviewDto> {
    run(&core, move |c| preview_update(c, &activity_id)).await
}

/// `accept_code_risk` must be `Some(true)` to update third-party code to a new commit.
#[tauri::command]
pub async fn marketplace_update(
    core: tauri::State<'_, Arc<Core>>,
    token: String,
    accept_code_risk: Option<bool>,
) -> Res<()> {
    let accept = accept_code_risk == Some(true);
    run(&core, move |c| {
        update_pending(c, &token, accept, Instant::now())
    })
    .await
}

#[tauri::command]
pub async fn activity_uninstall(
    core: tauri::State<'_, Arc<Core>>,
    activity_id: String,
    delete_saved_state: bool,
) -> Res<()> {
    run(&core, move |c| {
        uninstall(c, &activity_id, delete_saved_state)
    })
    .await
}

#[tauri::command]
pub async fn activity_set_enabled(
    core: tauri::State<'_, Arc<Core>>,
    activity_id: String,
    enabled: bool,
) -> Res<()> {
    run(&core, move |c| set_enabled(c, &activity_id, enabled)).await
}

#[cfg(test)]
mod tests;
