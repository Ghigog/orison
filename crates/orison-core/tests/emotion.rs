//! The emotion engine (§4.3).
//!
//! Two fixes the handoff names explicitly, "both of which were real bugs":
//! the RAG006 no-op delta skip and the RAG003 reaction debounce. Both are
//! asserted here rather than described in a comment, because both were
//! previously fixed and both regressed once already.

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::emotion::{EmotionEngine, EmotionState, Rapport};
use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::prompt::schemas::{BaselineDisposition, Emotion, EmotionalUpdate};
use orison_core::state::{CampaignStore, CharacterState};
use orison_core::turn::{TurnConfig, TurnEngine, TurnEvent};
use support::{character_response_json, test_session, test_tokenizer, FakeOllama};

fn update(emotion: Emotion, intensity: f32, rapport_delta: f32) -> EmotionalUpdate {
    EmotionalUpdate {
        emotion,
        intensity,
        reason: "Because of what the player said.".to_string(),
        rapport_delta,
    }
}

fn store_with_character() -> (CampaignStore, String) {
    let store = CampaignStore::open_in_memory().unwrap();
    let campaign = orison_core::state::Campaign::new("c", "Test", "2026-01-01T00:00:00Z");
    store.save_campaign(&campaign).unwrap();
    store
        .save_character_state("c", &CharacterState::new("quillion"))
        .unwrap();
    (store, "c".to_string())
}

/// RAG006. The event and the rapport change are always recorded — losing a
/// turn's affinity because a tag repeated would be a different bug — but a
/// state that did not move must not report itself as changed.
#[test]
fn a_repeated_emotion_records_history_but_reports_no_change() {
    let (store, campaign) = store_with_character();
    let engine = EmotionEngine::default();

    let first = engine
        .apply(
            &store,
            &campaign,
            "quillion",
            &update(Emotion::Trust, 0.6, 0.1),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
    assert!(first.changed, "the first update is always a change");

    let second = engine
        .apply(
            &store,
            &campaign,
            "quillion",
            &update(Emotion::Trust, 0.62, 0.1),
            "2026-01-01T00:01:00Z",
        )
        .unwrap();
    assert!(
        !second.changed,
        "an identical emotion at the same intensity is a no-op delta"
    );

    // Recorded regardless.
    assert_eq!(
        store
            .emotion_events(&campaign, "quillion", 10)
            .unwrap()
            .len(),
        2
    );
    assert!(
        (second.affinity - 0.2).abs() < 1e-6,
        "rapport must still move: {}",
        second.affinity
    );
}

/// RAG003, at the level the debounce actually has to hold: one player turn,
/// one reaction request.
#[tokio::test]
async fn one_turn_requests_at_most_one_reaction() {
    let server = FakeOllama::start(
        character_response_json("Quillion looks up sharply.", "You should not be here."),
        6,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .unwrap(),
    );
    let (session, _store) = test_session();
    let mut config = TurnConfig::default();
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), Arc::clone(&backend), config);
    let mut events = engine.subscribe();

    engine.player_input("I open the ledger.").await.unwrap();

    let mut reactions = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event, TurnEvent::ReactionRequested { .. }) {
            reactions += 1;
        }
    }
    assert_eq!(
        reactions, 1,
        "the Godot build queued three reaction generations per turn (RAG003)"
    );
}

/// The same turn twice: the second must request no reaction at all, because
/// the stand-in returns the same emotional update and nothing moved.
#[tokio::test]
async fn an_unchanged_emotion_requests_no_reaction() {
    let server = FakeOllama::start(
        character_response_json("He does not look up.", "Mm."),
        6,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .unwrap(),
    );
    let (session, _store) = test_session();
    let mut config = TurnConfig::default();
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), Arc::clone(&backend), config);

    engine.player_input("Hello.").await.unwrap();
    let mut events = engine.subscribe();
    engine.player_input("Hello again.").await.unwrap();

    let mut reactions = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event, TurnEvent::ReactionRequested { .. }) {
            reactions += 1;
        }
    }
    assert_eq!(
        reactions, 0,
        "a turn that did not move the emotion must not queue a reaction"
    );
}

