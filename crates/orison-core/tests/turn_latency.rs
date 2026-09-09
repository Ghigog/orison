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
//! **How the cache evidence works, since Phase 5.0.** Ollama reports
//! `prompt_eval_count`: the prompt tokens it actually evaluated. Tokens
//! served from its own prompt cache are not counted. So the table prints the
//! prompt we sent beside the prompt the server evaluated, and the gap between
//! them *is* the reuse — no inference required.
//!
//! Phase 4 had only time-to-first-token to read, which is why it could
//! conclude nothing firmer than "TTFT rose, so probably no reuse". A warm
//! model and a cold one differ by more than TTFT does; two token counts do
//! not. `evaluated / sent` on turn one will be ~1.0 (nothing is cached yet)
//! and should fall sharply from turn two onward. If it stays near 1.0 across
//! a whole transcript, the prefix is not being reused and §2.7 is not
//! reaching this backend — which is exactly the state Phase 4 measured and
//! Phase 5.0 fixed in two places (`prompt::ordering`, and the context window
//! `OllamaBackend` asks for).

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::turn::{TurnConfig, TurnEngine, TurnOutcome};
use support::{character_response_json, fixture_session, test_tokenizer, FakeOllama};

/// The recorded Godot p50, in milliseconds, per fixture.
const GODOT_BASELINE_P50_MS: &[(&str, f64)] = &[("minimal", 21_200.0), ("messy", 20_200.0)];

/// The fraction of this turn's prompt the backend did not have to evaluate.
///
/// `None` when the backend reported no accounting at all. A backend that
/// evaluated more than it was sent (possible: its tokenizer is not ours)
/// clamps to zero rather than going negative.
fn reuse_fraction(outcome: &TurnOutcome) -> Option<f64> {
    let evaluated = outcome.evaluated_prompt_tokens?;
    if outcome.prompt_tokens == 0 {
        return None;
    }
    let sent = outcome.prompt_tokens as f64;
    Some((1.0 - evaluated as f64 / sent).clamp(0.0, 1.0))
}

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
        "{:>4} | {:>9} | {:>9} | {:>7} | {:>9} | {:>9}",
        "turn", "sent", "evaluated", "reused", "ttft", "total"
    );
    println!(
        "{:-<5}|{:-<11}|{:-<11}|{:-<9}|{:-<11}|{:-<11}",
        "", "", "", "", "", ""
    );
    for (i, outcome) in outcomes.iter().enumerate() {
        println!(
            "{:>4} | {:>9} | {:>9} | {:>7} | {:>9} | {:>9}",
            i,
            outcome.prompt_tokens,
            outcome
                .evaluated_prompt_tokens
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into()),
            reuse_fraction(outcome)
                .map(|f| format!("{:.0}%", f * 100.0))
                .unwrap_or_else(|| "-".into()),
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

    // The cache evidence, measured rather than inferred: after the first
    // turn there is a prefix to reuse, so the fraction of the prompt the
    // server did not have to evaluate should be substantial and should grow
    // as the transcript does.
    let after_first: Vec<f64> = outcomes.iter().skip(1).filter_map(reuse_fraction).collect();
    if after_first.is_empty() {
        println!(
            "\nNo backend prompt accounting reported, so cache reuse is unmeasured here. \
             Fall back on time-to-first-token against a growing prompt, and treat it as weak."
        );
    } else {
        let mean = after_first.iter().sum::<f64>() / after_first.len() as f64;
        println!(
            "\nprefix reuse after turn 0: mean {:.0}% of the prompt served from cache",
            mean * 100.0
        );
        println!(
            "Near 0% means the prefix is not being reused and §2.7 is not reaching this \
             backend. That was Phase 4's state."
        );
    }

    if let (Some(first), Some(last)) = (outcomes.first(), outcomes.last()) {
        if let (Some(a), Some(b)) = (first.time_to_first_token, last.time_to_first_token) {
            println!(
                "prompt grew {} -> {} tokens; time to first token {:.0} -> {:.0} ms",
                first.prompt_tokens,
                last.prompt_tokens,
                a.as_secs_f64() * 1000.0,
                b.as_secs_f64() * 1000.0,
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
    // The cache evidence is only readable if the backend's own accounting
    // reaches the outcome. The stand-in reports it, so a break in the wiring
    // between `prompt_eval_count` and `TurnOutcome` fails here rather than
    // silently printing "-" in the middle of a measurement session.
    assert!(
        outcomes.iter().all(|o| o.evaluated_prompt_tokens.is_some()),
        "the backend's prompt accounting did not reach TurnOutcome, so reuse is unmeasurable"
    );
    // The property the cache evidence depends on: the prompt really does grow
    // as the transcript does, so a flat time-to-first-token would mean
    // something.
    assert!(
        outcomes.last().unwrap().prompt_tokens > outcomes.first().unwrap().prompt_tokens,
        "the prompt did not grow across turns, so the cache evidence would be vacuous"
    );
    // The assertion the Phase 4 harness was missing. The latencies from a
    // stand-in are meaningless, but *cache reuse is not a timing
    // measurement* — it is a property of the message list, and the stand-in
    // models it exactly (see `support::evaluated_prompt_tokens`). So the
    // thing §2.7 exists to deliver can be asserted in `cargo test`, with no
    // model, on a transcript where every line differs.
    let reuse: Vec<f64> = outcomes.iter().skip(1).filter_map(reuse_fraction).collect();
    assert_eq!(
        reuse.len(),
        outcomes.len() - 1,
        "every turn after the first must report a reuse fraction"
    );

    // The load-bearing property, and the one that tells the two orderings
    // apart: as the transcript grows, the share of the prompt already in the
    // cache must grow with it. It does when the transcript is inside the
    // stable prefix (72% -> 77% here). Move retrieved lore back ahead of the
    // transcript, as it was before Phase 5.0, and the same run *declines*
    // (72% -> 65%), because every turn re-processes a history that keeps
    // getting longer. An absolute floor could not see that difference on a
    // fixture this short — the character card dominates either way — which
    // is why the assertion is on the direction.
    let first = reuse.first().copied().expect("at least one");
    let last = reuse.last().copied().expect("at least one");
    assert!(
        last >= first,
        "cache reuse fell from {:.0}% to {:.0}% as the transcript grew, so something \
         volatile is ordered ahead of it (§2.7)",
        first * 100.0,
        last * 100.0
    );
    let mean = reuse.iter().sum::<f64>() / reuse.len() as f64;
    assert!(
        mean > 0.5,
        "only {:.0}% of the prompt was reusable across turns",
        mean * 100.0
    );

    report("minimal (stand-in, meaningless numbers)", &outcomes);
}
