//! The Director/Actor experiment (§4.2).
//!
//! **Run as an experiment, not an assumption.** The split exists because a 3B
//! Actor could not also do Director work, and it costs: two models resident,
//! doubled load time, cross-model consistency problems, and the whole
//! background-director apparatus including a stalling prompt that exists
//! purely to paper over Director latency. Current 8B-class models are
//! substantially more capable. Whether that changes the answer is a question
//! about models, so it is settled by running models.
//!
//! Three arms, on identical transcripts:
//!
//! | Arm | Configuration |
//! |---|---|
//! | **A** | The current split: Director 8B + Actor 3B, two calls per turn |
//! | **B** | One 8B model, two system prompts, two calls per turn |
//! | **C** | One 8B model, one call per turn, combined schema |
//!
//! A and B are the *same code path* with different backends configured, which
//! is the handoff's requirement that the arms be configuration profiles
//! rather than three code paths. Only C differs in code, as
//! `TurnProfile::SingleCall`.
//!
//! ```bash
//! ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
//! ORISON_EXPERIMENT_DIRECTOR_MODEL=gemma3:12b \
//! ORISON_EXPERIMENT_ACTOR_MODEL=llama3.2:3b \
//! ORISON_EXPERIMENT_SINGLE_MODEL=gemma3:12b \
//!   cargo test -p orison-core --test director_actor_experiment -- --nocapture
//! ```
//!
//! It skips loudly when those are unset. The scoring half runs unconditionally
//! in `cargo test` as unit tests on `turn::experiment`, so the metrics are
//! known to fire before anyone trusts a table they produce.

mod support;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::turn::{
    experiment::{failed_turn, score_turn},
    ArmReport, TranscriptScript, TurnConfig, TurnEngine, TurnProfile,
};
use support::{
    character_response_json, combined_response_json, director_response_json, fixture_session,
    test_tokenizer, FakeOllama,
};

/// Fixtures whose `ground_truth.json` carries a transcript. The same two the
/// Godot narrative baseline was recorded on.
const FIXTURES: &[&str] = &["minimal", "messy"];

struct Arm {
    name: &'static str,
    actor_model: String,
    director_model: String,
    profile: TurnProfile,
}

fn arms() -> Option<(String, Vec<Arm>)> {
    let url = std::env::var("ORISON_TEST_OLLAMA_URL").ok()?;
    let director = std::env::var("ORISON_EXPERIMENT_DIRECTOR_MODEL").ok()?;
    let actor = std::env::var("ORISON_EXPERIMENT_ACTOR_MODEL").ok()?;
    let single = std::env::var("ORISON_EXPERIMENT_SINGLE_MODEL").ok()?;
    Some((
        url,
        vec![
            Arm {
                name: "A: split (director + actor)",
                actor_model: actor,
                director_model: director,
                profile: TurnProfile::TwoCalls,
            },
            Arm {
                name: "B: one model, two calls",
                actor_model: single.clone(),
                director_model: single.clone(),
                profile: TurnProfile::TwoCalls,
            },
            Arm {
                name: "C: one model, one call",
                actor_model: single.clone(),
                director_model: single,
                profile: TurnProfile::SingleCall,
            },
        ],
    ))
}

async fn run_arm(url: &str, arm: &Arm, fixture: &str) -> ArmReport {
    let (session, _store, script) = fixture_session(fixture);
    let actor: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(url, &arm.actor_model, test_tokenizer())
            .await
            .expect("connect the actor model"),
    );
    let director: Arc<dyn InferenceBackend> = if arm.director_model == arm.actor_model {
        Arc::clone(&actor)
    } else {
        Arc::new(
            OllamaBackend::connect(url, &arm.director_model, test_tokenizer())
                .await
                .expect("connect the director model"),
        )
    };

    let mut config = TurnConfig {
        profile: arm.profile,
        ..TurnConfig::default()
    };
    // Every turn should exercise the Director's work, or the arms are not
    // comparable: C does it on every turn by construction.
    config.director.turn_threshold = 1;
    config.director.cooldown_turns = 0;

    let engine = TurnEngine::new(session, actor, director, config);
    score_transcript(&engine, &script, &format!("{} [{fixture}]", arm.name)).await
}

async fn score_transcript(
    engine: &Arc<TurnEngine>,
    script: &TranscriptScript,
    label: &str,
) -> ArmReport {
    let mut report = ArmReport::new(label.to_string());
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (index, line) in script.lines.iter().enumerate() {
        match engine.player_input(line.clone()).await {
            Ok(outcome) => report
                .turns
                .push(score_turn(index, script, &outcome, &mut seen)),
            Err(error) => report.turns.push(failed_turn(index, error.to_string())),
        }
        // A composed beat is part of the turn's cost either way: on the
        // two-call arms it lands in `pending_scene` and is shown at the next
        // quiet moment, which is here.
        let _ = engine.consume_pending_scene();
    }
    report
}

