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
//! **How the cache evidence works, since Phase 5.6.** Ollama reports
//! `prompt_eval_duration`: the time it spent evaluating the prompt. Tokens
//! served from its own prompt cache cost nothing, so this number stays flat
//! as a transcript grows when the prefix is reused, and rises with the prompt
//! when it is not. The table prints it beside the prompt we sent, and the
//! per-token cost derived from the two is the reuse evidence.
//!
//! **It used to read `prompt_eval_count`, and that was wrong.** That field is
//! documented as excluding cached tokens. It does not: `tests/prefix_cache.rs`
//! sent three requests sharing a byte-identical 2232-token prefix to
//! `llama3.2:3b` and got 2232, 2233 and 2232 back while the work behind them
//! fell from 9858 ms to 179 ms. Phase 5.6's first live run read that flat
//! count as "0% reuse" on every turn of both fixtures and very nearly
//! recorded a third structural defect on the strength of it. The count is
//! still printed, because the prompt's length as the server tokenised it is
//! worth seeing next to our own count — but it is not the cache signal and
//! nothing here treats it as one.
//!
//! Phase 4 had only time-to-first-token to read, which is why it could
//! conclude nothing firmer than "TTFT rose, so probably no reuse". TTFT also
//! carries queueing, sampling and the first token's own decode;
//! `prompt_eval_duration` carries none of them.

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::turn::{TurnConfig, TurnEngine, TurnOutcome};
use support::{character_response_json, fixture_session, test_tokenizer, FakeOllama};

/// The recorded Godot p50, in milliseconds, per fixture.
const GODOT_BASELINE_P50_MS: &[(&str, f64)] = &[("minimal", 21_200.0), ("messy", 20_200.0)];

/// What this turn's prompt cost the backend, per thousand tokens sent.
///
/// The reuse signal in one number. A backend evaluating every prompt in full
/// charges roughly the same per token however long the transcript gets; one
/// reusing a cached prefix charges less and less, because the tokens it skips
/// still count towards what we sent. `None` when the backend reported no
/// timing, or when the prompt was empty.
fn cost_per_1k_sent(outcome: &TurnOutcome) -> Option<f64> {
    let spent = outcome.prompt_eval_time?;
    if outcome.prompt_tokens == 0 {
        return None;
    }
    Some(spent.as_secs_f64() * 1000.0 / (outcome.prompt_tokens as f64 / 1000.0))
}

