//! What the Tauri shell holds that a turn's caller needs across commands: the
//! campaign store, and one running `TurnEngine` per campaign that has models
//! connected.
//!
//! `orison-core` never knows what is rendering it (§3.1), so nothing here
//! reaches into a window; this is the same shape `orison-cli`'s `Shell` holds
//! for one campaign, generalised to the several a desktop window can have
//! open across its lifetime.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use orison_core::state::CampaignStore;
use orison_core::turn::TurnEngine;

pub struct AppState {
    pub store: Arc<Mutex<CampaignStore>>,
    pub sessions: Mutex<HashMap<String, Arc<TurnEngine>>>,
}

impl AppState {
    pub fn new(store: Arc<Mutex<CampaignStore>>) -> Self {
        Self {
            store,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn engine_for(&self, campaign_id: &str) -> Option<Arc<TurnEngine>> {
        lock(&self.sessions).get(campaign_id).cloned()
    }

    pub fn insert_engine(&self, campaign_id: String, engine: Arc<TurnEngine>) {
        lock(&self.sessions).insert(campaign_id, engine);
    }
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
