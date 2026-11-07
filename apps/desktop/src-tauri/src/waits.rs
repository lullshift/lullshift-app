//! `wait_estimates` (ADR-026 section 3): the typical finished working stretch per tool over the
//! last 14 days, computed on demand from the events already stored. Nothing is recorded, nothing
//! leaves the computer, and it says nothing about the person. The UI contract is
//! `apps/desktop/src/suggest.ts` (`WaitEstimate`); change both together.

use std::sync::Arc;

use agent_core::reducer::DEFAULT_STALE_AFTER;
use local_store::{estimates, stretches_from_events};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::Core;

/// How far back finished stretches are read.
const WINDOW_DAYS: i64 = 14;
/// At most this many tools are returned, whatever the events hold.
const MAX_TOOLS: usize = 32;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WaitEstimateView {
    provider: String,
    median_seconds: i64,
    stretches: usize,
}

/// Tools with enough finished stretches to estimate; tools without are simply absent.
#[tauri::command]
pub async fn wait_estimates(
    core: tauri::State<'_, Arc<Core>>,
) -> Result<Vec<WaitEstimateView>, String> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || {
        let since = OffsetDateTime::now_utc() - Duration::days(WINDOW_DAYS);
        // Read the window with the store locked, then fold after releasing it: event ingestion
        // shares this lock and must not wait for the arithmetic.
        let events = {
            let guard = core.store.lock().map_err(|_| "internal".to_owned())?;
            let store = guard
                .as_ref()
                .ok_or_else(|| "store_unavailable".to_owned())?;
            store
                .wait_events(since)
                .map_err(|_| "store_unavailable".to_owned())?
        };
        // A gap longer than the app's stale threshold is idle time, not work.
        let stretches = stretches_from_events(&events, DEFAULT_STALE_AFTER);
        Ok(estimates(&stretches)
            .into_iter()
            .take(MAX_TOOLS)
            .map(|e| WaitEstimateView {
                provider: e.provider,
                median_seconds: e.median_seconds,
                stretches: e.stretches,
            })
            .collect())
    })
    .await
    .map_err(|_| "internal".to_owned())?
}
