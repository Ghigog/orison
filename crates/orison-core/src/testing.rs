//! A stand-in Ollama, and the response bodies a turn expects.
//!
//! **Test scaffolding that ships in the crate, on purpose.** It is behind the
//! `test-support` feature, off by default, so nothing here reaches a release
//! build. It lives here rather than in `tests/` because `orison-cli` needs the
//! same stand-in, and two stand-ins is how a stand-in starts flattering the
//! code: this repository's own lesson from Phase 4 is that a test passing
//! against a stand-in is evidence about the stand-in until a live run says
//! otherwise. One stand-in, told to lie the same way to everybody.
//!
//! Hermetic by construction: it binds `127.0.0.1:0` and serves the two
//! endpoints `OllamaBackend` uses. It exists because the Phase 4 exit bar for
//! cancellation is "assert the request actually stopped, not that a flag was
//! set" — and the only place that is observable is the server's side of the
//! socket. A mock trait implementation could not prove it; this can.

#![allow(dead_code)]

use std::io;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use std::collections::HashMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::prompt::schemas::{
    CharacterResponse, Choice, ChoiceType, DiceRoll, DirectorResponse, Emotion, EmotionalUpdate,
    EscalationSignal, MemoryUpdates,
};

/// A tokenizer that needs no model files. Token counts are word counts, which
/// is enough for budgeting assertions and is emphatically not `length / 4`.
pub fn test_tokenizer() -> tokenizers::Tokenizer {
    tokenizers::Tokenizer::from_str(
        r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0},"unk_token":"[UNK]"}}"#,
    )
    .expect("minimal tokenizer json is valid")
}

/// What the stand-in observed, readable from the test while it serves.
#[derive(Debug, Default)]
pub struct Observed {
    /// Streaming chunks written to the socket before the connection ended.
    pub chunks_sent: AtomicUsize,
    /// Chunks the script wanted to send.
    pub chunks_scripted: AtomicUsize,
    /// The client closed the connection before the response finished. This
    /// is the assertion the cancellation test exists for: a local model
    /// serving this request would stop generating here.
    pub client_disconnected: AtomicBool,
    /// `/api/chat` requests received.
    pub chat_requests: AtomicUsize,
    /// The body of the most recent `/api/chat` request.
    pub last_chat_body: std::sync::Mutex<String>,
    /// Every `/api/chat` body, in order. What the cache-stability assertion
    /// compares.
    pub chat_bodies: std::sync::Mutex<Vec<String>>,
    /// The rendered messages of the previous `/api/chat` request, so the
    /// stand-in can model a prompt cache instead of pretending there is none.
    /// See [`evaluated_prompt_tokens`].
    previous_messages: std::sync::Mutex<Vec<String>>,
}

