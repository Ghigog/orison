//! A campaign, played start to finish through the CLI (§5.1-§5.4).
//!
//! This is Phase 5's first exit criterion as a test rather than as a claim.
//! It creates a campaign, imports a vault, finds out who is present, travels,
//! addresses somebody, holds a conversation, and saves — through
//! [`Shell::run`], which is the same function the binary calls with `stdin`
//! and `stdout`. Nothing is reimplemented for the test's convenience, because
//! a test of a reimplementation would be a test of the reimplementation.
//!
//! The model is the stand-in from `orison_core::testing`, so the run is
//! hermetic. What it proves is that the shell drives the engine correctly, not
//! that any model is any good; that is the eval harness's job.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orison_cli::campaign;
use orison_cli::shell::{parse_command, Command, Shell};
use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::testing::{character_response_json, test_tokenizer, FakeOllama};
use orison_core::turn::{TurnConfig, TurnEngine};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

/// Everything the binary does before the first prompt, in one place.
async fn playable(
    dir: &tempfile::TempDir,
    fixture_name: &str,
) -> (Arc<TurnEngine>, Arc<Mutex<Vec<u8>>>) {
    let db = dir.path().join("orison.sqlite3");
    let store = campaign::open_store(&db).expect("open the database");
    let created = campaign::create(
        &store,
        "Thornwick",
        "2026-09-09T00:00:00Z",
        Some(&fixture(fixture_name)),
        &BTreeMap::new(),
    )
    .expect("create and import");

    let session = campaign::open_session(&store, &created.campaign.id).expect("open the session");
    assert!(
        !campaign::is_empty(&session),
        "the vault import produced no entities"
    );

    let server = FakeOllama::start(
        character_response_json(
            "He looks up from the toll book.",
            "Two coppers, same as yesterday.",
        ),
        4,
        Duration::from_millis(1),
    )
    .await;
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect to the stand-in"),
    );

    let mut config = TurnConfig::default();
    // The Director is a separate measurement; a shell test should not race it.
    config.director.enabled = false;
    let engine = TurnEngine::new(session, Arc::clone(&backend), backend, config);
    // Deliberately leaked with the engine: the stand-in must outlive the turns.
    std::mem::forget(server);
    (engine, Arc::new(Mutex::new(Vec::new())))
}

/// A writer the test can read back.
#[derive(Clone)]
struct Tee(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_campaign_is_playable_start_to_finish() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, buffer) = playable(&dir, "minimal").await;
    let mut shell = Shell::new(Arc::clone(&engine), Tee(Arc::clone(&buffer)));
    shell.greet("stand-in | two calls").unwrap();

    // The import put the player somewhere and gave them somebody to talk to,
    // so the session starts by looking around rather than by arriving.
    assert_eq!(
        engine.current_location().unwrap().map(|e| e.label),
        Some("Stonebridge".to_string()),
        "the import should have chosen a starting location"
    );

    // A whole session, as a person would type it.
    let script = "\
/where
/who
/talk Bram Holt
Morning. What's the toll?
And if I've no coin?
/go Thornwick Archive
/talk Elara Voss
Who keeps the archive?
/go Stonebridge
/save
/quit
";
    let outcomes = shell
        .run(Cursor::new(script))
        .await
        .expect("the session ran");
    engine.shutdown();

    let transcript = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();

    assert_eq!(
        outcomes.len(),
        3,
        "three lines were said, so three turns ran"
    );
    assert!(
        outcomes
            .iter()
            .all(|o| !o.response.dialogue.trim().is_empty()),
        "a turn produced no dialogue, which is the awkward silence this engine \
         is supposed to make impossible"
    );

    // Travel happened, both ways along the mill road.
    assert!(
        transcript.contains("You travel to Thornwick Archive"),
        "{transcript}"
    );
    assert!(
        transcript.contains("You travel to Stonebridge"),
        "{transcript}"
    );
    assert_eq!(
        engine.current_location().unwrap().map(|e| e.label),
        Some("Stonebridge".to_string())
    );

    // The person addressed changed, and the prompt says who it is.
    assert!(transcript.contains("You turn to Bram Holt"), "{transcript}");
    assert!(
        transcript.contains("You turn to Elara Voss"),
        "{transcript}"
    );
    assert!(transcript.contains("[Elara Voss] >"), "{transcript}");

    // The response was rendered, once, with the narration ahead of the line
    // it sets up. A response shown twice is what the complete-line fallback
    // does when it cannot tell "already streamed" from "not streaming right
    // now"; a response shown dialogue-first is what a stand-in produces when
    // its JSON is written by hand and comes back alphabetised.
    assert_eq!(
        transcript
            .matches("Two coppers, same as yesterday.")
            .count(),
        3,
        "each of the three turns should show its dialogue exactly once: {transcript}"
    );
    let narration_at = transcript
        .find("He looks up from the toll book.")
        .expect("the narration never reached the terminal");
    let dialogue_at = transcript
        .find("Two coppers, same as yesterday.")
        .expect("the dialogue never reached the terminal");
    assert!(
        narration_at < dialogue_at,
        "the character answered before the narration that sets it up: {transcript}"
    );

    // The transcript survives in the database, which is what makes save and
    // load a state of the world rather than a command.
    let campaign_id = engine.session().campaign_id().to_string();
    let history = engine
        .session()
        .with_store(|store| store.recent_history(&campaign_id, 100))
        .unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.content.contains("What's the toll")),
        "the player's line is not in the transcript"
    );
    assert!(
        history
            .iter()
            .any(|e| e.content.contains("travels to Thornwick Archive")),
        "the move is not in the transcript, so the next character is never told"
    );
}

