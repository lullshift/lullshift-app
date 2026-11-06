//! Fixture-driven tests that run the real `git` against repositories built in temp folders.
//!
//! - `malicious`: every attack in the S08-T05 list must be refused.
//! - `execution`: hooks, filters and option injection must never run anything.
//! - `happy`: installs, updates and digest checks that must work.
//! - `limits`: the fetch size cap and Git's own object check (security review M2, L3).
//! - `images`: pictures in content packs (S12-T11).
//! - `network`: where a fetch may go: private hosts, resolved addresses (decisions 17, 18).
//! - `plain_text`: the shared plain-text corpus (decision 18.7).

mod execution;
mod happy;
mod images;
mod limits;
mod malicious;
mod network;
mod plain_text;
