//! Backend conformance suite (§2.2, §2.3).
//!
//! `OllamaBackend` and `LlamaCppBackend` (behind `--features llama-cpp`, a
//! later task) are required to pass the same suite; that shared suite is as
//! much the deliverable as either implementation. Cases that need a live
//! model endpoint are gated behind environment variables, mirroring how
//! `eval/EvalRunnerNode.gd` gates its own `--live` mode: they skip loudly
//! rather than silently passing when nothing is configured, so a run that
//! reports success never means "nothing was actually exercised."
//!
//! - `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_OLLAMA_MODEL`: a live Ollama
//!   with that model pulled.
//!
//! Everything else in this file runs unconditionally in `cargo test`, no
//! network or model required — per the Phase 1 lesson this handoff calls
//! out explicitly: "a test that cannot fail is a rubber stamp." The cases
//! below that assert on Godot-build defect numbers (B-6, B-15) are written
//! so that reverting the fix they check would fail them.

use orison_core::inference::tools::{
    self, GetCharacterProfileArgs, GetLocationDetailArgs, GetRelationshipArgs, ToolArgs,
    ToolDispatcher,
};
use orison_core::inference::{
    ChatMessage, ChatRequest, HealthStatus, InferenceBackend, InferenceError, KeepAlive,
    OllamaBackend, OllamaConfig, ResponseFormat, DEFAULT_CONTEXT_LIMIT,
};
use orison_core::prompt::schemas::CharacterResponse;
use std::str::FromStr;
use std::time::Duration;

mod support;
use support::{character_response_json, FakeOllama, ADVERTISED_CONTEXT_LENGTH};

async fn stand_in() -> FakeOllama {
    FakeOllama::start(
        character_response_json("He nods.", "Aye."),
        2,
        Duration::from_millis(1),
    )
    .await
}

/// The served window is capped, and `num_ctx` carries the capped number.
///
/// Ollama sizes the runner's KV cache from `num_ctx` at load time regardless
/// of how much of the window a prompt uses. Passing a model's advertised
/// maximum straight through therefore buys nothing and costs the whole
/// allocation — on `llama3.2:3b` that is 131072 tokens of cache for prompts
/// measured in the low thousands, and it is half of why Phase 4's live turn
/// latency came in at 2x the engine it was meant to beat.
#[tokio::test]
async fn the_served_context_window_is_capped_not_whatever_the_model_advertises() {
    let server = stand_in().await;
    let backend = OllamaBackend::connect(server.url(), "stand-in", minimal_tokenizer())
        .await
        .expect("connect");

    const {
        assert!(
            ADVERTISED_CONTEXT_LENGTH > DEFAULT_CONTEXT_LIMIT,
            "the stand-in must advertise more than the cap or this proves nothing"
        )
    };
    assert_eq!(
        backend.context_length(),
        DEFAULT_CONTEXT_LIMIT,
        "the advertised window was passed through instead of capped"
    );

    backend
        .chat(ChatRequest::new(vec![ChatMessage::user("Hello.")]))
        .await
        .expect("chat");

    let body: serde_json::Value =
        serde_json::from_str(&server.observed.last_chat_body()).expect("a JSON body");
    assert_eq!(
        body["options"]["num_ctx"].as_u64(),
        Some(DEFAULT_CONTEXT_LIMIT as u64),
        "num_ctx must be the window we actually intend to pay for"
    );
}

/// The budget and the request can never disagree, which is the defect B-1
/// actually names: `context_length()` is what `PromptBudget` sizes against,
/// and it must be the same number the wire carries.
#[tokio::test]
async fn the_prompt_budget_and_num_ctx_are_the_same_number() {
    let server = stand_in().await;
    let backend = OllamaBackend::connect_with(
        server.url(),
        "stand-in",
        minimal_tokenizer(),
        OllamaConfig {
            context_limit: Some(3000),
        },
    )
    .await
    .expect("connect");

    assert_eq!(backend.context_length(), 3000);
    backend
        .chat(ChatRequest::new(vec![ChatMessage::user("Hello.")]))
        .await
        .expect("chat");
    let body: serde_json::Value =
        serde_json::from_str(&server.observed.last_chat_body()).expect("a JSON body");
    assert_eq!(body["options"]["num_ctx"].as_u64(), Some(3000));
}

