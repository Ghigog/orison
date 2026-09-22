//! Adventure-starter generation, against the loopback stand-in — the
//! same discipline `turn_loop.rs` uses: role-separated `/api/chat`, real
//! schema-constrained decoding, nothing hand-mocked at the trait level.

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, KeepAlive, OllamaBackend, SamplingOptions};
use orison_core::knowledge::{Edge, EdgeKind, Entity, EntityId, EntityKind, KnowledgeGraph};
use orison_core::onboarding::{
    build_connected_starting_clusters, generate_starters, StarterGenerationInput, StarterProgress,
};
use orison_core::prompt::PlayerCard;
use orison_core::testing::{test_tokenizer, FakeOllama};
use orison_core::turn::{CancelReason, CancelToken};

fn entity(name: &str, kind: EntityKind) -> Entity {
    let mut e = Entity::new(EntityId::slug(name), name, kind);
    e.body = format!("{name} is a fine and ordinary place or person.");
    e
}

/// Two locations, each with exactly one speaking character attached — the
/// same small vault the unit tests in `onboarding.rs` use, so a Pass 1
/// response naming their exact ids is easy to script by hand.
fn two_cluster_graph() -> KnowledgeGraph {
    let mut g = KnowledgeGraph::new();
    g.insert(entity("Stonebridge", EntityKind::Location));
    g.insert(entity("Bram Holt", EntityKind::Character));
    g.insert(entity("Thornwick Archive", EntityKind::Location));
    g.insert(entity("Elara Voss", EntityKind::Character));
    g.connect(Edge {
        from: EntityId::slug("Bram Holt"),
        to: EntityId::slug("Stonebridge"),
        kind: EdgeKind::AssociatedWith,
        weight: 1.0,
    });
    g.connect(Edge {
        from: EntityId::slug("Elara Voss"),
        to: EntityId::slug("Thornwick Archive"),
        kind: EdgeKind::AssociatedWith,
        weight: 1.0,
    });
    g
}

async fn backend(server: &FakeOllama) -> Arc<dyn InferenceBackend> {
    Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect to the stand-in"),
    )
}

fn selection_json(entries: &[(&str, &str)]) -> String {
    let starters: Vec<serde_json::Value> = entries
        .iter()
        .map(|(location_id, character_id)| {
            serde_json::json!({
                "title": "A Working Title",
                "concept": "Something is amiss.",
                "location_id": location_id,
                "character_id": character_id,
            })
        })
        .collect();
    serde_json::json!({ "starters": starters }).to_string()
}

fn narration_json(narration: &str, location_id: &str, character_id: &str) -> String {
    serde_json::json!({
        "title": "The Working Title",
        "description": "Something is amiss.",
        "location_id": location_id,
        "character_id": character_id,
        "narration": narration,
    })
    .to_string()
}

fn input<'a>(
    graph: &'a KnowledgeGraph,
    director: &'a dyn InferenceBackend,
    actor: &'a dyn InferenceBackend,
) -> StarterGenerationInput<'a> {
    StarterGenerationInput {
        graph,
        campaign_title: "Thornwick",
        writing_style: "",
        player: PlayerCard {
            name: "The Wanderer",
            physical_description: None,
            personality: None,
            backstory: None,
        },
        director,
        actor,
        director_sampling: SamplingOptions::new(0.7, 0.9),
        actor_sampling: SamplingOptions::new(0.8, 0.9),
        keep_alive: KeepAlive::Immediate,
    }
}

