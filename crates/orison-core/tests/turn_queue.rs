//! The request queue's guarantees (§4.1).
//!
//! `test_llm_request_queue` in the Godot suite is the specification the
//! handoff points at: three requests submitted, the first executing, the other
//! two queued in order. It asserts that by reaching into
//! `LLMClient._request_queue` and `_queue_processing` and then clearing both
//! so no HTTP call is ever made — so it checks the bookkeeping and not the
//! behaviour.
//!
//! These run the jobs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orison_core::turn::{CancelReason, Priority, RequestQueue, TurnError};
use tokio::sync::oneshot;

/// A job that reports when it starts and finishes, and that can be held open.
fn recorder(
    log: Arc<Mutex<Vec<String>>>,
    name: &'static str,
    hold: Duration,
) -> impl FnOnce(
    orison_core::turn::CancelToken,
) -> futures::future::BoxFuture<'static, Result<(), TurnError>>
       + Send
       + 'static {
    move |_cancel| {
        Box::pin(async move {
            log.lock().unwrap().push(format!("start:{name}"));
            tokio::time::sleep(hold).await;
            log.lock().unwrap().push(format!("end:{name}"));
            Ok(())
        })
    }
}

/// The Godot spec, run rather than inspected: first executing, rest queued in
/// arrival order.
#[tokio::test]
async fn three_submissions_run_one_at_a_time_in_arrival_order() {
    let queue = RequestQueue::spawn();
    let log = Arc::new(Mutex::new(Vec::new()));
    let (started_tx, started_rx) = oneshot::channel();
    let started_tx = Arc::new(Mutex::new(Some(started_tx)));

    let first_log = Arc::clone(&log);
    let signal = Arc::clone(&started_tx);
    let first = queue.submit("prompt-1", Priority::Low, move |_cancel| async move {
        first_log.lock().unwrap().push("start:prompt-1".to_string());
        if let Some(tx) = signal.lock().unwrap().take() {
            let _ = tx.send(());
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
        first_log.lock().unwrap().push("end:prompt-1".to_string());
        Ok::<(), TurnError>(())
    });
    let second = queue.submit(
        "prompt-2",
        Priority::Low,
        recorder(Arc::clone(&log), "prompt-2", Duration::from_millis(10)),
    );
    let third = queue.submit(
        "prompt-3",
        Priority::Low,
        recorder(Arc::clone(&log), "prompt-3", Duration::from_millis(10)),
    );

    // Wait for the first to actually be running, rather than for a flag.
    started_rx.await.expect("first job started");
    assert_eq!(queue.active_label().as_deref(), Some("prompt-1"));
    assert_eq!(queue.depth(), 2, "two requests should be waiting");
    assert_eq!(queue.pending_labels(), vec!["prompt-2", "prompt-3"]);

    first.join().await.expect("first completes");
    second.join().await.expect("second completes");
    third.join().await.expect("third completes");

    // Sequential execution means no interleaving: every start is followed by
    // its own end. `LLMClient` promises this and nothing there proves it.
    let entries = log.lock().unwrap().clone();
    assert_eq!(
        entries,
        vec![
            "start:prompt-1",
            "end:prompt-1",
            "start:prompt-2",
            "end:prompt-2",
            "start:prompt-3",
            "end:prompt-3",
        ]
    );
}

/// `LLMClient._enqueue_request`: a HIGH request goes behind waiting HIGHs and
/// ahead of every LOW.
#[tokio::test]
async fn high_priority_jumps_the_queue_but_not_other_high_priority_work() {
    let queue = RequestQueue::spawn();
    let (release_tx, release_rx) = oneshot::channel::<()>();

    // Occupy the worker so nothing else starts while we arrange the queue.
    let blocker = queue.submit("blocker", Priority::High, move |_cancel| async move {
        let _ = release_rx.await;
        Ok::<(), TurnError>(())
    });
    while queue.active_label().is_none() {
        tokio::task::yield_now().await;
    }

    let low_a = queue.submit("low-a", Priority::Low, |_| async {
        Ok::<(), TurnError>(())
    });
    let high_a = queue.submit("high-a", Priority::High, |_| async {
        Ok::<(), TurnError>(())
    });
    let low_b = queue.submit("low-b", Priority::Low, |_| async {
        Ok::<(), TurnError>(())
    });
    let high_b = queue.submit("high-b", Priority::High, |_| async {
        Ok::<(), TurnError>(())
    });

    assert_eq!(
        queue.pending_labels(),
        vec!["high-a", "high-b", "low-a", "low-b"],
    );

    let _ = release_tx.send(());
    for ticket in [blocker, low_a, high_a, low_b, high_b] {
        ticket.join().await.expect("everything completes");
    }
}

/// A player turn does not wait behind a background Director call.
///
/// The Godot build cancels the running LOW request and reports it to the
/// caller as the string `"Request cancelled (preempted by higher priority
/// request)"`. Here the caller gets a typed reason.
#[tokio::test]
async fn a_high_priority_submission_preempts_a_running_low_priority_one() {
    let queue = RequestQueue::spawn();
    let low_finished = Arc::new(AtomicUsize::new(0));

    let counter = Arc::clone(&low_finished);
    let background = queue.submit("director-beat", Priority::Low, move |_cancel| async move {
        // Long enough that only cancellation can end it inside this test.
        tokio::time::sleep(Duration::from_secs(30)).await;
        counter.fetch_add(1, Ordering::SeqCst);
        Ok::<(), TurnError>(())
    });
    while queue.active_label().as_deref() != Some("director-beat") {
        tokio::task::yield_now().await;
    }

    let turn = queue.submit("player-turn", Priority::High, |_| async {
        Ok::<&'static str, TurnError>("said something")
    });

    match background.join().await {
        Err(TurnError::Cancelled(CancelReason::Preempted)) => {}
        other => panic!("expected a typed preemption, got {other:?}"),
    }
    assert_eq!(
        low_finished.load(Ordering::SeqCst),
        0,
        "the preempted job must not have run to completion"
    );
    assert_eq!(turn.join().await.unwrap(), "said something");
}

/// A request cancelled while it is still waiting must never issue at all.
#[tokio::test]
async fn a_queued_request_cancelled_before_it_starts_never_runs() {
    let queue = RequestQueue::spawn();
    let ran = Arc::new(AtomicUsize::new(0));
    let (release_tx, release_rx) = oneshot::channel::<()>();

    let blocker = queue.submit("blocker", Priority::Low, move |_cancel| async move {
        let _ = release_rx.await;
        Ok::<(), TurnError>(())
    });
    while queue.active_label().is_none() {
        tokio::task::yield_now().await;
    }

    let counter = Arc::clone(&ran);
    let doomed = queue.submit("doomed", Priority::Low, move |_cancel| async move {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok::<(), TurnError>(())
    });
    doomed.cancel(CancelReason::Requested);
    let _ = release_tx.send(());

    match doomed.join().await {
        Err(TurnError::Cancelled(CancelReason::Requested)) => {}
        other => panic!("expected a cancelled request, got {other:?}"),
    }
    blocker.join().await.expect("blocker completes");
    assert_eq!(
        ran.load(Ordering::SeqCst),
        0,
        "a cancelled request must not have executed"
    );
}

/// `LLMClient.cancel()` clears the queue and leaves every dropped caller with
/// no notification. Every caller is answered here.
#[tokio::test]
async fn cancel_all_answers_every_waiting_caller() {
    let queue = RequestQueue::spawn();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let blocker = queue.submit("blocker", Priority::Low, move |_cancel| async move {
        let _ = release_rx.await;
        Ok::<(), TurnError>(())
    });
    while queue.active_label().is_none() {
        tokio::task::yield_now().await;
    }
    let waiting: Vec<_> = (0..3)
        .map(|i| {
            queue.submit(format!("queued-{i}"), Priority::Low, |_| async {
                Ok::<(), TurnError>(())
            })
        })
        .collect();

    queue.cancel_all(CancelReason::Shutdown);
    let _ = release_tx.send(());

    for ticket in waiting {
        match ticket.join().await {
            Err(TurnError::Cancelled(CancelReason::Shutdown)) => {}
            other => panic!("every waiting caller must be told, got {other:?}"),
        }
    }
    let _ = blocker.join().await;
}