/// The Godot decay always ran to serenity, which made `base_emotion` inert
/// after one turn — a whole model call producing a field nothing read.
#[test]
fn decay_returns_a_character_to_their_own_baseline() {
    let (store, campaign) = store_with_character();
    let engine = EmotionEngine::default();
    engine
        .set_baseline(
            &store,
            &campaign,
            "quillion",
            &BaselineDisposition {
                base_emotion: Emotion::Sadness,
                base_intensity: 0.3,
                reason: "A grieving man.".to_string(),
            },
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
    engine
        .apply(
            &store,
            &campaign,
            "quillion",
            &update(Emotion::Anger, 0.1, 0.0),
            "2026-01-01T00:01:00Z",
        )
        .unwrap();

    let entities = vec!["quillion".to_string()];
    for i in 0..5 {
        engine
            .decay(
                &store,
                &campaign,
                &entities,
                None,
                "Emotional decay over time.",
                &format!("2026-01-01T00:0{}:00Z", i + 2),
            )
            .unwrap();
    }

    let state = engine.current(&store, &campaign, "quillion").unwrap();
    assert_eq!(
        state.emotion,
        Emotion::Sadness,
        "a character with a recorded baseline should return to it, not to serenity"
    );
    assert!((state.intensity - 0.3).abs() < 1e-6, "{state:?}");
}

/// A character with nothing recorded still fades to neutral, exactly as
/// before. The control case for the change above.
#[test]
fn a_character_without_a_baseline_still_fades_to_neutral() {
    let (store, campaign) = store_with_character();
    let engine = EmotionEngine::default();
    engine
        .apply(
            &store,
            &campaign,
            "quillion",
            &update(Emotion::Anger, 0.08, 0.0),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

    let entities = vec!["quillion".to_string()];
    for i in 0..3 {
        engine
            .decay(
                &store,
                &campaign,
                &entities,
                None,
                "Emotional decay over time.",
                &format!("2026-01-01T00:0{}:00Z", i + 1),
            )
            .unwrap();
    }
    let state = engine.current(&store, &campaign, "quillion").unwrap();
    assert_eq!(state.emotion, Emotion::Serenity);
    assert_eq!(state.intensity, 0.0);
}

/// The character the player is talking to is not faded out from under them.
#[test]
fn the_addressed_character_is_not_decayed() {
    let (store, campaign) = store_with_character();
    let engine = EmotionEngine::default();
    engine
        .apply(
            &store,
            &campaign,
            "quillion",
            &update(Emotion::Joy, 0.9, 0.0),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

    engine
        .decay(
            &store,
            &campaign,
            &["quillion".to_string()],
            Some("quillion"),
            "Emotional decay over time.",
            "2026-01-01T00:01:00Z",
        )
        .unwrap();

    let state = engine.current(&store, &campaign, "quillion").unwrap();
    assert_eq!(state.intensity, 0.9);
}

/// Rapport is clamped at both ends however many turns push at it.
#[test]
fn rapport_saturates_rather_than_running_away() {
    let (store, campaign) = store_with_character();
    let engine = EmotionEngine::default();
    for i in 0..20 {
        engine
            .apply(
                &store,
                &campaign,
                "quillion",
                &update(Emotion::Trust, 0.5 + (i as f32) * 0.01, 0.2),
                "2026-01-01T00:00:00Z",
            )
            .unwrap();
    }
    let affinity = store
        .character_state(&campaign, "quillion")
        .unwrap()
        .unwrap()
        .affinity;
    assert_eq!(affinity, 1.0);
    assert_eq!(Rapport::of(affinity), Rapport::BestFriend);
}

/// The prompt block states the band and the tone, and never the raw numbers
/// the model is told not to reveal.
#[test]
fn the_profile_block_carries_the_band_and_forbids_leaking_it() {
    let engine = EmotionEngine::default();
    let block = engine.profile_block("Quillion", &EmotionState::new(Emotion::Anger, 0.8), -0.7);
    assert!(block.contains("Nemesis"));
    assert!(block.contains("anger"));
    assert!(block.contains("Clipped, blunt"));
    assert!(block.contains("Never state your raw affinity score"));
}
