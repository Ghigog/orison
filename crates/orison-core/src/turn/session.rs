//! What a turn runs against: one campaign's state, graph and index.
//!
//! Bundled into one type because the alternative — passing four handles into
//! every function — is how `GameLoopController` ended up reaching into six
//! autoloads from one file. Nothing here is a second copy of anything: the
//! graph is [`crate::knowledge`]'s, the rows are [`crate::state`]'s.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::knowledge::KnowledgeGraph;
use crate::retrieval::LexicalIndex;
use crate::state::{CampaignStore, StateError};

use super::clock::{Clock, SystemClock};
use super::error::TurnError;

/// The long-lived, per-campaign context a turn needs.
///
/// Cheap to clone: everything inside is an `Arc`.
#[derive(Clone)]
pub struct Session {
    campaign_id: String,
    /// The database is behind a mutex because `rusqlite::Connection` is not
    /// `Sync`, not because concurrency needs serialising — WAL does that
    /// (§3.1). The lock is therefore never held across an `await`; every use
    /// goes through [`Session::with_store`], which cannot.
    store: Arc<Mutex<CampaignStore>>,
    graph: Arc<KnowledgeGraph>,
    /// Rebuilt on load, per §3.4: the lexical index is in-memory and derived.
    lexical: Arc<LexicalIndex>,
    clock: Arc<dyn Clock>,
}

impl Session {
    pub fn new(
        campaign_id: impl Into<String>,
        store: Arc<Mutex<CampaignStore>>,
        graph: Arc<KnowledgeGraph>,
        lexical: Arc<LexicalIndex>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            campaign_id: campaign_id.into(),
            store,
            graph,
            lexical,
            clock,
        }
    }

    /// Load a campaign's graph from the store and build its index.
    pub fn open(
        campaign_id: impl Into<String>,
        store: Arc<Mutex<CampaignStore>>,
    ) -> Result<Self, TurnError> {
        let campaign_id = campaign_id.into();
        let graph = {
            let guard = lock(&store);
            KnowledgeGraph::load(&guard, &campaign_id)?
        };
        let lexical = LexicalIndex::build(&graph)?;
        Ok(Self {
            campaign_id,
            store,
            graph: Arc::new(graph),
            lexical: Arc::new(lexical),
            clock: Arc::new(SystemClock),
        })
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn campaign_id(&self) -> &str {
        &self.campaign_id
    }

    pub fn graph(&self) -> &KnowledgeGraph {
        &self.graph
    }

    pub fn lexical(&self) -> &LexicalIndex {
        &self.lexical
    }

    pub fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    /// Run one synchronous unit of database work.
    ///
    /// Taking a closure rather than handing out the guard is what makes
    /// "never hold the lock across an await" a property of the type instead
    /// of a rule someone has to remember.
    pub fn with_store<R>(
        &self,
        f: impl FnOnce(&mut CampaignStore) -> Result<R, StateError>,
    ) -> Result<R, TurnError> {
        let mut guard = lock(&self.store);
        f(&mut guard).map_err(TurnError::from)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
