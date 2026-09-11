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
//!   cargo test -p orison-core --test prefix_cache -- --nocapture --test-threads=1
//! ```
//!
//! **`--test-threads=1` is not optional.** Both probes here measure a server's
//! prompt cache, which is shared mutable state on the other side of the wire.
//! Run them concurrently and each one's requests evict the other's prefix
//! between calls, so the timings measure contention. They also take distinct
//! system text for the same reason. Without both, this file reports the
//! contention and calls it a cache.
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
/// overhead, and stable to the byte across all of one probe's requests.
///
/// **Each probe gets its own text.** Both tests in this file talk to the same
/// server, `cargo test` runs them on separate threads, and a server's prompt
/// cache is shared state. When they shared this string, the second probe's
/// turn 0 — its uncached baseline — landed on a slot the first probe had
/// already warmed, and every later turn was measured against a baseline that
/// was itself a cache hit. The verdict came out inverted. Distinct text per
/// probe is what makes each one's turn 0 mean what it says.
fn system_prefix(tag: &str) -> String {
    format!("The {tag} keeps its ledgers in the north aisle. ").repeat(200)
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
    let system = system_prefix("archive");

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

/// Does this server still reuse when the conversation *grows* and the tail
/// moves — which is the shape every real turn has?
///
/// The probe above holds one system message fixed and varies only the
/// question. That is the easy case, and `llama3.2:3b` passes it outright.
/// A turn is harder in a specific way: the previous request ended with
/// blocks that this one does not have in the same place. Request *n* is
///
/// ```text
/// [system][turn 1 .. turn n-1][fresh tail][question]
/// ```
///
/// so the common prefix with request *n-1* ends where *n-1*'s tail began —
/// the cached sequence is longer than the prefix now shared with it, and the
/// server has to keep a proper prefix of what it holds and discard the rest.
///
/// `tests/prefix_growth.rs` proves the engine builds exactly this shape, with
/// a prefix that grows every turn, on both fixtures and with one model call
/// per turn. Phase 5.6's live run nonetheless measured a cached region that
/// stayed flat at roughly 715 tokens on `minimal` while the prompt doubled,
/// and no reuse at all on `messy`. Either the server does not reuse this
/// shape, or something below the message list does not survive it.
///
/// Read the `prompt ms` column against the growing prompt:
///
/// - **flat** — the server reuses a growing conversation. The engine's
///   prompts are cache-stable and the shortfall is elsewhere: look below the
///   message list, at how the prompt renders.
/// - **rising with the prompt** — the server only reuses when nothing follows
///   the cached region. The ordering work cannot fix that, and the engine has
///   to stop moving blocks behind the transcript: the tail is the problem,
///   not its contents.
#[tokio::test]
async fn does_this_server_reuse_a_growing_conversation() {
    let (Ok(url), Ok(model)) = (
        std::env::var("ORISON_TEST_OLLAMA_URL"),
        std::env::var("ORISON_TEST_OLLAMA_MODEL"),
    ) else {
        eprintln!(
            "SKIPPED does_this_server_reuse_a_growing_conversation: set \
             ORISON_TEST_OLLAMA_URL and ORISON_TEST_OLLAMA_MODEL."
        );
        return;
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .expect("build a client");
    let system = system_prefix("almonry");

    println!("\nPrefix reuse with a growing conversation and a moving tail");
    println!("model: {model}");
    println!(
        "{:>5} | {:>9} | {:>11} | {:>11}",
        "turn", "sent", "prompt eval", "ms/1k sent"
    );
    println!("{:-<6}|{:-<11}|{:-<13}|{:-<13}", "", "", "", "");

    // The transcript, grown one exchange at a time, exactly as a scene does.
    let mut transcript: Vec<serde_json::Value> = Vec::new();
    let mut costs = Vec::new();

    for turn in 0..6 {
        let question = format!("Question number {turn}: what stands in the north aisle?");

        let mut messages = vec![serde_json::json!({ "role": "system", "content": system })];
        messages.extend(transcript.iter().cloned());
        // The volatile tail: new on every call, and ordered behind the
        // transcript exactly as `prompt::ordering` puts it.
        messages.push(serde_json::json!({
            "role": "system",
            "content": format!("Retrieved for this turn only, turn {turn}: \
                                the ledgers were rebound in the spring of the {turn}th year."),
        }));
        messages.push(serde_json::json!({ "role": "user", "content": question }));

        let body = serde_json::json!({
            "model": model,
            "stream": false,
            "keep_alive": "5m",
            "options": { "num_ctx": 8192, "temperature": 0, "num_predict": 24 },
            "messages": messages,
        });
        let sent_words: usize = body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| {
                m["content"]
                    .as_str()
                    .unwrap_or("")
                    .split_whitespace()
                    .count()
            })
            .sum();

        let reply: Reply = client
            .post(format!("{}/api/chat", url.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .expect("reach the configured Ollama")
            .json()
            .await
            .expect("a JSON reply");

        let ms = reply.prompt_eval_duration.unwrap_or(0) as f64 / 1e6;
        let per_1k = ms / (sent_words as f64 / 1000.0);
        println!("{turn:>5} | {sent_words:>9} | {ms:>8.0} ms | {per_1k:>11.0}");
        costs.push((sent_words, ms, per_1k));

        // Grow the transcript by this exchange, which is what makes the next
        // request's shared prefix longer than this one's was.
        transcript.push(serde_json::json!({ "role": "user", "content": question }));
        transcript.push(serde_json::json!({
            "role": "assistant",
            "content": "A copper kettle sits in the stove at the far end, and it has never \
                        been seen to move while observed. The ledgers beside it are bound in \
                        calfskin and shelved by year.",
        }));
    }

    // Turn 0 caches nothing, so its per-token cost is what an uncached token
    // costs here. Every later turn is read against it — the same baseline
    // `turn_latency` uses, so the two runs are directly comparable.
    let baseline = costs[0].2.max(0.001);
    let early = costs[1].2;
    let late = costs.last().expect("turns").2;
    println!(
        "\ncost per 1k sent: turn 0 {:.0} (uncached) | turn 1 {:.0} ({:.0}%) | turn {} {:.0} ({:.0}%)",
        baseline,
        early,
        early / baseline * 100.0,
        costs.len() - 1,
        late,
        late / baseline * 100.0,
    );

    // Anything at or above 60% of the uncached rate is not meaningfully
    // cached: the remaining gap is batching, not reuse.
    let reused_at_all = early < baseline * 0.6;
    let still_reused = late < baseline * 0.6;

    println!(
        "\n{}",
        match (reused_at_all, still_reused) {
            (true, true) =>
                "REUSED, AND IT HOLDS. The per-token cost stayed well under the uncached \
                 rate as the transcript grew, so this server honours the engine's shape. A \
                 live turn that does not get this reuse is failing below the message list \
                 — look at how the prompt renders, not at the ordering.",
            (true, false) =>
                "REUSED AT FIRST, THEN DECAYED. The cost climbed back towards the uncached \
                 rate as the conversation grew, so the server keeps a fixed head and \
                 re-evaluates what follows it. This is the shape Phase 5.6 measured live, \
                 and it is the server's behaviour rather than the engine's prompt.",
            (false, _) =>
                "NOT REUSED IN THIS SHAPE. The per-token cost never left the uncached rate, \
                 even though the fixed-prefix probe above reuses fine. The difference is the \
                 moving tail behind the transcript: ordering cannot fix that, and the engine \
                 has to stop putting blocks behind the history.",
        }
    );
}
