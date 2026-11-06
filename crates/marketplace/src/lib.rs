//! Lullshift Git marketplaces (Sprint 08, ADR-015).
//!
//! No Tauri, no UI, no database. The desktop crate calls this to add and refresh marketplaces,
//! review and install activities pinned to commits, verify them later, and describe updates.
//! The marketplace repository, its index and every file in it are untrusted input; see
//! `README.md` for the threat model and the order of checks.
//!
//! Typical flow:
//!
//! 1. [`Git::new`], then [`Git::fetch`] the marketplace into its cache and [`Git::read_index`].
//! 2. [`Git::resolve_entry`] pins an entry to a commit (fetching an external source if needed).
//! 3. [`Git::prepare_pack`] validates everything and returns an [`InstallPreview`] to show.
//! 4. After the user confirms, [`install`] writes it; [`verify_install`] re-checks it later.
//! 5. For updates, prepare the new commit and call [`diff_update`].

mod builtin;
pub mod data_schema;
pub mod error;
pub mod gate;
pub mod git;
pub mod index;
pub mod install;
pub mod manifest;
pub mod net;
pub mod pack;
pub mod pack_text;
pub mod redirect;
pub mod repo;
pub mod validate;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testserver;
#[cfg(test)]
mod testutil;

pub use error::{
    Error, GitFailureKind, IndexError, NameError, PackError, PathError, RefError, UrlError,
};
pub use gate::{set_webrtc_confirmed_off, third_party_code_allowed};
pub use git::{Backend, Git, classify_failure};
pub use index::{EntrySource, IndexEntry, MarketplaceIndex, parse_index};
pub use install::{
    Installed, InstalledSummary, UpdateDiff, VerifiedHashes, VerifyError, diff_update, install,
    install_dir, remove_install, verified_hashes, verify_install,
};
pub use pack::{AvailableEngine, InstallPreview, Kind, PackFile, PrepareRequest, PreparedPack};
pub use repo::{MarketplaceRef, ResolvedSource};
pub use validate::{Commit, GitRef, GitUrl, HostClass, RelPath, Scheme, scrub_url};
