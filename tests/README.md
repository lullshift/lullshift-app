# Tests

- `fixtures/<schema>/valid` and `fixtures/<schema>/invalid`: shared contract fixtures used by both the TypeScript validators (`packages/agent-protocol`) and the Rust crates (ADR-006). Each invalid fixture breaks exactly one rule, named in its file name.
- Unit tests live next to the code they test (`*.test.ts`, Rust `#[cfg(test)]` and `crates/*/tests`).
