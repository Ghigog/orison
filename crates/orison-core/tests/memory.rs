//! The memory tiers (§4.4).
//!
//! Two things this has to hold, and both are easy to lose in a rewrite:
//! biography and session memory never meet, and compaction is triggered by
//! real token counts rather than a turn count standing in for a size.

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::memory::{MemoryManager, MemoryPolicy};
use orison_core::state::{Campaign, CampaignStore, CharacterState, HistoryEntry, HistoryRole};
use orison_core::turn::{TurnConfig, TurnEngine};
use support::{character_response_json, test_session, test_tokenizer, FakeOllama};

/// Word count, standing in for a real tokenizer in the synchronous tests.
/// Emphatically not `length / 4` (B-4).
fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn campaign_with_transcript(entries: usize, words_each: usize) -> (CampaignStore, String) {
    let store = CampaignStore::open_in_memory().unwrap();
    let campaign = Campaign::new("c", "Test", "2026-01-01T00:00:00Z");
    store.save_campaign(&campaign).unwrap();
    store
        .save_character_state("c", &CharacterState::new("quillion"))
        .unwrap();

    let line = vec!["word"; words_each].join(" ");
    for i in 0..entries {
        store
            .append_history(
                "c",
                &HistoryEntry {
                    role: if i % 2 == 0 {
                        HistoryRole::Player
                    } else {
                        HistoryRole::Character
                    },
                    content: format!("{line} {i}"),
                    timestamp: "2026-01-01T00:00:00Z".to_string(),
                    sender: Some("player".to_string()),
                    active_character: Some("quillion".to_string()),
                },
            )
            .unwrap();
    }
    (store, "c".to_string())
}

/// The change the handoff asks for: the trigger is a real size, not
/// `COMPACTION_THRESHOLD = 30`.
#[test]
fn compaction_is_triggered_by_token_count_not_turn_count() {
    let manager = MemoryManager::default();

    // Forty short turns — well past the Godot turn threshold — that still fit
    // comfortably. Nothing should happen.
    let (store, campaign) = campaign_with_transcript(40, 2);
    let plan = manager
        .plan_compaction(&store, &campaign, "quillion", 4000, words)
        .unwrap();
    assert!(
        plan.is_none(),
        "40 terse turns fit the budget; a turn count would have compacted them"
    );

    // Twenty long ones, which do not fit.
    let (store, campaign) = campaign_with_transcript(20, 300);
    let plan = manager
        .plan_compaction(&store, &campaign, "quillion", 4000, words)
        .unwrap()
        .expect("a transcript over the budget should compact");
    assert_eq!(
        plan.entries.len(),
        20 - MemoryPolicy::default().keep_recent_entries,
        "the most recent turns stay verbatim: that is the short-term tier"
    );
}

/// Compaction shortens the prompt. It must not shorten the record.
#[test]
fn compaction_retires_lines_from_the_prompt_without_losing_the_transcript() {
    let manager = MemoryManager::default();
    let (store, campaign) = campaign_with_transcript(20, 300);
    let plan = manager
        .plan_compaction(&store, &campaign, "quillion", 4000, words)
        .unwrap()
        .unwrap();

    manager
        .apply_compaction(
            &store,
            &campaign,
            &plan,
            "They argued about the ledger and parted on cold terms.",
            "2026-01-01T01:00:00Z",
        )
        .unwrap();

    assert_eq!(
        store.recent_history(&campaign, 1000).unwrap().len(),
        20,
        "the transcript must still hold every line"
    );
    assert_eq!(
        store.recent_live_history(&campaign, 1000).unwrap().len(),
        10,
        "the prompt should only see what is not yet summarised"
    );

    let summaries = store
        .session_summaries(&campaign, "quillion", true)
        .unwrap();
    assert_eq!(summaries.len(), 1);
    // The provenance: a summary can point at the lines it stands in for.
    assert_eq!(summaries[0].covers_from, plan.covers_from);
    assert_eq!(summaries[0].covers_to, plan.covers_to);
}

/// A second compaction must summarise the next stretch, not the same one.
#[test]
fn a_second_compaction_covers_new_ground() {
    let manager = MemoryManager::default();
    let (store, campaign) = campaign_with_transcript(20, 300);

    let first = manager
        .plan_compaction(&store, &campaign, "quillion", 4000, words)
        .unwrap()
        .unwrap();
    manager
        .apply_compaction(&store, &campaign, &first, "First stretch.", "t1")
        .unwrap();

    // More conversation happens.
    for i in 0..20 {
        store
            .append_history(
                &campaign,
                &HistoryEntry {
                    role: HistoryRole::Player,
                    content: vec!["word"; 300].join(" ") + &i.to_string(),
                    timestamp: "t2".to_string(),
                    sender: Some("player".to_string()),
                    active_character: Some("quillion".to_string()),
                },
            )
            .unwrap();
    }

    let second = manager
        .plan_compaction(&store, &campaign, "quillion", 4000, words)
        .unwrap()
        .unwrap();
    assert!(
        second.covers_from > first.covers_to,
        "the second summary must start after the first ends: {} vs {}",
        second.covers_from,
        first.covers_to
    );
}

