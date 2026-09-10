//! The evaluation harness, run against the CLI (§5.5).
//!
//! Phase 5's exit criteria ask for the harness to be run "against the CLI",
//! and this is what makes that phrase mean something: the fixture transcripts
//! are typed into [`Shell::run`], the same function the binary hands `stdin`
//! and `stdout` to. Nothing is replayed through a private path built for the
//! measurement, because a measurement of a private path measures the private
//! path.
//!
//! Two halves, and they are graded differently on purpose:
//!
//! - **Deterministic** (`turn::experiment`): schema validity, pronoun flags,
//!   verbatim repeats, forbidden phrasing, empty responses. Runs in
//!   `cargo test` with no model. **This is the gate**, per §1.3.
//! - **Judged** (`turn::judge`): the four 1-5 axes, scored by a model. Gated
//!   on `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_JUDGE_MODEL`, skips loudly
//!   when unset, and gates nothing.
//!
//! ```bash
//! # Deterministic, no model:
//! cargo test -p orison-cli --test harness -- --nocapture
//!
//! # With a judge, which should be a larger model than the one under test:
//! ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
//! ORISON_TEST_ACTOR_MODEL=llama3.2:3b \
//! ORISON_TEST_JUDGE_MODEL=qwen2.5:7b-instruct \
//!   cargo test -p orison-cli --test harness -- --nocapture
//! ```

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orison_cli::campaign;
use orison_cli::shell::Shell;
use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::knowledge::{CanonicalField, EntityId};
use orison_core::prompt::assembly::CharacterCard;
use orison_core::testing::{
    character_response_json, fixture_root, fixture_script, test_tokenizer, FakeOllama,
};
use orison_core::turn::{
    experiment, ArmReport, Judge, JudgedTurnInput, TranscriptScript, TurnConfig, TurnEngine,
    TurnOutcome,
};

const FIXTURES: [&str; 2] = ["minimal", "messy"];

/// A writer that keeps what was written, so a scripted run can be read back.
#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Played {
    engine: Arc<TurnEngine>,
    script: TranscriptScript,
    outcomes: Vec<TurnOutcome>,
    transcript: String,
}

/// Play a fixture's scripted transcript through the shell.
///
/// `backend` decides what is being measured: the stand-in when the point is
/// the harness, a live model when the point is the model.
async fn play(
    fixture: &str,
    dir: &tempfile::TempDir,
    backend: Arc<dyn InferenceBackend>,
) -> Played {
    let script = fixture_script(fixture);
    let store = campaign::open_store(&dir.path().join(format!("{fixture}.sqlite3")))
        .expect("open the database");
    let created = campaign::create(
        &store,
        fixture,
        "2026-09-09T00:00:00Z",
        Some(&fixture_root(fixture)),
        &BTreeMap::new(),
    )
    .expect("create and import");
    let session = campaign::open_session(&store, &created.campaign.id).expect("open the session");

    let mut config = TurnConfig::default();
    // The Actor turn alone, which is what the Godot narrative baseline
    // measured. The Director's cost belongs to the §4.2 comparison.
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), backend, config);

    // Typed, not injected: `/talk` is how a player chooses who to address,
    // so the measurement uses it too.
    let mut typed = format!("/talk {}\n", script.character);
    for line in &script.lines {
        typed.push_str(line);
        typed.push('\n');
    }
    typed.push_str("/quit\n");

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut shell = Shell::new(Arc::clone(&engine), Buffer(Arc::clone(&buffer)));
    let outcomes = shell
        .run(Cursor::new(typed))
        .await
        .expect("the scripted session ran");
    let transcript = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    Played {
        engine,
        script,
        outcomes,
        transcript,
    }
}

fn score(played: &Played) -> ArmReport {
    let mut report = ArmReport::new("orison-cli");
    let mut seen = HashMap::new();
    for (index, outcome) in played.outcomes.iter().enumerate() {
        report.turns.push(experiment::score_turn(
            index,
            &played.script,
            outcome,
            &mut seen,
        ));
    }
    report
}

