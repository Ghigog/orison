//! Moving between locations, and who is there when you arrive (§5.4).
//!
//! Against `minimal`, which has exactly the shape this needs: two locations
//! joined by the mill road, a character at each end, and a creature that
//! cannot speak.

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::InferenceBackend;
use orison_core::inference::OllamaBackend;
use orison_core::turn::{TurnConfig, TurnEngine, TurnError};
use support::{character_response_json, fixture_session, test_tokenizer, FakeOllama};

async fn engine() -> Arc<TurnEngine> {
    let server = FakeOllama::start(
        character_response_json("He nods.", "Aye."),
        2,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect"),
    );
    let (session, _store, _script) = fixture_session("minimal");
    // Leak the store handle deliberately: the engine holds its own `Arc` and
    // the test has no use for a second one.
    let mut config = TurnConfig::default();
    config.director.enabled = false;
    TurnEngine::new(session, Arc::clone(&backend), backend, config)
}

#[tokio::test]
async fn the_vaults_own_connections_are_the_exits() {
    let engine = engine().await;

    // Nowhere active yet, so everywhere is a starting point.
    assert!(engine.current_location().unwrap().is_none());
    let names: Vec<String> = engine
        .exits()
        .unwrap()
        .into_iter()
        .map(|e| e.label)
        .collect();
    assert_eq!(names, vec!["Stonebridge", "Thornwick Archive"]);

    let arrived = engine.move_to_location("Stonebridge").expect("travel");
    assert_eq!(arrived.label, "Stonebridge");
    assert_eq!(
        engine.current_location().unwrap().map(|e| e.label),
        Some("Stonebridge".to_string())
    );

    // From Stonebridge the mill road is the only way out, and it goes to one
    // place. This is the vault's own `Connected to [[Thornwick Archive]]`,
    // not a movement graph invented by the shell.
    let names: Vec<String> = engine
        .exits()
        .unwrap()
        .into_iter()
        .map(|e| e.label)
        .collect();
    assert_eq!(names, vec!["Thornwick Archive"]);
}

#[tokio::test]
async fn travelling_somewhere_unreachable_is_a_typed_refusal_not_a_silent_move() {
    let engine = engine().await;
    engine.move_to_location("Stonebridge").expect("travel");

    // A place that does not exist.
    let err = engine.move_to_location("Ashmere").unwrap_err();
    assert!(
        matches!(&err, TurnError::CannotTravel { detail, .. } if detail.contains("no place called")),
        "{err}"
    );

    // Something real, but not somewhere you can stand.
    let err = engine.move_to_location("Elara Voss").unwrap_err();
    assert!(
        matches!(&err, TurnError::CannotTravel { detail, .. } if detail.contains("not a place")),
        "{err}"
    );

    // Already here.
    let err = engine.move_to_location("Stonebridge").unwrap_err();
    assert!(
        matches!(&err, TurnError::CannotTravel { detail, .. } if detail.contains("already at")),
        "{err}"
    );

    // The move that failed did not happen.
    assert_eq!(
        engine.current_location().unwrap().map(|e| e.label),
        Some("Stonebridge".to_string())
    );
}

/// The move has to reach the transcript, or the character the player walks up
/// to is answering a conversation they were never told restarted somewhere
/// else.
#[tokio::test]
async fn a_move_is_written_to_the_transcript() {
    let engine = engine().await;
    engine
        .move_to_location("Thornwick Archive")
        .expect("travel");

    let campaign_id = engine.session().campaign_id().to_string();
    let history = engine
        .session()
        .with_store(|store| store.recent_history(&campaign_id, 10))
        .unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.content.contains("travels to Thornwick Archive")),
        "the move is missing from the transcript: {history:#?}"
    );
}

/// Presence, and the two behaviours carried over from
/// `NearbyCharacterList.get_nearby_character_ids`.
#[tokio::test]
async fn who_is_present_follows_the_graph_and_excludes_what_cannot_speak() {
    let engine = engine().await;

    // With no location active, everyone who can speak is available — a vault
    // of nothing but characters must still be playable.
    let all: Vec<String> = engine
        .characters_present()
        .unwrap()
        .into_iter()
        .map(|e| e.label)
        .collect();
    assert!(all.contains(&"Bram Holt".to_string()));
    assert!(all.contains(&"Elara Voss".to_string()));
    assert!(
        !all.contains(&"The Kettle".to_string()),
        "The Kettle has can_speak: false and is not someone to talk to: {all:?}"
    );

    engine.move_to_location("Stonebridge").expect("travel");
    let here: Vec<String> = engine
        .characters_present()
        .unwrap()
        .into_iter()
        .map(|e| e.label)
        .collect();
    assert!(
        here.contains(&"Bram Holt".to_string()),
        "Bram holds the toll post at Stonebridge: {here:?}"
    );
    assert!(!here.contains(&"The Kettle".to_string()));
}
