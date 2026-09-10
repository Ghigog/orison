//! Does this Ollama reuse a shared prompt prefix, and does it say so?
//!
//! `tests/turn_latency.rs` reads cache reuse out of `prompt_eval_count`, on
//! the documented understanding that tokens served from the server's own
//! prompt cache are not counted. The Phase 5.6 live run made that reading
//! load-bearing and then contradicted it: reuse read 0% on every turn of both
//! fixtures while `evaluated` tracked `sent` at a constant ratio, which is
//! what a full re-evaluation looks like — and also what a server that reports
//! the full count regardless would look like.
//!
//! Those two are indistinguishable from the latency table alone, and they
//! call for opposite work. This test separates them without involving the
//! engine at all: three requests sharing one long system prefix, straight at
//! `/api/chat`.
//!
//! ```bash
//! ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
//! ORISON_TEST_OLLAMA_MODEL=llama3.2:3b \
//!   cargo test -p orison-core --test prefix_cache -- --nocapture
//! ```
//!
//! Read the two columns together:
//!
//! - **count falls sharply after the first** — the server reuses the prefix
//!   and reports it honestly. The metric is sound, the 0% in the latency run
//!   is real, and the engine's prompts are not sharing a prefix in live play
//!   despite `turn_loop.rs` asserting that they do. The bug is ours.
//! - **count stays flat, duration collapses** — the server reuses the prefix
//!   and reports the full count anyway. `prompt_eval_count` is not a reuse
//!   signal on this build, `turn_latency`'s reuse column measures nothing,
//!   and both need replacing with `prompt_eval_duration`.
//! - **both stay flat** — this server is not reusing prefixes at all, for
//!   reasons outside the prompt: parallel slots, cache type, a runner that
//!   drops the cache between requests. Ordering cannot fix that and the
//!   investigation belongs in configuration.

use std::time::Duration;

/// Long enough that reuse is unmistakable against per-message template
/// overhead, and stable to the byte across all three requests.
fn shared_system_prefix() -> String {
    "The archive keeps its ledgers in the north aisle. ".repeat(200)
}

#[derive(serde::Deserialize)]
struct Reply {
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    prompt_eval_duration: Option<u64>,
}

#[tokio::test]
async fn does_this_server_reuse_a_shared_prefix() {
    let (Ok(url), Ok(model)) = (
        std::env::var("ORISON_TEST_OLLAMA_URL"),
        std::env::var("ORISON_TEST_OLLAMA_MODEL"),
    ) else {
        eprintln!(
            "SKIPPED does_this_server_reuse_a_shared_prefix: set \
             ORISON_TEST_OLLAMA_URL and ORISON_TEST_OLLAMA_MODEL."
        );
        return;
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .expect("build a client");
    let system = shared_system_prefix();

    println!("\nPrefix reuse, straight at /api/chat");
    println!("model: {model}");
    println!("{:>5} | {:>9} | {:>11}", "call", "evaluated", "prompt eval");
    println!("{:-<6}|{:-<11}|{:-<13}", "", "", "");

    let mut counts = Vec::new();
    let mut durations = Vec::new();

    // The same system message every time; only the trailing question moves.
    // A server that reuses prefixes has nothing to evaluate after the first
    // call but the question.
    for (i, question) in [
        "What colour is the stove?",
        "How many aisles are there?",
        "Who keeps the ledgers?",
    ]
    .iter()
    .enumerate()
    {
        let body = serde_json::json!({
            "model": model,
            "stream": false,
            "keep_alive": "5m",
            "options": { "num_ctx": 8192, "temperature": 0, "num_predict": 16 },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": question },
            ],
        });
        let reply: Reply = client
            .post(format!("{}/api/chat", url.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .expect("reach the configured Ollama")
            .json()
            .await
            .expect("a JSON reply");

        let count = reply.prompt_eval_count.unwrap_or(0);
        let ms = reply.prompt_eval_duration.unwrap_or(0) as f64 / 1e6;
        println!("{:>5} | {:>9} | {:>8.0} ms", i, count, ms);
        counts.push(count);
        durations.push(ms);
    }

    let first_count = counts[0].max(1) as f64;
    let later_count = counts[1..].iter().sum::<u64>() as f64 / (counts.len() - 1) as f64;
    let first_ms = durations[0].max(0.001);
    let later_ms = durations[1..].iter().sum::<f64>() / (durations.len() - 1) as f64;

    let count_fell = later_count < first_count * 0.5;
    let duration_fell = later_ms < first_ms * 0.5;

    println!(
        "\ncount {:.0} -> {:.0} ({:.0}%)   prompt eval {:.0} ms -> {:.0} ms ({:.0}%)",
        first_count,
        later_count,
        later_count / first_count * 100.0,
        first_ms,
        later_ms,
        later_ms / first_ms * 100.0,
    );

    println!(
        "\n{}",
        match (count_fell, duration_fell) {
            (true, _) =>
                "REUSED, AND REPORTED. `prompt_eval_count` is a sound reuse signal here, so \
                 the 0% in the Phase 5.6 latency run is real: the engine's consecutive \
                 prompts are not sharing a prefix in live play. Look at the engine.",
            (false, true) =>
                "REUSED, BUT NOT REPORTED. The count is flat and the work collapsed, so \
                 `prompt_eval_count` reports the whole prompt whether or not it was cached. \
                 `turn_latency`'s reuse column measures nothing on this build and must be \
                 rebuilt on `prompt_eval_duration`.",
            (false, false) =>
                "NOT REUSED AT ALL. Neither the count nor the work fell across three \
                 requests sharing a byte-identical prefix, so this server is not reusing \
                 prefixes regardless of what the engine sends. Look at the server: \
                 parallel slots, cache type, keep-alive.",
        }
    );
}
