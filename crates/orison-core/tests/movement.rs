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
use orison_core::turn::{DirectorState, TurnConfig, TurnEngine, TurnError};
use support::{
    character_response_json, combined_response_json, director_response_json, fixture_session,
    test_tokenizer, FakeOllama,
};

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

/// B-19: a beat composed while the player walks away must not walk them back.
///
/// `compose_beat` runs fire-and-forget after the Actor's turn resolves, and
/// its model call is ten to thirty seconds of real time. `save_campaign`
/// writes the whole campaign row, so a beat that writes back the snapshot it
/// took *before* that call reverts every state change the player made while
/// it was thinking — where they are standing and who they are addressing.
///
/// Found by playing: a session that moved to Thornwick Archive, changed
/// speaker and took a turn reopened at Stonebridge, addressing the character
/// the move had left behind.
#[tokio::test]
async fn a_beat_composed_in_the_background_does_not_revert_a_move() {
    // Slow enough that the move below lands inside the Director's call, which
    // is the race. `by_schema` answers the Actor and the Director each with
    // the shape its request asked for.
    let server = FakeOllama::by_schema(
        character_response_json("He nods.", "Aye, five coppers."),
        director_response_json("The mill wheel slows."),
        combined_response_json("He nods.", "Aye."),
        12,
        Duration::from_millis(60),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect"),
    );
    let (session, _store, _script) = fixture_session("minimal");

    let mut config = TurnConfig::default();
    // A beat on the very first turn, so the window under test opens at once.
    config.director.enabled = true;
    config.director.turn_threshold = 1;
    config.director.cooldown_turns = 0;
    let engine = TurnEngine::new(session, Arc::clone(&backend), backend, config);

    engine.move_to_location("Stonebridge").expect("travel");
    engine.select_character("bram_holt").expect("select");

    let outcome = engine.player_input("What's the toll?").await.expect("turn");
    assert!(
        outcome.director_triggered,
        "the race needs a beat in flight; none was composed"
    );

    // Wait for the beat to reach its model call. `Composing` is set after
    // `compose_beat` has read the campaign row and before it awaits the
    // model, which is precisely the window a stale write-back would lose.
    // Moving before the job starts proves nothing: it would read the row
    // after the move and write the right thing by luck.
    for _ in 0..400 {
        if engine.director_state() == DirectorState::Composing {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        engine.director_state(),
        DirectorState::Composing,
        "the beat never reached its model call, so the race was never opened"
    );

    // The player walks on while the Director is still thinking. This is the
    // ordinary case, not a contrived one: the beat takes longer than reading
    // the reply does.
    engine
        .move_to_location("Thornwick Archive")
        .expect("travel while the beat composes");
    engine.select_character("elara_voss").expect("select");

    // Let the beat land.
    for _ in 0..200 {
        if engine.director_state() == DirectorState::Ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        engine.director_state(),
        DirectorState::Ready,
        "the beat never finished, so the race was never run"
    );

    assert_eq!(
        engine.current_location().unwrap().map(|e| e.label),
        Some("Thornwick Archive".to_string()),
        "the beat wrote back a stale campaign row and moved the player back"
    );
    let speaking: Option<String> = engine
        .session()
        .with_store(|store| {
            Ok(store
                .load_campaign(engine.session().campaign_id())?
                .map(|c| c.active_character))
        })
        .expect("read the campaign row");
    assert_eq!(
        speaking.as_deref(),
        Some("elara_voss"),
        "the beat wrote back a stale campaign row and changed who is speaking"
    );
}
