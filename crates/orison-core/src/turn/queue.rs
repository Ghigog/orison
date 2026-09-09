//! The request queue (§4.1).
//!
//! `LLMClient.gd`'s queue is the specification, and `test_llm_request_queue`
//! in the Godot suite is the part of it that is written down: requests execute
//! one at a time, in order, and a HIGH-priority request jumps ahead of queued
//! LOW ones and cancels a running one.
//!
//! What changes is what "cancel" means. The Godot build cancels by freeing an
//! `HTTPRequest` node and firing a callback with the string
//! `"Request cancelled (preempted by higher priority request)"`; a *streaming*
//! request it cannot stop at all, so `LLMStreamRequest.cancel()` sets a flag
//! and discards whatever eventually arrives ([orison_audit.md §16]). The model
//! keeps generating on a local GPU for output nobody will read.
//!
//! Here a running job is a future. Cancelling drops it, which drops the
//! response body, which closes the connection, which stops the model.
//! `tests/turn_cancellation.rs` asserts that from the server's side rather
//! than trusting the flag.
//!
//! [orison_audit.md §16]: ../../../../docs/orison_audit.md

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use futures::future::BoxFuture;
use tokio::sync::{oneshot, watch, Notify};

use super::error::{CancelReason, TurnError};

/// Execution priority. A player turn is `High`; background work — the
/// Director, memory summarisation, base-emotion deduction — is `Low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    High,
}

/// A cancellation signal shared by everyone who can stop a request and
/// everyone who has to notice.
///
/// Cloning shares the signal. The first `cancel` wins: a job cancelled because
/// the player acted again does not later report itself as preempted.
#[derive(Debug, Clone)]
pub struct CancelToken {
    tx: Arc<watch::Sender<Option<CancelReason>>>,
    rx: watch::Receiver<Option<CancelReason>>,
}

