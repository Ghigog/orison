//! Cancellation, verified rather than asserted (§4.1 exit criterion).
//!
//! The handoff is specific about the bar: "assert the request actually
//! stopped, not that a flag was set". A flag is exactly what the Godot build
//! has — `LLMStreamRequest.cancel()` sets `_cancelled = true` and discards
//! whatever arrives later, so a local model keeps generating on the GPU for a
//! response nobody will read ([orison_audit.md §16]).
//!
//! The only place "the request actually stopped" is observable is the
//! server's side of the socket, so these tests bring their own server:
//! `tests/support` binds a loopback port, speaks enough HTTP for
//! `OllamaBackend`, and records whether the client went away mid-response.
//!
//! [orison_audit.md §16]: ../../docs/orison_audit.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use orison_core::inference::{ChatMessage, ChatRequest, InferenceBackend, OllamaBackend};
use orison_core::turn::{CancelReason, Priority, RequestQueue, TurnError};
use support::{character_response_json, test_tokenizer, FakeOllama};

use futures::StreamExt;

async fn backend(server: &FakeOllama) -> Arc<dyn InferenceBackend> {
    Arc::new(
        OllamaBackend::connect(server.url(), "stand-in", test_tokenizer())
            .await
            .expect("connect to the stand-in"),
    )
}

/// The whole point of the phase, in one assertion: the server sees the
/// connection close while it still had response left to send.
#[tokio::test]
async fn cancelling_a_streamed_request_closes_the_connection_to_the_model() {
    let server = FakeOllama::start(
        character_response_json("The ledger-keeper looks up.", "Ask me tomorrow."),
        40,
        Duration::from_millis(20),
    )
    .await;
    let backend = backend(&server).await;
    let queue = RequestQueue::spawn();

    let ticket = queue.submit("actor", Priority::High, move |cancel| async move {
        let mut stream = backend
            .chat_stream(ChatRequest::new(vec![ChatMessage::user("hello")]))
            .await?;
        loop {
            tokio::select! {
                biased;
                reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
                item = stream.next() => match item {
                    Some(Ok(delta)) if delta.done => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                },
            }
        }
        Ok::<(), TurnError>(())
    });

    // Let a few chunks arrive, so this is a cancellation mid-response rather
    // than one that raced the request.
    while server.observed.chunks_sent() < 3 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    ticket.cancel(CancelReason::PlayerActedAgain);

    match ticket.join().await {
        Err(TurnError::Cancelled(CancelReason::PlayerActedAgain)) => {}
        other => panic!("expected a typed cancellation, got {other:?}"),
    }

    // Give the server a moment to notice; it notices on its next chunk.
    for _ in 0..100 {
        if server.observed.disconnected() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(
        server.observed.disconnected(),
        "the model's endpoint never saw the client leave, so generation would continue"
    );
    assert!(
        server.observed.chunks_sent() < server.observed.chunks_scripted(),
        "the response finished anyway: {} of {} chunks sent",
        server.observed.chunks_sent(),
        server.observed.chunks_scripted(),
    );
}

/// The same thing through the queue's preemption path: a player turn arriving
/// while the Director is composing stops the Director's request, rather than
/// letting it run to completion and discarding the answer.
#[tokio::test]
async fn a_preempted_background_request_stops_at_the_socket() {
    let server = FakeOllama::start(
        character_response_json("The room stills.", "Later."),
        40,
        Duration::from_millis(20),
    )
    .await;
    let backend = backend(&server).await;
    let queue = RequestQueue::spawn();

    let director_backend = Arc::clone(&backend);
    let director = queue.submit("director-beat", Priority::Low, move |cancel| async move {
        let mut stream = director_backend
            .chat_stream(ChatRequest::new(vec![ChatMessage::user("compose a beat")]))
            .await?;
        loop {
            tokio::select! {
                biased;
                reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
                item = stream.next() => if item.is_none() { break },
            }
        }
        Ok::<(), TurnError>(())
    });

    while server.observed.chunks_sent() < 3 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let turn = queue.submit("player-turn", Priority::High, |_| async {
        Ok::<(), TurnError>(())
    });

    match director.join().await {
        Err(TurnError::Cancelled(CancelReason::Preempted)) => {}
        other => panic!("expected preemption, got {other:?}"),
    }
    turn.join().await.expect("the player's turn runs");

    for _ in 0..100 {
        if server.observed.disconnected() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        server.observed.disconnected(),
        "preemption did not reach the socket"
    );
}

/// A request nobody cancels must run to completion. The control case: without
/// it, a server that always reports a disconnect would pass the tests above.
#[tokio::test]
async fn an_uncancelled_request_runs_to_completion() {
    let server = FakeOllama::start(
        character_response_json("Nothing happens.", "Quite."),
        8,
        Duration::from_millis(1),
    )
    .await;
    let backend = backend(&server).await;

    let mut stream = backend
        .chat_stream(ChatRequest::new(vec![ChatMessage::user("hello")]))
        .await
        .expect("stream starts");
    let mut text = String::new();
    while let Some(delta) = stream.next().await {
        if let Some(content) = delta.expect("no stream error").content {
            text.push_str(&content);
        }
    }

    assert_eq!(
        server.observed.chunks_sent(),
        server.observed.chunks_scripted()
    );
    assert!(!server.observed.disconnected());
    assert!(text.contains("Nothing happens."), "got {text:?}");
}