/// Reopening is a `Session::open` away, and the transcript is still there.
#[tokio::test(flavor = "multi_thread")]
async fn a_campaign_reloads_with_its_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("orison.sqlite3");

    let id = {
        let (engine, buffer) = {
            let store = campaign::open_store(&db).unwrap();
            let created = campaign::create(
                &store,
                "Thornwick",
                "2026-09-09T00:00:00Z",
                Some(&fixture("minimal")),
                &BTreeMap::new(),
            )
            .unwrap();
            let session = campaign::open_session(&store, &created.campaign.id).unwrap();
            let server = FakeOllama::start(
                character_response_json("He nods.", "Aye."),
                2,
                Duration::from_millis(1),
            )
            .await;
            let backend: Arc<dyn InferenceBackend> = Arc::new(
                OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
                    .await
                    .unwrap(),
            );
            std::mem::forget(server);
            let mut config = TurnConfig::default();
            config.director.enabled = false;
            (
                TurnEngine::new(session, Arc::clone(&backend), backend, config),
                Arc::new(Mutex::new(Vec::new())),
            )
        };
        let mut shell = Shell::new(Arc::clone(&engine), Tee(buffer));
        shell
            .run(Cursor::new(
                "/go Stonebridge\n/talk Bram Holt\nMorning.\n/quit\n",
            ))
            .await
            .unwrap();
        let id = engine.session().campaign_id().to_string();
        engine.shutdown();
        id
    };

    // A cold open, as `orison play <id>` does it.
    let store = campaign::open_store(&db).unwrap();
    let listed = campaign::list(&store).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert!(
        listed[0].playtime_seconds >= 0.0,
        "the playtime clock was never written"
    );

    let session = campaign::open_session(&store, &id).unwrap();
    let history = session
        .with_store(|store| store.recent_history(&id, 100))
        .unwrap();
    assert!(
        history.iter().any(|e| e.content.contains("Morning.")),
        "the transcript did not survive the reload"
    );
    let campaign = session
        .with_store(|s| s.load_campaign(&id))
        .unwrap()
        .expect("the campaign is still there");
    assert!(
        campaign.active_location.contains("stonebridge"),
        "where the player was standing did not survive: {campaign:?}"
    );
}

#[test]
fn a_slash_marks_a_command_and_nothing_else_does() {
    assert_eq!(
        parse_command("go on, then").unwrap(),
        Command::Say("go on, then".into()),
        "a sentence starting with a command word is speech"
    );
    assert_eq!(
        parse_command("/go Stonebridge").unwrap(),
        Command::Go("Stonebridge".into())
    );
    assert_eq!(parse_command("  /who  ").unwrap(), Command::Who);
    // An argument-less command that needs one says so instead of being
    // silently ignored or passed to the model as dialogue.
    assert!(parse_command("/talk").unwrap_err().contains("/talk needs"));
    assert!(parse_command("/dance").unwrap_err().contains("no command"));
}

/// Obsidian's own markup is not something to show a player.
#[tokio::test(flavor = "multi_thread")]
async fn a_locations_description_is_prose_not_wiki_markup() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, buffer) = playable(&dir, "minimal").await;
    let mut shell = Shell::new(Arc::clone(&engine), Tee(Arc::clone(&buffer)));
    shell.run(Cursor::new("/where\n/quit\n")).await.unwrap();
    engine.shutdown();

    let transcript = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        transcript.contains("Connected to Thornwick Archive by the mill road."),
        "{transcript}"
    );
    assert!(
        !transcript.contains("[["),
        "wiki-link brackets reached the terminal: {transcript}"
    );
}
