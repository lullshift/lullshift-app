//! The models.dev model list commands (decision 32): memory-only reads, one load, and a model
//! picked from the list saves as a profile model.

use std::sync::atomic::{AtomicUsize, Ordering};

use ::companion::models_dev::{CatalogCache, CatalogLimits};

use super::*;

const SAMPLE: &str =
    include_str!("../../../../../../crates/companion/tests/fixtures/models_dev_sample.json");

struct Catalog {
    url: String,
    hits: Arc<AtomicUsize>,
}

fn models_dev_mock() -> Catalog {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let hits = Arc::new(AtomicUsize::new(0));
    let h = Arc::clone(&hits);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            if read_request(&mut s).is_none() {
                continue;
            }
            h.fetch_add(1, Ordering::SeqCst);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\netag: \"s1\"\r\nconnection: close\r\n\r\n",
                SAMPLE.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(SAMPLE.as_bytes());
        }
    });
    Catalog {
        url: format!("http://127.0.0.1:{port}/api.json"),
        hits,
    }
}

fn core_on(dir: &std::path::Path, url: &str) -> Core {
    let mut core = core_with(dir, memory(), 2000);
    core.companion.catalog = CatalogCache::with_url(url, CatalogLimits::default());
    core
}

#[test]
fn before_a_load_the_list_is_not_there_and_nothing_is_sent() {
    let dir = tempfile::tempdir().expect("dir");
    let m = models_dev_mock();
    let core = core_on(dir.path(), &m.url);
    assert_eq!(
        model_catalog(&core, CatalogMode::Cached).expect("cached"),
        None
    );
    let e = catalog_models(&core, "anthropic").unwrap_err();
    assert_eq!(kind(&e), "notFound");
    assert!(e.message.contains("isn't loaded"), "{}", e.message);
    assert_eq!(m.hits.load(Ordering::SeqCst), 0, "no request before Load");
}

#[test]
fn the_production_state_reads_memory_only_until_asked() {
    // The real state points at models.dev; the memory-only paths must not touch it.
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    assert_eq!(
        model_catalog(&core, CatalogMode::Cached).expect("cached"),
        None
    );
    assert_eq!(
        kind(&catalog_models(&core, "openai").unwrap_err()),
        "notFound"
    );
}

#[test]
fn load_then_browse_from_memory() {
    let dir = tempfile::tempdir().expect("dir");
    let m = models_dev_mock();
    let core = core_on(dir.path(), &m.url);
    let c = model_catalog(&core, CatalogMode::Load)
        .expect("load")
        .expect("some");
    assert!(!c.from_memory && !c.unchanged);
    assert!(c.fetched_at.ends_with('Z'), "{}", c.fetched_at);
    let ids: Vec<&str> = c.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["anthropic", "local-lab", "openai"]);
    assert_eq!(c.providers[0].model_count, 2);

    // Loading again and reading models stay in memory.
    let again = model_catalog(&core, CatalogMode::Load)
        .expect("load")
        .expect("some");
    assert!(again.from_memory);
    let models = catalog_models(&core, "openai").expect("models");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "gpt-4o-mini");
    assert_eq!(models[0].cost_input, Some(0.15));
    assert!(
        model_catalog(&core, CatalogMode::Cached)
            .expect("cached")
            .is_some()
    );
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);

    let e = catalog_models(&core, "no-such-provider").unwrap_err();
    assert_eq!(kind(&e), "notFound");
    let e = catalog_models(&core, &"x".repeat(500)).unwrap_err();
    assert_eq!(kind(&e), "notFound");

    // Refresh is the only other path that sends.
    model_catalog(&core, CatalogMode::Refresh).expect("refresh");
    assert_eq!(m.hits.load(Ordering::SeqCst), 2);
}

#[test]
fn a_listed_model_id_always_saves_as_a_profile_model() {
    let dir = tempfile::tempdir().expect("dir");
    let m = models_dev_mock();
    let core = core_on(dir.path(), &m.url);
    model_catalog(&core, CatalogMode::Load).expect("load");
    let lab = catalog_models(&core, "local-lab").expect("models");
    let saved = save_profile(
        &core,
        input(
            "openaiCompatible",
            Some("http://127.0.0.1:9/v1"),
            &lab[0].id,
            None,
        ),
    )
    .expect("save");
    assert_eq!(saved.model, "lab/free-model:latest");
}

#[test]
fn modes_parse_from_the_ui_names() {
    for (text, mode) in [
        ("\"cached\"", CatalogMode::Cached),
        ("\"load\"", CatalogMode::Load),
        ("\"refresh\"", CatalogMode::Refresh),
    ] {
        assert_eq!(
            serde_json::from_str::<CatalogMode>(text).expect("mode"),
            mode
        );
    }
    assert!(serde_json::from_str::<CatalogMode>("\"url\"").is_err());
}
