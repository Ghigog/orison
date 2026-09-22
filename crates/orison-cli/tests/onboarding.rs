//! Adventure-starter generation, wired into a real campaign: generate
//! against a compiled vault, pick one, and prove the pick is what a `play`
//! session would actually see afterwards — active location, active
//! character, `intro_narration`, and the narration as the first transcript
//! line.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use orison_cli::{campaign, onboarding};
use orison_core::inference::{InferenceBackend, KeepAlive, OllamaBackend, SamplingOptions};
use orison_core::state::HistoryRole;
use orison_core::testing::{test_tokenizer, FakeOllama};
use orison_core::turn::CancelToken;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

#[tokio::test]
async fn generating_and_picking_a_starter_persists_it_for_play() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("orison.sqlite3");
    let store = campaign::open_store(&db).expect("open the database");
    let created = campaign::create(
        &store,
        "Thornwick",
        "2026-09-09T00:00:00Z",
        Some(&fixture("minimal")),
        &BTreeMap::new(),
    )
    .expect("create and import");

    let session = campaign::open_session(&store, &created.campaign.id).expect("open session");
    assert!(!campaign::is_empty(&session));

    // One scripted Pass 1 response naming real ids from the `minimal`
    // fixture, then a Pass 2 narration for the first hook. The rest fall
    // back deterministically, which is fine: the point of this test is
    // persistence, not prompt content.
    let server = FakeOllama::scripted(
        vec![
            serde_json::json!({
                "starters": [{
                    "title": "The Toll Ledger",
                    "concept": "A discrepancy in the toll ledger points somewhere unwelcome.",
                    "location_id": "stonebridge",
                    "character_id": "bram_holt",
                }]
            })
            .to_string(),
            serde_json::json!({
                "title": "The Toll Ledger",
                "description": "A discrepancy in the toll ledger points somewhere unwelcome.",
                "location_id": "stonebridge",
                "character_id": "bram_holt",
                "narration": "The bridge's lanterns gutter as Bram Holt frowns over his ledger.",
            })
            .to_string(),
        ],
        1,
        Duration::ZERO,
    )
    .await;
    let backend: std::sync::Arc<dyn InferenceBackend> = std::sync::Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect to the stand-in"),
    );
    let cancel = CancelToken::new();

    let mut campaign = campaign::load(&store, &created.campaign.id).expect("load campaign");
    let starters = onboarding::generate(
        session.graph(),
        &campaign,
        backend.as_ref(),
        backend.as_ref(),
        SamplingOptions::new(0.7, 0.9),
        SamplingOptions::new(0.8, 0.9),
        KeepAlive::Immediate,
        &cancel,
        |_| {},
    )
    .await
    .expect("no cancellation");
    assert_eq!(starters.len(), 3);

    let picked = starters[0].clone();
    assert_eq!(picked.location_id, "stonebridge");
    assert_eq!(picked.character_id, "bram_holt");

    onboarding::pick(&store, &mut campaign, &picked, "2026-09-09T00:05:00Z").expect("pick");

    // Reload from scratch: this is what `orison play` would read next.
    let reloaded = campaign::load(&store, &created.campaign.id).expect("reload campaign");
    assert_eq!(reloaded.active_location, "stonebridge");
    assert_eq!(reloaded.active_character, "bram_holt");
    assert_eq!(reloaded.intro_narration, picked.narration);

    let history = store
        .lock()
        .unwrap()
        .recent_history(&created.campaign.id, 10)
        .expect("recent history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].role, HistoryRole::Narrator);
    assert_eq!(history[0].content, picked.narration);
}
