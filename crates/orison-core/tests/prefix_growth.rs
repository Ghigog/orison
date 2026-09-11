//! The prompt prefix over a whole transcript, not just two turns (§2.7).
//!
//! `turn_loop.rs` asserts a byte-identical prefix across **two** consecutive
//! turns. That is how B-17 survived: two turns is long enough to show that
//! nothing volatile sits at the very front, and too short to show whether the
//! growing transcript stays inside the cached region. The defect that cost
//! Phase 4 its latency was visible only from the third turn on.
//!
//! So this runs eight turns against each fixture and asserts the shared
//! prefix *grows*. A prefix that stays the same size while the prompt grows
//! is the signature of something volatile ordered ahead of the transcript,
//! and it is what the live numbers would look like if §2.7 had regressed.
//!
//! It also counts model calls. One turn must make exactly one Actor call:
//! anything else — a background job, a stray deduction — lands in the same
//! Ollama slot with a different prefix and evicts the cache the ordering
//! exists to fill, which no prompt-level assertion would ever see.

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::turn::{TurnConfig, TurnEngine};
use support::{character_response_json, fixture_session, test_tokenizer, FakeOllama};

/// `role:content` per message, which is what a shared prefix is made of.
fn messages_of(body: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    v["messages"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|m| {
                    format!(
                        "{}:{}",
                        m["role"].as_str().unwrap_or(""),
                        m["content"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn words(messages: &[String]) -> usize {
    messages.iter().map(|m| m.split_whitespace().count()).sum()
}

async fn prefix_report(fixture: &str) -> Vec<(usize, usize)> {
    let server = FakeOllama::start(
        character_response_json("She sets down the ledger.", "Nineteen years, near enough."),
        4,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect to the stand-in"),
    );
    let (session, _store, script) = fixture_session(fixture);
    let mut config = TurnConfig::default();
    // The Actor turn alone. A Director beat is a second call with a different
    // prefix by design, and its cost belongs to the §4.2 comparison.
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), backend, config);

    let turns = 8;
    for line in script.lines.iter().cycle().take(turns) {
        engine.player_input(line.clone()).await.expect("turn");
    }

    let bodies = server.observed.chat_bodies();
    assert_eq!(
        bodies.len(),
        turns,
        "{fixture}: {turns} turns made {} model calls. Anything beyond one call per turn \
         shares the backend's cache slot and evicts the prefix §2.7 exists to fill.",
        bodies.len()
    );

    println!("\n=== {fixture} ===");
    let mut shared_sizes = Vec::new();
    let mut previous: Option<Vec<String>> = None;
    for (turn, body) in bodies.iter().enumerate() {
        let current = messages_of(body);
        let total = words(&current);
        if let Some(previous) = &previous {
            let shared = previous
                .iter()
                .zip(&current)
                .take_while(|(a, b)| a == b)
                .count();
            let shared_words = words(&current[..shared]);
            println!(
                "turn {turn}: {:>2} messages, {total:>5} words | shared with previous: \
                 {shared:>2} messages, {shared_words:>5} words ({:>3.0}%)",
                current.len(),
                shared_words as f64 / total as f64 * 100.0
            );
            shared_sizes.push((shared_words, total));
        } else {
            println!(
                "turn {turn}: {:>2} messages, {total:>5} words | first request",
                current.len()
            );
        }
        previous = Some(current);
    }
    shared_sizes
}

/// The shared prefix must grow as the transcript does, on every fixture.
///
/// Not "must be large": the character card dominates a short transcript
/// either way, so a floor cannot tell a cache-stable ordering from a
/// cache-hostile one. Growth can. Put retrieved lore back ahead of the
/// transcript, as it was before Phase 5.0, and this stops growing because
/// every turn's prefix ends at a lore block that changed.
#[tokio::test]
async fn the_shared_prefix_grows_with_the_transcript() {
    for fixture in ["minimal", "messy"] {
        let sizes = prefix_report(fixture).await;
        assert!(sizes.len() >= 4, "{fixture}: too few turns to see a trend");

        let first = sizes.first().expect("at least one").0;
        let last = sizes.last().expect("at least one").0;
        assert!(
            last > first,
            "{fixture}: the shared prefix did not grow ({first} -> {last} words) while the \
             transcript did, so something volatile is ordered ahead of it (§2.7)"
        );

        // Monotone, not merely larger at the end: a prefix that shrinks on any
        // turn means a block ahead of the transcript moved on that turn.
        for pair in sizes.windows(2) {
            assert!(
                pair[1].0 >= pair[0].0,
                "{fixture}: the shared prefix shrank, {} -> {} words. Something ahead of \
                 the transcript changed between turns (§2.7)",
                pair[0].0,
                pair[1].0
            );
        }
    }
}