impl Observed {
    pub fn chunks_sent(&self) -> usize {
        self.chunks_sent.load(Ordering::SeqCst)
    }
    pub fn chunks_scripted(&self) -> usize {
        self.chunks_scripted.load(Ordering::SeqCst)
    }
    pub fn disconnected(&self) -> bool {
        self.client_disconnected.load(Ordering::SeqCst)
    }
    pub fn chat_requests(&self) -> usize {
        self.chat_requests.load(Ordering::SeqCst)
    }
    pub fn last_chat_body(&self) -> String {
        self.last_chat_body
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn chat_bodies(&self) -> Vec<String> {
        self.chat_bodies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// What a prompt-caching server would have to evaluate for this request,
/// given what it was sent last time.
///
/// Ollama reports exactly this as `prompt_eval_count`: tokens served from its
/// own cache are not counted. Modelling it here rather than reporting a flat
/// zero is what lets `tests/turn_latency.rs`'s harness self-test fail when
/// the prompt ordering regresses — the previous stand-in could not tell a
/// cache-stable prompt from a cache-hostile one, which is precisely how a
/// green wire test coexisted with a live run at 2x the Godot baseline.
///
/// Token counts are word counts, matching [`test_tokenizer`].
fn evaluated_prompt_tokens(previous: &[String], current: &[String]) -> usize {
    let shared = previous
        .iter()
        .zip(current)
        .take_while(|(a, b)| a == b)
        .count();
    current[shared..]
        .iter()
        .map(|m| m.split_whitespace().count())
        .sum()
}

/// The `role:content` rendering of a request's messages, for prefix
/// comparison. An unparseable body yields no messages, which reads as "no
/// cache" rather than as a panic inside the stand-in.
fn messages_of(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    value["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .map(|m| {
                    format!(
                        "{}:{}",
                        m["role"].as_str().unwrap_or(""),
                        m["content"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A scripted Ollama endpoint.
pub struct FakeOllama {
    addr: SocketAddr,
    pub observed: Arc<Observed>,
}

impl FakeOllama {
    /// Serve `content` as a stream, split into `chunks` pieces with `gap`
    /// between them. A long gap is what gives a test room to cancel
    /// mid-response.
    pub async fn start(content: String, chunks: usize, gap: Duration) -> Self {
        Self::scripted(vec![content], chunks, gap).await
    }

    /// How many chunks the current response was split into. Only meaningful
    /// once a request has been served.
    pub fn chunks_for(&self, content: &str, chunks: usize) -> usize {
        split_into(content, chunks).len()
    }

    /// Serve a different response to each `/api/chat` request, in order. The
    /// last one repeats once the script runs out, so a test only has to
    /// script the requests it cares about.
    pub async fn scripted(responses: Vec<String>, chunks: usize, gap: Duration) -> Self {
        let responses = Arc::new(responses);
        Self::responding(
            move |_body, nth| responses[nth.min(responses.len() - 1)].clone(),
            chunks,
            gap,
        )
        .await
    }

    /// Choose the response by the schema the request asked for.
    ///
    /// The §4.2 arms make two kinds of call, and on the two-call arms the
    /// Director's runs in the background — so which request arrives second is
    /// a race. Answering by schema rather than by arrival order removes the
    /// race from the test instead of papering over it with a sleep.
    pub async fn by_schema(
        character: String,
        director: String,
        combined: String,
        chunks: usize,
        gap: Duration,
    ) -> Self {
        Self::responding(
            move |body, _nth| {
                // The request carries the JSON Schema generated from the type
                // the caller will deserialise into, so its property names say
                // which response is wanted.
                let asks_character = body.contains("thinking");
                let asks_director = body.contains("dice_roll");
                match (asks_character, asks_director) {
                    (true, true) => combined.clone(),
                    (true, false) => character.clone(),
                    _ => director.clone(),
                }
            },
            chunks,
            gap,
        )
        .await
    }

    async fn responding(
        responder: impl Fn(&str, usize) -> String + Send + Sync + 'static,
        chunks: usize,
        gap: Duration,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a loopback port");
        let addr = listener.local_addr().expect("local addr");
        let observed = Arc::new(Observed::default());
        let responder: Responder = Arc::new(responder);

        let serving = Arc::clone(&observed);
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                let observed = Arc::clone(&serving);
                let responder = Arc::clone(&responder);
                tokio::spawn(async move {
                    let _ = serve(socket, responder, chunks, gap, observed).await;
                });
            }
        });

        Self { addr, observed }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// The context window `FakeOllama` claims from `/api/show`, matching what
/// `llama3.2:3b` really advertises. Deliberately far above
/// `inference::DEFAULT_CONTEXT_LIMIT` so tests can tell the two apart.
pub const ADVERTISED_CONTEXT_LENGTH: usize = 131_072;

type Responder = Arc<dyn Fn(&str, usize) -> String + Send + Sync>;

async fn serve(
    socket: tokio::net::TcpStream,
    responder: Responder,
    chunks: usize,
    gap: Duration,
    observed: Arc<Observed>,
) -> io::Result<()> {
    let (mut reader, mut writer) = socket.into_split();

    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    // Read until the headers are complete, then until the declared body is.
    let (path, body) = loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        raw.extend_from_slice(&buf[..n]);
        let Some(head_end) = find_subslice(&raw, b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
        let path = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string();
        let length: usize = head
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().to_string())
            })
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if raw.len() >= head_end + 4 + length {
            let body =
                String::from_utf8_lossy(&raw[head_end + 4..head_end + 4 + length]).to_string();
            break (path, body);
        }
    };

    if path.ends_with("/api/show") {
        // What a real small model advertises, not what we would like it to.
        // `llama3.2:3b` — the model the baseline was recorded on — reports
        // 131072 here, and the stand-in used to report 8192, which is exactly
        // the number the backend caps at. A stand-in that agrees with the
        // code by coincidence cannot catch the code being wrong: Phase 4's
        // live turn latency was 2x the baseline partly because this figure
        // was passed straight through as `num_ctx`, and nothing in
        // `cargo test` could see it.
        let payload = serde_json::json!({
            "model_info": { "general.context_length": ADVERTISED_CONTEXT_LENGTH },
            "capabilities": ["tools", "completion"],
        })
        .to_string();
        writer
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{payload}",
                    payload.len()
                )
                .as_bytes(),
            )
            .await?;
        return Ok(());
    }

    if !path.ends_with("/api/chat") {
        writer
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    }

    let nth = observed.chat_requests.fetch_add(1, Ordering::SeqCst);
    let streaming = body.contains("\"stream\":true");
    *observed
        .last_chat_body
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = body;
    observed
        .chat_bodies
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(
            observed
                .last_chat_body
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        );

    // Model the server's prompt cache before answering: what it would have to
    // evaluate is whatever this request does not share as a prefix with the
    // last one.
    let evaluated = {
        let current = messages_of(
            &observed
                .last_chat_body
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        );
        let mut previous = observed
            .previous_messages
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let evaluated = evaluated_prompt_tokens(&previous, &current);
        *previous = current;
        evaluated
    };

    let pieces = split_into(
        &responder(
            &observed
                .last_chat_body
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            nth,
        ),
        chunks,
    );
    observed
        .chunks_scripted
        .store(pieces.len(), Ordering::SeqCst);

    if !streaming {
        // A non-streamed `/api/chat` answers with one JSON object, so it
        // needs a Content-Length like any ordinary response. The Director
        // uses this path.
        let payload = serde_json::json!({
            "model": "stand-in",
            "message": { "role": "assistant", "content": pieces.concat() },
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": evaluated,
            "eval_count": 0,
        })
        .to_string();
        writer
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{payload}",
                    payload.len()
                )
                .as_bytes(),
            )
            .await?;
        observed.chunks_sent.fetch_add(1, Ordering::SeqCst);
        return Ok(());
    }

    // No Content-Length: the body ends when the connection closes, which is
    // how Ollama's own NDJSON stream behaves.
    writer
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n",
        )
        .await?;

    let last = pieces.len().saturating_sub(1);
    for (i, piece) in pieces.iter().enumerate() {
        // Wait, watching the read half. A closed connection surfaces here as
        // a zero-length read — immediately, and long before a write would
        // fail into a socket buffer.
        let mut discard = [0u8; 256];
        tokio::select! {
            read = reader.read(&mut discard) => {
                if matches!(read, Ok(0) | Err(_)) {
                    observed.client_disconnected.store(true, Ordering::SeqCst);
                    return Ok(());
                }
            }
            _ = tokio::time::sleep(gap) => {}
        }

        // Ollama puts its accounting on the final chunk only, so a consumer
        // that reads it from the first chunk would work here and fail live.
        let mut line_value = serde_json::json!({
            "model": "stand-in",
            "message": { "role": "assistant", "content": piece },
            "done": i == last,
            "done_reason": if i == last { "stop" } else { "" },
        });
        if i == last {
            line_value["prompt_eval_count"] = serde_json::json!(evaluated);
            line_value["eval_count"] = serde_json::json!(0);
        }
        let line = line_value.to_string();
        if writer
            .write_all(format!("{line}\n").as_bytes())
            .await
            .is_err()
        {
            observed.client_disconnected.store(true, Ordering::SeqCst);
            return Ok(());
        }
        observed.chunks_sent.fetch_add(1, Ordering::SeqCst);
    }
    Ok(())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Split on character boundaries into at most `count` pieces.