/// Raising the cap can never invent capacity the model does not have.
#[tokio::test]
async fn a_cap_above_what_the_model_advertises_still_yields_the_model_s_window() {
    let server = stand_in().await;
    let backend = OllamaBackend::connect_with(
        server.url(),
        "stand-in",
        minimal_tokenizer(),
        OllamaConfig {
            context_limit: Some(ADVERTISED_CONTEXT_LENGTH * 4),
        },
    )
    .await
    .expect("connect");
    assert_eq!(backend.context_length(), ADVERTISED_CONTEXT_LENGTH);
}

fn live_ollama_config() -> Option<(String, String)> {
    let url = std::env::var("ORISON_TEST_OLLAMA_URL").ok()?;
    let model = std::env::var("ORISON_TEST_OLLAMA_MODEL").ok()?;
    Some((url, model))
}

fn minimal_tokenizer() -> tokenizers::Tokenizer {
    tokenizers::Tokenizer::from_str(
        r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0},"unk_token":"[UNK]"}}"#,
    )
    .expect("minimal tokenizer json is valid")
}

/// B-15's exact failure mode, for the backend that could actually be
/// misconfigured this way: an endpoint nothing is listening on. In the
/// Godot build this produced a 404 a caller could ignore. Here it must come
/// back as a typed error, unconditionally — no live server required, since
/// a refused local connection is a condition this test controls itself
/// rather than one it hopes an external service produces (the Phase 1
/// lesson about asserting only against behaviour you control).
#[tokio::test]
async fn unreachable_ollama_endpoint_is_a_typed_error_not_a_silent_failure() {
    // Port 1 is a privileged port nothing binds to in a test environment;
    // the connection is refused rather than merely slow.
    let result =
        OllamaBackend::connect("http://127.0.0.1:1", "any-model", minimal_tokenizer()).await;

    match result {
        Err(InferenceError::Unreachable { .. }) => {}
        other => {
            panic!("expected InferenceError::Unreachable for a refused connection, got: {other:?}")
        }
    }
}

/// B-6: the Godot build sends `keep_alive: -1` on every request, pinning
/// both models in VRAM forever. The typed default here must not reproduce
/// that.
#[test]
fn keep_alive_defaults_to_a_bounded_duration_not_forever() {
    assert_ne!(
        KeepAlive::default(),
        KeepAlive::Forever,
        "B-6: the default must not pin the model in memory indefinitely"
    );
}

#[tokio::test]
async fn live_ollama_health_reports_available_for_an_installed_model() {
    let Some((url, model)) = live_ollama_config() else {
        eprintln!(
            "SKIPPED: set ORISON_TEST_OLLAMA_URL and ORISON_TEST_OLLAMA_MODEL to run this against a live Ollama"
        );
        return;
    };
    let backend = OllamaBackend::connect(&url, &model, minimal_tokenizer())
        .await
        .expect("connect to configured live Ollama");

    let health = backend
        .health()
        .await
        .expect("health() must not error for a reachable server");
    assert_eq!(health.status, HealthStatus::Available);
    assert!(health.is_available());
}