#[test]
fn distillation_absorbs_exactly_the_summaries_it_was_planned_with() {
    let manager = MemoryManager::default();
    let (store, campaign) = campaign_with_transcript(1, 1);
    for i in 0..MemoryPolicy::default().distill_after_summaries {
        store
            .add_session_summary(
                &campaign,
                &orison_core::state::SessionSummary {
                    id: 0,
                    entity_id: "quillion".to_string(),
                    summary: format!("Summary {i}."),
                    created_at: "t".to_string(),
                    covers_from: 0,
                    covers_to: 0,
                    distilled: false,
                },
            )
            .unwrap();
    }

    let plan = manager
        .plan_distillation(&store, &campaign, "quillion")
        .unwrap()
        .expect("ten summaries should be enough");

    // A summary written while the model call was in flight.
    store
        .add_session_summary(
            &campaign,
            &orison_core::state::SessionSummary {
                id: 0,
                entity_id: "quillion".to_string(),
                summary: "Written during the distillation.".to_string(),
                created_at: "t".to_string(),
                covers_from: 0,
                covers_to: 0,
                distilled: false,
            },
        )
        .unwrap();

    manager
        .apply_distillation(
            &store,
            &campaign,
            &plan,
            "Over many weeks their dealings soured.",
        )
        .unwrap();

    let state = store
        .character_state(&campaign, "quillion")
        .unwrap()
        .unwrap();
    assert_eq!(
        state.long_term_memory,
        "Over many weeks their dealings soured."
    );
    let pending = store
        .session_summaries(&campaign, "quillion", true)
        .unwrap();
    assert_eq!(
        pending.len(),
        1,
        "the summary written mid-flight must survive undistilled"
    );
    assert_eq!(pending[0].summary, "Written during the distillation.");
}

/// An empty distillation would erase everything a character remembers.
#[test]
fn an_empty_distillation_changes_nothing() {
    let manager = MemoryManager::default();
    let (store, campaign) = campaign_with_transcript(1, 1);
    let mut state = CharacterState::new("quillion");
    state.long_term_memory = "They have known each other for years.".to_string();
    store.save_character_state(&campaign, &state).unwrap();

    store
        .add_session_summary(
            &campaign,
            &orison_core::state::SessionSummary {
                id: 0,
                entity_id: "quillion".to_string(),
                summary: "A summary.".to_string(),
                created_at: "t".to_string(),
                covers_from: 0,
                covers_to: 0,
                distilled: false,
            },
        )
        .unwrap();
    let plan = orison_core::memory::DistillationPlan {
        entity_id: "quillion".to_string(),
        summary_ids: vec![1],
        summaries: vec!["A summary.".to_string()],
        existing: state.long_term_memory.clone(),
    };
    manager
        .apply_distillation(&store, &campaign, &plan, "   ")
        .unwrap();

    let after = store
        .character_state(&campaign, "quillion")
        .unwrap()
        .unwrap();
    assert_eq!(
        after.long_term_memory,
        "They have known each other for years."
    );
}

/// The separation §1.5 is emphatic about, asserted at the wire.
#[tokio::test]
async fn biography_and_session_memory_reach_the_model_as_separate_blocks() {
    let server = FakeOllama::start(
        character_response_json("He turns a page.", "Ask plainly."),
        6,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .unwrap(),
    );
    let (session, store) = test_session();
    {
        let guard = store.lock().unwrap();
        let mut state = CharacterState::new("quillion");
        state.long_term_memory =
            "In this adventure the player has twice lied to him about the seal.".to_string();
        guard.save_character_state("test-campaign", &state).unwrap();
    }

    let mut config = TurnConfig::default();
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), Arc::clone(&backend), config);
    engine.player_input("About that seal.").await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&server.observed.last_chat_body()).unwrap();
    let messages = body["messages"].as_array().unwrap();
    let card = messages[0]["content"].as_str().unwrap();
    let all = messages
        .iter()
        .map(|m| m["content"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");

    // The biography is in the card, at the front.
    assert!(card.contains("kept the accord's ledger for nineteen years"));
    // The session memory is somewhere else entirely, and labelled as such.
    assert!(
        !card.contains("twice lied"),
        "session memory must not be in the character card: {card}"
    );
    assert!(all.contains("WHAT Quillion REMEMBERS OF THIS ADVENTURE:"));
    assert!(all.contains("twice lied"));
}