fn split_into(text: &str, count: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if count == 0 || chars.is_empty() {
        return vec![text.to_string()];
    }
    let per = chars.len().div_ceil(count);
    chars
        .chunks(per)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

/// A well-formed Actor response, as the schema-constrained decoder would
/// produce it.
pub fn character_response_json(narration: &str, dialogue: &str) -> String {
    serialise(&CharacterResponse {
        thinking: "The player addressed me directly. A reply is the logical next step.".into(),
        narration: narration.into(),
        dialogue: dialogue.into(),
        emotional_update: EmotionalUpdate {
            emotion: Emotion::Trust,
            intensity: 0.6,
            reason: "They kept their word about the ledger.".into(),
            rapport_delta: 0.1,
        },
        escalation_signal: EscalationSignal::None,
    })
}

/// Serialise a response type rather than hand-writing its JSON.
///
/// **Field order is the reason, and it is not cosmetic.** `serde_json::json!`
/// builds a `Value`, whose object is a `BTreeMap` unless `preserve_order` is
/// on, so a hand-written literal comes back out *alphabetically*: dialogue
/// before narration. A backend doing constrained decoding emits the schema's
/// order, which is the struct's order — narration before dialogue — and
/// `FieldStreamer` forwards whichever it meets first. A stand-in built from a
/// literal therefore streams the character's answer before the narration that
/// sets it up, and every shell rendering the stream shows it that way, while
/// the same shell against a real model shows it correctly.
///
/// Deriving the body from the type it will be parsed back into keeps the
/// stand-in honest about the one thing it is standing in for.
fn serialise<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("a response type serialises")
}