#[tokio::test]
async fn live_ollama_chat_honours_schema_constrained_decoding() {
    let Some((url, model)) = live_ollama_config() else {
        eprintln!(
            "SKIPPED: set ORISON_TEST_OLLAMA_URL and ORISON_TEST_OLLAMA_MODEL to run this against a live Ollama"
        );
        return;
    };
    let backend = OllamaBackend::connect(&url, &model, minimal_tokenizer())
        .await
        .expect("connect to configured live Ollama");

    let req = ChatRequest::new(vec![
        ChatMessage::system("You are a tavern guard. Reply in character."),
        ChatMessage::user("What's your name?"),
    ])
    .with_response_format(ResponseFormat::for_type::<CharacterResponse>());

    let response = backend
        .chat(req)
        .await
        .expect("chat() must succeed against a live server");
    // Exit bar for §2.4: schema validity is 100% by construction. If this
    // parse fails, the constraint was not actually applied by the backend.
    let parsed: CharacterResponse = response.parse().expect(
        "response must parse as CharacterResponse: schema-constrained decoding was not honoured",
    );
    assert!(!parsed.thinking.is_empty());
}

// ---------------------------------------------------------------------
// Native tool calling (§2.6).
// ---------------------------------------------------------------------

/// §2.6: the tool set offered to the Director is the three tools suited to
/// unpredictable lookups, not the original four — `search_knowledge_graph`
/// is deliberately excluded here; the handoff moves it to a deterministic
/// Phase 3 pre-pass rather than a model-chosen tool call.
#[test]
fn director_tool_set_is_three_tools_and_excludes_search_knowledge_graph() {
    let defs = tools::director_tools();
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names.len(), 3, "tool count must stay in the 3-5 range");
    assert!(names.contains(&GetCharacterProfileArgs::NAME));
    assert!(names.contains(&GetLocationDetailArgs::NAME));
    assert!(names.contains(&GetRelationshipArgs::NAME));
    assert!(
        !names.contains(&"search_knowledge_graph"),
        "search_knowledge_graph must become a Phase 3 pre-pass, not a tool call"
    );
}

struct MockDispatcher;

#[async_trait::async_trait]
impl ToolDispatcher for MockDispatcher {
    async fn get_character_profile(&self, args: GetCharacterProfileArgs) -> String {
        format!("profile for {}", args.character_name)
    }
    async fn get_location_detail(&self, args: GetLocationDetailArgs) -> String {
        format!("detail for {}", args.location_name)
    }
    async fn get_relationship(&self, args: GetRelationshipArgs) -> String {
        format!(
            "relationship between {} and {}",
            args.entity_a, args.entity_b
        )
    }
}

/// A model's tool call is decoded into the exact typed struct its name
/// promises, and dispatched to the matching handler — "typed and validated
/// by code, not parsed from prose" (§2.6's exit bar), demonstrated without
/// needing a live model to actually emit the call.
#[tokio::test]
async fn tool_calls_are_typed_and_dispatched_by_name() {
    let dispatcher = MockDispatcher;
    let call = orison_core::inference::ToolCall {
        id: Some("call-1".to_string()),
        name: GetCharacterProfileArgs::NAME.to_string(),
        arguments: serde_json::json!({ "character_name": "Elowen" }),
    };

    let result = tools::dispatch(&dispatcher, &call).await;
    assert_eq!(result.unwrap().unwrap(), "profile for Elowen");

    let unknown_call = orison_core::inference::ToolCall {
        id: None,
        name: "not_a_real_tool".to_string(),
        arguments: serde_json::json!({}),
    };
    assert!(tools::dispatch(&dispatcher, &unknown_call).await.is_none());
}

/// A tool call whose arguments don't match its own declared schema is a
/// typed decode error, not a silently empty/default struct.
#[tokio::test]
async fn malformed_tool_call_arguments_are_a_typed_error() {
    let dispatcher = MockDispatcher;
    let call = orison_core::inference::ToolCall {
        id: None,
        name: GetRelationshipArgs::NAME.to_string(),
        // Missing `entity_b`, which `GetRelationshipArgs` requires.
        arguments: serde_json::json!({ "entity_a": "Elowen" }),
    };
    let result = tools::dispatch(&dispatcher, &call).await;
    assert!(matches!(result, Some(Err(InferenceError::Decode(_)))));
}