/// The harness itself, run against the loopback stand-in.
///
/// "A dependency compiling is not a dependency working." Without this, every
/// line of the experiment would be untested until the moment someone points
/// it at a real model — and a harness that falls over then costs a whole
/// measurement session. This runs both code paths (`TwoCalls` and
/// `SingleCall`) over a real fixture transcript and asserts the report comes
/// back populated. It measures nothing about models, and is not evidence for
/// any arm.
#[tokio::test(flavor = "multi_thread")]
async fn the_harness_works_without_a_model() {
    let server = FakeOllama::by_schema(
        character_response_json(
            "Bram sets down the tally stick.",
            "Two coppers, same as ever.",
        ),
        director_response_json("A cart rattles past the gate."),
        combined_response_json("Bram sets down the tally stick.", "Two coppers."),
        6,
        Duration::from_millis(1),
    )
    .await;

    for profile in [TurnProfile::TwoCalls, TurnProfile::SingleCall] {
        let (session, _store, script) = fixture_session("minimal");
        let backend: Arc<dyn InferenceBackend> = Arc::new(
            OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
                .await
                .expect("connect to the stand-in"),
        );
        let mut config = TurnConfig {
            profile,
            ..TurnConfig::default()
        };
        config.director.turn_threshold = 1;
        config.director.cooldown_turns = 0;

        let engine = TurnEngine::new(session, Arc::clone(&backend), Arc::clone(&backend), config);
        let report = score_transcript(&engine, &script, &format!("stand-in {profile:?}")).await;

        assert_eq!(
            report.attempted(),
            script.lines.len(),
            "{profile:?}: every scripted line should have produced a turn"
        );
        assert_eq!(
            report.parsed(),
            report.attempted(),
            "{profile:?}: {:?}",
            report.turns
        );
        assert!(report.latency_p50_ms().is_some());
        assert!(report.quality_per_second().is_some());
        assert!(report.total_prompt_tokens() > 0, "tokens were not counted");
        // The stand-in says the same thing every turn, so the repeat metric
        // must notice. A harness whose metrics never fire is a rubber stamp.
        assert!(
            report.verbatim_repeats() > 0,
            "{profile:?}: identical dialogue every turn went undetected"
        );
        println!("{}", ArmReport::header());
        println!("{}", report.row());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn compare_the_three_arms() {
    let Some((url, arms)) = arms() else {
        eprintln!(
            "SKIPPED compare_the_three_arms: set ORISON_TEST_OLLAMA_URL, \
             ORISON_EXPERIMENT_DIRECTOR_MODEL, ORISON_EXPERIMENT_ACTOR_MODEL and \
             ORISON_EXPERIMENT_SINGLE_MODEL to run the §4.2 experiment."
        );
        return;
    };

    println!("\nDirector/Actor experiment (migration_plan.md §4.2)");
    println!("{}", ArmReport::header());

    let mut reports = Vec::new();
    for arm in &arms {
        for fixture in FIXTURES {
            let report = run_arm(&url, arm, fixture).await;
            println!("{}", report.row());
            reports.push(report);
        }
    }

    println!(
        "\nprn = pronoun flags (read each by hand; the metric cannot tell a wrong pronoun for \
         the speaker\n      from a right one for a third party). rep = verbatim repeats. \
         bad = forbidden phrasing\n      plus empty responses. quality is a deterministic \
         composite in [0,1]; it is not the\n      judge suite, which is Phase 5's."
    );

    for report in &reports {
        for turn in &report.turns {
            for finding in &turn.findings {
                println!("  {} turn {}: {finding:?}", report.arm, turn.index);
            }
        }
    }

    let best = reports
        .iter()
        .filter_map(|r| r.quality_per_second().map(|q| (r, q)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    match best {
        Some((report, q)) => println!("\nHighest quality per second: {} ({q:.4})", report.arm),
        None => println!("\nNo arm produced a scorable turn."),
    }

    // Schema validity is the one number that is a hard failure rather than a
    // comparison: under constrained decoding (§2.4) it is 1.000 by
    // construction, and anything less means that claim is wrong.
    for report in &reports {
        assert_eq!(
            report.parsed(),
            report.attempted(),
            "{}: {} of {} responses parsed — constrained decoding did not hold",
            report.arm,
            report.parsed(),
            report.attempted(),
        );
    }
}