/// A well-formed combined response, for the §4.2 single-call arm.
pub fn combined_response_json(narration: &str, dialogue: &str) -> String {
    let mut character: serde_json::Value =
        serde_json::from_str(&character_response_json(narration, dialogue)).unwrap();
    let director: serde_json::Value = serde_json::from_str(&director_response_json("")).unwrap();
    let object = character.as_object_mut().unwrap();
    for key in [
        "memory_updates",
        "plot_updates",
        "inventory_updates",
        "choices",
        "dice_roll",
    ] {
        object.insert(key.to_string(), director[key].clone());
    }
    serde_json::to_string(&character).unwrap()
}

pub fn director_response_json(narration: &str) -> String {
    serialise(&DirectorResponse {
        narration: narration.into(),
        memory_updates: MemoryUpdates {
            short_term: "They are in the counting house.".into(),
            medium_term: "The ledger has to reach the chapel before dusk.".into(),
            long_term: "The accord is unravelling.".into(),
        },
        plot_updates: HashMap::from([("ledger_found".to_string(), true)]),
        inventory_updates: Vec::new(),
        choices: vec![Choice {
            text: "Take the ledger".into(),
            kind: ChoiceType::Do,
        }],
        dice_roll: DiceRoll {
            required: false,
            ability: None,
            dc: None,
            reason: None,
        },
    })
}

// --------------------------------------------------------------- fixtures

/// The fixture vault directory, resolved from this crate rather than from the
/// caller's, so `orison-cli`'s tests find the same three vaults.
pub fn fixture_root(fixture: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(fixture)
}

/// A fixture's scripted transcript, from its `ground_truth.json`.
///
/// One loader, because the alternative is each harness deciding for itself
/// which character the script addresses — and `messy`'s answer to that is the
/// whole point of the fixture. Lord Anneke is the pronoun trap: no gender
/// field, an unmistakably masculine body, and a name with a strong feminine
/// prior. A harness that picked its own character would skip the regression
/// the fixture exists for.
pub fn fixture_script(fixture: &str) -> crate::turn::TranscriptScript {
    let path = fixture_root(fixture).join("ground_truth.json");
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let ground_truth: serde_json::Value =
        serde_json::from_str(&raw).expect("ground_truth.json parses");
    let strings = |key: &str| -> Vec<String> {
        ground_truth[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    crate::turn::TranscriptScript {
        character: ground_truth["transcript_character"]
            .as_str()
            .unwrap_or_else(|| panic!("{fixture} defines no transcript character"))
            .to_string(),
        forbidden_pronouns: strings("transcript_forbidden_pronouns"),
        lines: strings("transcript_script"),
    }
}
