//! Turn latency, which is Phase 2's entire justification (§4.2).
//!
//! `docs/eval_baseline.md` calls ~20 s p50 per conversational turn "the worst
//! number in this document" and "the number Phase 2 must beat". Cache-stable
//! ordering (§2.7) is what is supposed to have beaten it, and until Phase 4
//! there was no loop that generated real turns to measure. The *precondition*
//! is unit-tested — `prompt::ordering` asserts byte-identical prefixes, and
//! `tests/turn_loop.rs` asserts it again at the wire — but the before and
//! after has never been run.
//!
//! ```bash
//! ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
//! ORISON_TEST_OLLAMA_MODEL=<the model the baseline used> \
//!   cargo test -p orison-core --test turn_latency -- --nocapture
//! ```
//!
//! **Use the model the baseline was recorded on** (`llama3.2:3b`), or the
//! comparison measures a change of model rather than a change of engine.
//!
//! The "before" is not re-run here: it is the recorded Godot figure, 21.2 s on
//! `minimal` and 20.2 s on `messy`, over these same transcripts.
//!
//! **What the cache evidence can and cannot show.** Each turn's prompt is
//! longer than the last, because the previous turn joined the history. If
//! time-to-first-token stays flat while prompt tokens climb, the backend is
//! not re-processing the prefix. If it climbs with them, it is — and the
//! ordering work is not reaching this backend. Neither is proof on its own
//! (a warm model and a cold one differ by more than this), which is why the
//! per-turn table is printed rather than a single verdict.

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::turn::{TurnConfig, TurnEngine, TurnOutcome};
use support::{character_response_json, fixture_session, test_tokenizer, FakeOllama};

/// The recorded Godot p50, in milliseconds, per fixture.
const GODOT_BASELINE_P50_MS: &[(&str, f64)] = &[("minimal", 21_200.0), ("messy", 20_200.0)];

fn percentile(sorted: &[f64], q: f64) -> f64 {
    let index = ((sorted.len() as f64) * q) as usize;
    sorted[index.min(sorted.len() - 1)]
}

async fn run_transcript(
    engine: &Arc<TurnEngine>,
    lines: &[String],
    passes: usize,
) -> Vec<TurnOutcome> {
    let mut outcomes = Vec::new();
    for _ in 0..passes {
        for line in lines {
            match engine.player_input(line.clone()).await {
                Ok(outcome) => outcomes.push(outcome),
                Err(e) => panic!("turn failed: {e}"),
            }
        }
    }
    outcomes
}

fn report(fixture: &str, outcomes: &[TurnOutcome]) {
    println!("\n=== {fixture} ===");
    println!(
        "{:>4} | {:>13} | {:>9} | {:>9}",
        "turn", "prompt tokens", "ttft", "total"
    );
    println!("{:-<5}|{:-<15}|{:-<11}|{:-<11}", "", "", "", "");
    for (i, outcome) in outcomes.iter().enumerate() {
        println!(
            "{:>4} | {:>13} | {:>9} | {:>9}",
            i,
            outcome.prompt_tokens,
            outcome
                .time_to_first_token
                .map(|d| format!("{:.0} ms", d.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "-".into()),
            format!("{:.0} ms", outcome.latency.as_secs_f64() * 1000.0),
        );
    }

    let mut latencies: Vec<f64> = outcomes
        .iter()
        .map(|o| o.latency.as_secs_f64() * 1000.0)
        .collect();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = percentile(&latencies, 0.50);
    let p95 = percentile(&latencies, 0.95);
    println!("\np50 {p50:.0} ms   p95 {p95:.0} ms");

    if let Some((_, baseline)) = GODOT_BASELINE_P50_MS.iter().find(|(f, _)| *f == fixture) {
        println!(
            "Godot baseline p50 {baseline:.0} ms  ->  {:.2}x",
            baseline / p50.max(1.0)
        );
        println!(
            "(Phase 5's migration gate is p95, not p50, and it is against the same baseline.)"
        );
    }

    // The cache evidence: prompt length climbs, time to first token should
    // not climb with it.
    let first = outcomes.first();
    let last = outcomes.last();
    if let (Some(first), Some(last)) = (first, last) {
        if let (Some(a), Some(b)) = (first.time_to_first_token, last.time_to_first_token) {
            println!(
                "prompt grew {} -> {} tokens; time to first token {:.0} -> {:.0} ms",
                first.prompt_tokens,
                last.prompt_tokens,
                a.as_secs_f64() * 1000.0,
                b.as_secs_f64() * 1000.0,
            );
            println!(
                "Flat time-to-first-token against a growing prompt is the prefix being reused. \
                 Rising with it means it is not."
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn measure_turn_latency() {
    let (Ok(url), Ok(model)) = (
        std::env::var("ORISON_TEST_OLLAMA_URL"),
        std::env::var("ORISON_TEST_OLLAMA_MODEL"),
    ) else {
        eprintln!(
            "SKIPPED measure_turn_latency: set ORISON_TEST_OLLAMA_URL and \
             ORISON_TEST_OLLAMA_MODEL (use the model the baseline was recorded on, \
             llama3.2:3b, or the comparison measures a change of model)."
        );
        return;
    };

    println!("\nTurn latency, against the Godot baseline in docs/eval_baseline.md");
    println!("model: {model}");

    for fixture in ["minimal", "messy"] {
        let (session, _store, script) = fixture_session(fixture);
        let backend: Arc<dyn InferenceBackend> = Arc::new(
            OllamaBackend::connect(&url, &model, test_tokenizer())
                .await
                .expect("connect to the configured Ollama"),
        );
        let mut config = TurnConfig::default();
        // The Actor turn alone, which is what the Godot figure measured. The
        // Director's cost is separate and belongs to the §4.2 comparison.
        config.director.enabled = false;
        let engine = TurnEngine::new(session, Arc::clone(&backend), Arc::clone(&backend), config);

        // Two passes over the transcript, so the prompt grows enough for the
        // cache evidence to mean something.
        let outcomes = run_transcript(&engine, &script.lines, 2).await;
        report(fixture, &outcomes);
    }
}

/// The harness itself, against the loopback stand-in.
///
/// The numbers it produces are meaningless — the stand-in answers in
/// milliseconds — but a harness that falls over is discovered here rather
/// than in the middle of a measurement session.
#[tokio::test(flavor = "multi_thread")]
async fn the_harness_works_without_a_model() {
    let server = FakeOllama::start(
        character_response_json("He turns a page.", "Ask plainly."),
        8,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .unwrap(),
    );
    let (session, _store, script) = fixture_session("minimal");
    let mut config = TurnConfig::default();
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), Arc::clone(&backend), config);

    let outcomes = run_transcript(&engine, &script.lines, 2).await;
    assert_eq!(outcomes.len(), script.lines.len() * 2);
    assert!(outcomes.iter().all(|o| o.prompt_tokens > 0));
    assert!(
        outcomes.iter().all(|o| o.time_to_first_token.is_some()),
        "time to first token must be measured on every turn"
    );
    // The property the cache evidence depends on: the prompt really does grow
    // as the transcript does, so a flat time-to-first-token would mean
    // something.
    assert!(
        outcomes.last().unwrap().prompt_tokens > outcomes.first().unwrap().prompt_tokens,
        "the prompt did not grow across turns, so the cache evidence would be vacuous"
    );
    report("minimal (stand-in, meaningless numbers)", &outcomes);
}