/// How much cheaper this turn's prompt was, per token, than the first turn's.
///
/// Turn 0 has nothing cached, so its per-token cost is what an uncached token
/// costs on this machine. Later turns divided by it give the fraction of the
/// prompt that stopped costing anything — reuse, measured in work rather than
/// inferred from a token count that does not move.
fn reuse_fraction(outcome: &TurnOutcome, baseline_cost_per_1k: f64) -> Option<f64> {
    if baseline_cost_per_1k <= 0.0 {
        return None;
    }
    let cost = cost_per_1k_sent(outcome)?;
    Some((1.0 - cost / baseline_cost_per_1k).clamp(0.0, 1.0))
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
        "{:>4} | {:>9} | {:>9} | {:>10} | {:>10} | {:>7} | {:>9} | {:>9}",
        "turn", "sent", "counted", "prompt ms", "ms/1k sent", "reused", "ttft", "total"
    );
    println!(
        "{:-<5}|{:-<11}|{:-<11}|{:-<12}|{:-<12}|{:-<9}|{:-<11}|{:-<11}",
        "", "", "", "", "", "", "", ""
    );

    // Turn 0 pays for everything, because nothing is cached yet. Its cost per
    // token is therefore the price of an uncached token here, and every later
    // turn is read against it.
    let baseline = outcomes.first().and_then(cost_per_1k_sent).unwrap_or(0.0);

    for (i, outcome) in outcomes.iter().enumerate() {
        println!(
            "{:>4} | {:>9} | {:>9} | {:>10} | {:>10} | {:>7} | {:>9} | {:>9}",
            i,
            outcome.prompt_tokens,
            outcome
                .evaluated_prompt_tokens
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into()),
            outcome
                .prompt_eval_time
                .map(|d| format!("{:.0}", d.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "-".into()),
            cost_per_1k_sent(outcome)
                .map(|c| format!("{c:.0}"))
                .unwrap_or_else(|| "-".into()),
            reuse_fraction(outcome, baseline)
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

    if let Some((_, baseline_ms)) = GODOT_BASELINE_P50_MS.iter().find(|(f, _)| *f == fixture) {
        println!(
            "Godot baseline p50 {baseline_ms:.0} ms  ->  {:.2}x",
            baseline_ms / p50.max(1.0)
        );
        println!(
            "(Phase 5's migration gate is p95, not p50, and it is against the same baseline.)"
        );
    }

    // The cache evidence, in work rather than in a token count: after the
    // first turn there is a prefix to reuse, so what each thousand tokens of
    // prompt costs the server should fall as the transcript grows.
    let after_first: Vec<f64> = outcomes
        .iter()
        .skip(1)
        .filter_map(|o| reuse_fraction(o, baseline))
        .collect();
    if after_first.is_empty() {
        println!(
            "\nNo backend prompt timing reported, so cache reuse is unmeasured here. \
             Fall back on time-to-first-token against a growing prompt, and treat it as weak."
        );
    } else {
        let mean = after_first.iter().sum::<f64>() / after_first.len() as f64;
        println!(
            "\nprefix reuse after turn 0: mean {:.0}% of the prompt cost nothing to evaluate",
            mean * 100.0
        );
        println!(
            "Near 0% means the prefix is not being reused and §2.7 is not reaching this \
             backend. That was Phase 4's state."
        );
        println!(
            "Read the `counted` column as the prompt's length, not as evidence: Ollama \
             reports it in full whether or not it evaluated it (tests/prefix_cache.rs)."
        );
    }

    if let (Some(first), Some(last)) = (outcomes.first(), outcomes.last()) {
        if let (Some(a), Some(b)) = (first.prompt_eval_time, last.prompt_eval_time) {
            println!(
                "prompt grew {} -> {} tokens; prompt evaluation {:.0} -> {:.0} ms",
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
    // The cache evidence is only readable if the backend's own timing reaches
    // the outcome. The stand-in reports it, so a break in the wiring between
    // `prompt_eval_duration` and `TurnOutcome` fails here rather than
    // silently printing "-" in the middle of a measurement session.
    assert!(
        outcomes.iter().all(|o| o.prompt_eval_time.is_some()),
        "the backend's prompt timing did not reach TurnOutcome, so reuse is unmeasurable"
    );
    // The property the cache evidence depends on: the prompt really does grow
    // as the transcript does, so a flat time-to-first-token would mean
    // something.
    assert!(
        outcomes.last().unwrap().prompt_tokens > outcomes.first().unwrap().prompt_tokens,
        "the prompt did not grow across turns, so the cache evidence would be vacuous"
    );
    // The assertion the Phase 4 harness was missing. The latencies from a
    // stand-in are meaningless, but what the stand-in charges for a prompt is
    // a deterministic function of the message list, not of this machine: it
    // bills a fixed rate for the tokens a caching server would have to
    // evaluate and nothing for the ones it would not (see
    // `support::evaluated_prompt_tokens`). So the thing §2.7 exists to
    // deliver can be asserted in `cargo test`, with no model, on a transcript
    // where every line differs.
    let baseline = outcomes
        .first()
        .and_then(cost_per_1k_sent)
        .expect("turn 0 must report a prompt evaluation cost");
    let reuse: Vec<f64> = outcomes
        .iter()
        .skip(1)
        .filter_map(|o| reuse_fraction(o, baseline))
        .collect();
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
