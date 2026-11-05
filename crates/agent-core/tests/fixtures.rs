//! Shared-fixture conformance (ADR-006): the same files the TypeScript validator checks.

use agent_core::parse_event;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_files(kind: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/agent-event")
        .join(kind);
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no {kind} fixtures in {}", dir.display());
    files
}

#[test]
fn valid_agent_event_fixtures_parse() {
    for path in fixture_files("valid") {
        let json = fs::read_to_string(&path).expect("read fixture");
        if let Err(e) = parse_event(&json) {
            panic!("{} should be valid: {e}", path.display());
        }
    }
}

#[test]
fn invalid_agent_event_fixtures_are_rejected() {
    for path in fixture_files("invalid") {
        let json = fs::read_to_string(&path).expect("read fixture");
        assert!(
            parse_event(&json).is_err(),
            "{} should be rejected",
            path.display()
        );
    }
}
