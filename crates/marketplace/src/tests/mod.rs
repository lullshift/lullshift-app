//! Fixture-driven tests that run the real `git` against repositories built in temp folders.
//!
//! - `malicious`: every attack in the S08-T05 list must be refused.
//! - `execution`: hooks, filters and option injection must never run anything.
//! - `happy`: installs, updates and digest checks that must work.
//! - `limits`: the fetch size cap and Git's own object check (security review M2, L3).

mod execution;
mod happy;
mod limits;
mod malicious;