/// The deterministic half, which is the gate.
///
/// Against the stand-in, so it runs in CI with no model. What it can prove is
/// that the shell drives a whole scripted transcript to completion and that
/// every deterministic metric is computable from what comes back — not that
/// any model is any good, which is the judge's half and is not a gate.
#[tokio::test(flavor = "multi_thread")]
async fn every_fixture_plays_through_the_cli_and_scores() {
    for fixture in FIXTURES {
        let dir = tempfile::tempdir().unwrap();
        let server = FakeOllama::start(
            character_response_json(
                "He sets the ledger down and considers the question.",
                "Ask me plainly and I will answer plainly.",
            ),
            6,
            Duration::from_millis(1),
        )
        .await;
        let backend: Arc<dyn InferenceBackend> = Arc::new(
            OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
                .await
                .expect("connect to the stand-in"),
        );
        let played = play(fixture, &dir, backend).await;
        played.engine.shutdown();

        assert_eq!(
            played.outcomes.len(),
            played.script.lines.len(),
            "{fixture}: every scripted line must produce a turn"
        );

        let report = score(&played);
        println!("\n=== {fixture} (stand-in; the numbers grade the harness) ===");
        println!("{}", ArmReport::header());
        println!("{}", report.row());

        assert_eq!(
            report.schema_validity(),
            1.0,
            "{fixture}: under constrained decoding schema validity is 100% or there \
             is a bug in the harness or the backend"
        );
        assert_eq!(
            report.empty_responses(),
            0,
            "{fixture}: an empty response reached the player as an awkward silence"
        );
        assert!(
            !played.transcript.is_empty(),
            "{fixture}: nothing was rendered to the terminal"
        );
        // The stand-in answers identically every turn, so the loop-detection
        // metric must fire. A harness where it cannot is a rubber stamp.
        assert!(
            report.verbatim_repeats() > 0,
            "{fixture}: the stand-in repeats itself verbatim and loop detection \
             did not notice, so the metric cannot fire at all"
        );
    }
}

/// The judged half. Skips loudly when no judge is configured.
///
/// **This gates nothing**, per §1.3: judge scores are noisy and are for trend
/// detection across many samples. It prints a report and asserts only that
/// the report was actually produced — a suite that "passed" without scoring
/// anything is the failure this project keeps writing tests against.
#[tokio::test(flavor = "multi_thread")]
async fn the_judge_suite_scores_a_played_transcript() {
    let (Ok(url), Ok(judge_model)) = (
        std::env::var("ORISON_TEST_OLLAMA_URL"),
        std::env::var("ORISON_TEST_JUDGE_MODEL"),
    ) else {
        eprintln!(
            "SKIPPED the_judge_suite_scores_a_played_transcript: set \
             ORISON_TEST_OLLAMA_URL and ORISON_TEST_JUDGE_MODEL (and optionally \
             ORISON_TEST_ACTOR_MODEL). The judge should be a larger model than the \
             one under test; judging a model with itself is not a measurement."
        );
        return;
    };
    let actor_model =
        std::env::var("ORISON_TEST_ACTOR_MODEL").unwrap_or_else(|_| judge_model.clone());
    if actor_model == judge_model {
        eprintln!(
            "WARNING: the judge and the model under test are both {judge_model}. §1.3 \
             asks for a larger judge; these scores measure a model grading itself."
        );
    }

    let actor: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(&url, &actor_model, test_tokenizer())
            .await
            .expect("connect the model under test"),
    );
    let judge = Judge::new(
        Arc::new(
            OllamaBackend::connect(&url, &judge_model, test_tokenizer())
                .await
                .expect("connect the judge"),
        ),
        &judge_model,
    );

    for fixture in FIXTURES {
        let dir = tempfile::tempdir().unwrap();
        let played = play(fixture, &dir, Arc::clone(&actor)).await;

        let deterministic = score(&played);
        println!("\n=== {fixture} ===");
        println!("{}", ArmReport::header());
        println!("{}", deterministic.row());

        let inputs = evidence(&played);
        let report = judge
            .score_all(fixture, &inputs)
            .await
            .expect("the judge scored the transcript");
        println!("{}", report.render());
        assert_eq!(
            report.len(),
            played.outcomes.len(),
            "{fixture}: the judge skipped a turn, so the report covers an unknown subset"
        );
        played.engine.shutdown();
    }
}

/// What the judge is shown, rebuilt from the same graph the Actor read.
///
/// The lore comes off the outcome rather than from a fresh query: the Actor
/// saw one set of passages and re-running retrieval would ask a different
/// question of a database that has since moved.
fn evidence(played: &Played) -> Vec<JudgedTurnInput> {
    let graph = played.engine.session().graph();
    played
        .outcomes
        .iter()
        .enumerate()
        .map(|(index, outcome)| {
            let entity = graph.get(&EntityId::from_stored(&outcome.speaker));
            let card = CharacterCard {
                name: entity.map(|e| e.label.as_str()).unwrap_or(&outcome.speaker),
                gender: entity.and_then(|e| e.field(CanonicalField::Gender)),
                biography: entity.and_then(|e| e.field(CanonicalField::Biography)),
                personality: entity.and_then(|e| e.field(CanonicalField::Personality)),
                appearance: entity.and_then(|e| e.field(CanonicalField::Appearance)),
                goals: entity.and_then(|e| e.field(CanonicalField::Goals)),
                writing_style: None,
            };
            JudgedTurnInput {
                character_card: card.render(),
                retrieved_lore: outcome.retrieved_context.clone(),
                player_input: played.script.lines.get(index).cloned().unwrap_or_default(),
                narration: outcome.response.narration.clone(),
                dialogue: outcome.response.dialogue.clone(),
            }
        })
        .collect()
}