impl CancelToken {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(None);
        Self {
            tx: Arc::new(tx),
            rx,
        }
    }

    /// Signal cancellation. Idempotent; the first reason is kept.
    pub fn cancel(&self, reason: CancelReason) {
        self.tx.send_if_modified(|slot| {
            if slot.is_none() {
                *slot = Some(reason);
                true
            } else {
                false
            }
        });
    }

    pub fn reason(&self) -> Option<CancelReason> {
        *self.rx.borrow()
    }

    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    /// Resolves when cancelled, and never otherwise.
    ///
    /// The sender is held inside the token itself, so this cannot resolve
    /// spuriously because the last sender was dropped — a subtlety worth
    /// stating, since "the channel closed" resolving as "cancelled" would
    /// abort healthy requests.
    pub async fn cancelled(&self) -> CancelReason {
        let mut rx = self.rx.clone();
        loop {
            if let Some(reason) = *rx.borrow_and_update() {
                return reason;
            }
            if rx.changed().await.is_err() {
                // Unreachable while `self.tx` is alive, which it is: `self`
                // owns an `Arc` of it.
                std::future::pending::<()>().await;
            }
        }
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to one submitted request.
///
/// Dropping a ticket does *not* cancel the request: background work is
/// submitted and forgotten on purpose. Cancel explicitly.
pub struct Ticket<T> {
    rx: oneshot::Receiver<Result<T, TurnError>>,
    cancel: CancelToken,
}

impl<T> Ticket<T> {
    /// Wait for the request to finish.
    pub async fn join(self) -> Result<T, TurnError> {
        match self.rx.await {
            Ok(result) => result,
            // The worker dropped the sender without sending, which happens
            // only if the queue was closed mid-flight.
            Err(_) => Err(TurnError::Cancelled(CancelReason::Shutdown)),
        }
    }

    pub fn cancel(&self, reason: CancelReason) {
        self.cancel.cancel(reason);
    }

    pub fn token(&self) -> CancelToken {
        self.cancel.clone()
    }
}

type BoxedJob = Box<dyn FnOnce(CancelToken) -> BoxFuture<'static, ()> + Send>;

struct Entry {
    label: String,
    priority: Priority,
    cancel: CancelToken,
    job: BoxedJob,
}

#[derive(Clone)]
struct Active {
    label: String,
    priority: Priority,
    cancel: CancelToken,
}

struct Shared {
    pending: Mutex<VecDeque<Entry>>,
    active: Mutex<Option<Active>>,
    /// Woken on submit and on close. `Notify::notify_one` stores a permit, so
    /// a submit that lands between the worker's "queue is empty" check and its
    /// await is not lost.
    wake: Notify,
    closed: AtomicBool,
    completed: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panicking job poisons nothing the queue depends on: the deque and the
    // active slot are plain data. Recovering is strictly better than turning
    // one bad turn into a dead engine.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A queue that runs one request at a time, in priority-then-arrival order.
pub struct RequestQueue {
    shared: Arc<Shared>,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl RequestQueue {
    /// Start a queue and its worker task.
    pub fn spawn() -> Arc<Self> {
        let shared = Arc::new(Shared {
            pending: Mutex::new(VecDeque::new()),
            active: Mutex::new(None),
            wake: Notify::new(),
            closed: AtomicBool::new(false),
            completed: AtomicU64::new(0),
        });
        let worker = tokio::spawn(run_worker(Arc::clone(&shared)));
        Arc::new(Self {
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }

    /// Submit a request.
    ///
    /// `f` receives the same [`CancelToken`] the queue will use, so a job that
    /// has cooperative checkpoints of its own (a multi-step research loop, for
    /// instance) can stop between steps rather than only at an await.
    pub fn submit<F, Fut, T>(&self, label: impl Into<String>, priority: Priority, f: F) -> Ticket<T>
    where
        F: FnOnce(CancelToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, TurnError>> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let cancel = CancelToken::new();

        if self.shared.closed.load(Ordering::SeqCst) {
            let _ = tx.send(Err(TurnError::QueueClosed));
            return Ticket { rx, cancel };
        }

        let job: BoxedJob = Box::new(move |token: CancelToken| {
            Box::pin(async move {
                let outcome = tokio::select! {
                    // Biased so that a job cancelled while it was still queued
                    // never issues its request at all.
                    biased;
                    reason = token.cancelled() => Err(TurnError::Cancelled(reason)),
                    result = f(token.clone()) => result,
                };
                let _ = tx.send(outcome);
            })
        });

        let entry = Entry {
            label: label.into(),
            priority,
            cancel: cancel.clone(),
            job,
        };

        {
            let mut pending = lock(&self.shared.pending);
            match priority {
                // Behind any HIGH already waiting, ahead of every LOW. Ported
                // from `LLMClient._enqueue_request`.
                Priority::High => {
                    let at = pending
                        .iter()
                        .position(|e| e.priority != Priority::High)
                        .unwrap_or(pending.len());
                    pending.insert(at, entry);
                }
                Priority::Low => pending.push_back(entry),
            }
        }

        // A player turn does not wait behind a background Director call.
        if priority == Priority::High {
            let running = lock(&self.shared.active).clone();
            if let Some(active) = running {
                if active.priority == Priority::Low {
                    active.cancel.cancel(CancelReason::Preempted);
                }
            }
        }

        self.shared.wake.notify_one();
        Ticket { rx, cancel }
    }

    /// Requests waiting to start. Excludes the one running.
    pub fn depth(&self) -> usize {
        lock(&self.shared.pending).len()
    }

    /// Labels of waiting requests, in the order they will run.
    pub fn pending_labels(&self) -> Vec<String> {
        lock(&self.shared.pending)
            .iter()
            .map(|e| e.label.clone())
            .collect()
    }

    pub fn active_label(&self) -> Option<String> {
        lock(&self.shared.active).as_ref().map(|a| a.label.clone())
    }

    pub fn is_busy(&self) -> bool {
        lock(&self.shared.active).is_some()
    }

    /// How many requests have run to completion, cancelled ones included.
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::SeqCst)
    }

    /// Cancel everything: the running request and every queued one.
    ///
    /// The direct replacement for `LLMClient.cancel()`, which cleared the
    /// queue and left each dropped caller with no notification at all.
    pub fn cancel_all(&self, reason: CancelReason) {
        if let Some(active) = lock(&self.shared.active).as_ref() {
            active.cancel.cancel(reason);
        }
        for entry in lock(&self.shared.pending).iter() {
            entry.cancel.cancel(reason);
        }
        self.shared.wake.notify_one();
    }

    /// Stop accepting work and cancel what is in flight. Queued jobs are still
    /// drained, so every caller gets its `Cancelled` answer rather than a
    /// dropped sender.
    pub fn close(&self) {
        self.shared.closed.store(true, Ordering::SeqCst);
        self.cancel_all(CancelReason::Shutdown);
        self.shared.wake.notify_one();
    }
}

impl Drop for RequestQueue {
    fn drop(&mut self) {
        self.close();
        if let Some(handle) = lock(&self.worker).take() {
            handle.abort();
        }
    }
}

async fn run_worker(shared: Arc<Shared>) {
    loop {
        let next = lock(&shared.pending).pop_front();
        match next {
            Some(entry) => {
                *lock(&shared.active) = Some(Active {
                    label: entry.label.clone(),
                    priority: entry.priority,
                    cancel: entry.cancel.clone(),
                });
                let token = entry.cancel.clone();
                (entry.job)(token).await;
                *lock(&shared.active) = None;
                shared.completed.fetch_add(1, Ordering::SeqCst);
            }
            None => {
                if shared.closed.load(Ordering::SeqCst) {
                    return;
                }
                shared.wake.notified().await;
            }
        }
    }
}