#[tokio::test]
async fn a_full_pipeline_run_narrates_every_selected_hook() {
    let graph = two_cluster_graph();
    let clusters = build_connected_starting_clusters(&graph);
    assert_eq!(clusters.len(), 2);
    // Both locations connect to exactly one speaking character and no lore,
    // so the connection-count tie is broken by id: "stonebridge" sorts
    // before "thornwick_archive".
    assert_eq!(clusters[0].location_id, EntityId::slug("Stonebridge"));
    assert_eq!(clusters[1].location_id, EntityId::slug("Thornwick Archive"));

    let server = FakeOllama::scripted(
        vec![
            selection_json(&[
                ("stonebridge", "bram_holt"),
                ("thornwick_archive", "elara_voss"),
            ]),
            narration_json(
                "The bridge's stones are cold underfoot as Bram calls out a warning.",
                "stonebridge",
                "bram_holt",
            ),
            narration_json(
                "Dust motes hang in Thornwick's lamplight as Elara looks up.",
                "thornwick_archive",
                "elara_voss",
            ),
        ],
        1,
        Duration::ZERO,
    )
    .await;
    let backend = backend(&server).await;
    let cancel = CancelToken::new();

    let mut progress = Vec::new();
    let starters = generate_starters(
        input(&graph, backend.as_ref(), backend.as_ref()),
        &cancel,
        |p| progress.push(p),
    )
    .await
    .expect("no cancellation");

    assert_eq!(starters.len(), 3);
    assert_eq!(starters[0].location_id, "stonebridge");
    assert_eq!(starters[0].character_id, "bram_holt");
    assert!(starters[0].narration.contains("Bram calls out"));
    assert_eq!(starters[1].location_id, "thornwick_archive");
    assert_eq!(starters[1].character_id, "elara_voss");
    assert!(starters[1].narration.contains("Elara looks up"));
    // Only 2 clusters exist, so the 3rd slot is the deterministic fallback,
    // padded in rather than left missing.
    assert_eq!(starters[2].location_id, "");

    assert!(matches!(progress[0], StarterProgress::BuildingClusters));
    assert!(matches!(progress[1], StarterProgress::SelectingHooks));
    assert!(progress
        .iter()
        .any(|p| matches!(p, StarterProgress::WritingNarration { index: 0, total: 2 })));
}

#[tokio::test]
async fn a_malformed_selection_response_falls_back_for_every_hook() {
    let graph = two_cluster_graph();
    let server = FakeOllama::scripted(vec!["not json".to_string()], 1, Duration::ZERO).await;
    let backend = backend(&server).await;
    let cancel = CancelToken::new();

    let starters = generate_starters(
        input(&graph, backend.as_ref(), backend.as_ref()),
        &cancel,
        |_| {},
    )
    .await
    .expect("no cancellation");

    assert_eq!(starters.len(), 3);
    // Fallback starters are flavour narrations built without a model.
    assert!(
        starters[0].location_id == "thornwick_archive" || starters[0].location_id == "stonebridge"
    );
    assert_eq!(server.observed.chat_requests(), 1); // Pass 2 never ran.
}

#[tokio::test]
async fn one_hooks_narration_failing_does_not_take_down_the_others() {
    let graph = two_cluster_graph();
    let server = FakeOllama::scripted(
        vec![
            selection_json(&[
                ("stonebridge", "bram_holt"),
                ("thornwick_archive", "elara_voss"),
            ]),
            "not json".to_string(),
            narration_json(
                "Dust motes hang in Thornwick's lamplight as Elara looks up.",
                "thornwick_archive",
                "elara_voss",
            ),
        ],
        1,
        Duration::ZERO,
    )
    .await;
    let backend = backend(&server).await;
    let cancel = CancelToken::new();

    let starters = generate_starters(
        input(&graph, backend.as_ref(), backend.as_ref()),
        &cancel,
        |_| {},
    )
    .await
    .expect("no cancellation");

    // Index 0's narration failed: it gets the deterministic fallback for
    // that slot, not an empty or errored starter, and index 1 still got its
    // real narration.
    assert_eq!(starters[0].location_id, "stonebridge");
    assert!(!starters[0].narration.contains("Bram calls out")); // fallback text, not the (missing) real one
    assert_eq!(starters[1].location_id, "thornwick_archive");
    assert!(starters[1].narration.contains("Elara looks up"));
}

#[tokio::test]
async fn cancelling_before_the_first_call_never_issues_one() {
    let graph = two_cluster_graph();
    let server =
        FakeOllama::scripted(vec![selection_json(&[]).to_string()], 1, Duration::ZERO).await;
    let backend = backend(&server).await;
    let cancel = CancelToken::new();
    cancel.cancel(CancelReason::Requested);

    let result = generate_starters(
        input(&graph, backend.as_ref(), backend.as_ref()),
        &cancel,
        |_| {},
    )
    .await;

    assert!(matches!(result, Err(CancelReason::Requested)));
    assert_eq!(server.observed.chat_requests(), 0);
}
