//! Creating, listing, opening and importing into a campaign (§5.1, §5.2).
//!
//! **There is no save command that writes a file.** Every turn commits to
//! SQLite as it happens, so "save" is a state of the world rather than an
//! action — which is the whole point of §3.1's move off `SaveManager.gd`'s
//! JSON blobs. `/save` in the shell exists to flush the playtime clock, and
//! says so.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use orison_core::ingest::{ingest_vault, IngestOptions, IngestReport};
use orison_core::knowledge::{EntityKind, KnowledgeGraph};
use orison_core::state::{Campaign, CampaignStore, CampaignSummary, ChunkRow};
use orison_core::turn::Session;

use crate::error::CliError;

/// Where campaigns live when the caller does not say.
///
/// One file for every campaign, because `CampaignStore` keys everything by
/// campaign id and the Godot build's file-per-save is what made listing them
/// mean parsing all of them.
pub fn default_database() -> PathBuf {
    std::env::var_os("ORISON_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("orison.sqlite3"))
}

pub fn open_store(path: &Path) -> Result<Arc<Mutex<CampaignStore>>, CliError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(Arc::new(Mutex::new(CampaignStore::open(path)?)))
}

pub fn list(store: &Arc<Mutex<CampaignStore>>) -> Result<Vec<CampaignSummary>, CliError> {
    Ok(lock(store).list_campaigns()?)
}

/// An id derived from the title, so `orison play` takes something typeable.
///
/// Collisions are resolved by suffix rather than by refusing: two campaigns
/// called "Thornwick" is a reasonable thing to want.
pub fn slug_for(store: &Arc<Mutex<CampaignStore>>, title: &str) -> Result<String, CliError> {
    let base: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let base = base.trim_matches('-').replace("--", "-");
    let base = if base.is_empty() {
        "campaign".to_string()
    } else {
        base
    };

    let taken: Vec<String> = list(store)?.into_iter().map(|c| c.id).collect();
    if !taken.contains(&base) {
        return Ok(base);
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !taken.contains(&candidate) {
            return Ok(candidate);
        }
    }
    unreachable!("the loop above returns")
}

pub struct Created {
    pub campaign: Campaign,
    pub import: Option<IngestReport>,
}

/// Create a campaign, optionally importing a vault into it at once.
pub fn create(
    store: &Arc<Mutex<CampaignStore>>,
    title: &str,
    now: &str,
    vault: Option<&Path>,
    folder_types: &BTreeMap<String, String>,
) -> Result<Created, CliError> {
    let id = slug_for(store, title)?;
    let campaign = Campaign::new(&id, title, now);
    lock(store).save_campaign(&campaign)?;

    let import = match vault {
        Some(vault) => Some(import(store, &id, vault, folder_types)?),
        None => None,
    };
    let campaign = lock(store)
        .load_campaign(&id)?
        .ok_or_else(|| CliError::NoSuchCampaign(id.clone()))?;
    Ok(Created { campaign, import })
}

/// Compile a vault into an existing campaign (§5.2).
///
/// Replaces the campaign's graph and chunks wholesale rather than merging.
/// Re-importing an edited vault is the common case and a merge would have to
/// guess what the author deleted; the transcript, emotions and inventory are
/// keyed separately and survive.
pub fn import(
    store: &Arc<Mutex<CampaignStore>>,
    campaign_id: &str,
    vault: &Path,
    folder_types: &BTreeMap<String, String>,
) -> Result<IngestReport, CliError> {
    let options = IngestOptions {
        folder_types: folder_types.clone(),
        ..IngestOptions::default()
    };
    let outcome = ingest_vault(vault, &options)?;

    let rows: Vec<ChunkRow> = outcome.chunks.iter().map(ChunkRow::from).collect();
    let mut guard = lock(store);
    outcome.graph.save(&mut guard, campaign_id)?;
    guard.replace_chunks(campaign_id, &rows)?;

    let mut campaign = guard
        .load_campaign(campaign_id)?
        .ok_or_else(|| CliError::NoSuchCampaign(campaign_id.to_string()))?;
    campaign.writing_style = outcome.writing_style.clone();

    // Somewhere to stand and someone to talk to, chosen only if the campaign
    // has neither. Overwriting a player's current position on a re-import
    // would be a surprise, and a silent one.
    if campaign.active_location.is_empty() {
        if let Some(first) = first_by_label(&outcome.graph, EntityKind::Location) {
            campaign.active_location = first;
        }
    }
    if campaign.active_character.is_empty() {
        if let Some(first) = first_by_label(&outcome.graph, EntityKind::Character) {
            campaign.active_character = first;
        }
    }
    guard.save_campaign(&campaign)?;
    Ok(outcome.report)
}

fn first_by_label(graph: &KnowledgeGraph, kind: EntityKind) -> Option<String> {
    let mut all: Vec<_> = graph.by_kind(kind).collect();
    all.sort_by(|a, b| a.label.cmp(&b.label));
    all.first().map(|e| e.id.as_str().to_string())
}

/// Load a campaign's graph and build its index.
pub fn open_session(
    store: &Arc<Mutex<CampaignStore>>,
    campaign_id: &str,
) -> Result<Session, CliError> {
    let exists = lock(store).load_campaign(campaign_id)?.is_some();
    if !exists {
        return Err(CliError::NoSuchCampaign(campaign_id.to_string()));
    }
    Ok(Session::open(campaign_id, Arc::clone(store))?)
}

/// The lexical index a session builds is derived, so a campaign with no
/// entities is a legitimate state rather than a failure. Kept here so the
/// shell can say "import a vault first" instead of the engine returning
/// nothing from every query.
pub fn is_empty(session: &Session) -> bool {
    session.graph().is_empty()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
