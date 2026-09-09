//! Backend conformance suite (§2.2, §2.3).
//!
//! `OllamaBackend` and `LlamaCppBackend` (behind `--features llama-cpp`) are
//! required to pass the same suite; that shared suite is as much the
//! deliverable as either implementation. Cases that need a live model
//! endpoint are gated behind environment variables, mirroring how
//! `eval/EvalRunnerNode.gd` gates its own `--live` mode: they skip loudly
//! rather than silently passing when nothing is configured, so a run that
//! reports success never means "nothing was actually exercised."
//!
//! - `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_OLLAMA_MODEL`: a live Ollama
//!   with that model pulled.
//! - `ORISON_TEST_GGUF_MODEL` (only with `--features llama-cpp`): reserved
//!   for `LlamaCppBackend` conformance cases. Not yet exercised here — see
//!   `crate::inference::llamacpp`'s module doc for why.
//!
//! Everything else in this file runs unconditionally in `cargo test`, no
//! network or model required — per the Phase 1 lesson this handoff calls
//! out explicitly: "a test that cannot fail is a rubber stamp." The cases
//! below that assert on Godot-build defect numbers (B-6, B-15) are written
//! so that reverting the fix they check would fail them.

use orison_core::inference::{
    HealthStatus, InferenceBackend, InferenceError, KeepAlive, OllamaBackend,
};
use std::str::FromStr;

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
