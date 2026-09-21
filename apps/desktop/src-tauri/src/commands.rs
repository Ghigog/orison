//! The typed command layer over `orison-core` (migration_plan.md §6.1).
//!
//! Every command here is a thin wrapper over `orison_cli::campaign` and
//! `orison_core::turn::TurnEngine` — the same functions `orison-cli` calls,
//! so the desktop shell cannot drift into a second implementation of
//! "how to open a campaign" the way `docs/migration_plan.md`'s own ordering
//! principle warns against. A turn's streamed content never rides a command's
//! return value: `connect_models` starts forwarding this campaign's
//! `TurnEvent`s as `"turn-event"` window events the moment its engine exists,
//! and every command that starts a turn (`submit_player_input`) returns
//! immediately rather than blocking the IPC call for a 15-second turn.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use orison_cli::campaign;
use orison_cli::config::{ModelArgs, Profile};
use orison_core::inference;
use orison_core::ingest::{classify, scan};
use orison_core::state::CampaignSummary;
use orison_core::turn::{CancelReason, TurnConfig, TurnEngine};

use crate::dto::{
    CharacterEmotionDto, EdgeDto, EntityDto, HistoryLineDto, IngestReportDto, ModelArgsDto,
    ModelHealthDto, ModelsSummaryDto, VaultFolderDto,
};
use crate::settings::{self, ShellSettings};
use crate::state::{lock, AppState};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The player's saved shell preferences (theme, #35), read once at startup
/// so the first screen renders in the theme they left in rather than the
/// default.
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> ShellSettings {
    settings::load(&state.settings_path)
}

/// Persist a shell preference change (e.g. a theme pick on the Settings
/// screen) so it survives a restart.
#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: ShellSettings) -> Result<(), String> {
    crate::settings::save(&state.settings_path, &settings)
}

#[tauri::command]
pub fn list_campaigns(state: State<AppState>) -> Result<Vec<CampaignSummary>, String> {
    campaign::list(&state.store).map_err(err)
}

#[tauri::command]
pub fn create_campaign(
    state: State<AppState>,
    title: String,
    vault: Option<PathBuf>,
    folder_types: BTreeMap<String, String>,
) -> Result<CampaignSummary, String> {
    let created = campaign::create(
        &state.store,
        &title,
        &orison_cli::shell::timestamp(),
        vault.as_deref(),
        &folder_types,
    )
    .map_err(err)?;
    Ok(CampaignSummary {
        id: created.campaign.id.clone(),
        title: created.campaign.title.clone(),
        last_played: created.campaign.last_played.clone(),
        playtime_seconds: created.campaign.playtime_seconds,
        active_scene: created.campaign.active_location.clone(),
    })
}

/// Compile a vault into an existing campaign, replacing its graph (§5.2).
/// The vault-import screen's "48% untyped" problem is `folder_types`: without
/// it, notes in a folder with no type hint stay `Note`, not `Character`.
#[tauri::command]
pub fn import_vault(
    state: State<AppState>,
    campaign_id: String,
    vault: PathBuf,
    folder_types: BTreeMap<String, String>,
) -> Result<IngestReportDto, String> {
    let report =
        campaign::import(&state.store, &campaign_id, &vault, &folder_types).map_err(err)?;
    Ok(IngestReportDto::from(&report))
}

/// Every folder in a vault that holds notes, with the type ingest would give
/// it unaided. The import screen lists these so the player corrects the few
/// guesses that are wrong instead of writing a mapping from scratch.
///
/// Folder mappings match a note's own folder exactly, not its ancestors, so
/// this lists every such folder rather than just the top level.
#[tauri::command]
pub fn scan_vault_folders(vault: PathBuf) -> Result<Vec<VaultFolderDto>, String> {
    let files = scan::scan(&vault).map_err(err)?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for note in &files.notes {
        let relative = scan::relative_string(note);
        *counts
            .entry(classify::parent_folder(&relative).to_string())
            .or_default() += 1;
    }
    Ok(counts
        .into_iter()
        .map(|(folder, notes)| {
            let probe = if folder.is_empty() {
                "note.md".to_string()
            } else {
                format!("{folder}/note.md")
            };
            let guess = classify::classify(&BTreeMap::new(), &probe, &BTreeMap::new());
            VaultFolderDto {
                folder,
                notes,
                guess: guess.as_str().to_string(),
            }
        })
        .collect())
}

/// Connect this campaign's two backends and build its `TurnEngine`. Every
/// other turn command needs this to have run first — mirrors `orison play`
/// connecting before handing off to `Shell`.
#[tauri::command]
pub async fn connect_models(
    app: AppHandle,
    state: State<'_, AppState>,
    campaign_id: String,
    args: ModelArgsDto,
) -> Result<ModelsSummaryDto, String> {
    let session = campaign::open_session(&state.store, &campaign_id).map_err(err)?;
    if campaign::is_empty(&session) {
        return Err(format!(
            "{campaign_id} has no vault compiled into it yet. Compile one first."
        ));
    }

    let model_args = ModelArgs {
        url: args.url,
        actor_model: args.actor_model,
        director_model: args.director_model,
        profile: if args.two_calls {
            Profile::TwoCalls
        } else {
            Profile::SingleCall
        },
        tokenizer: None,
        context_limit: args.context_limit,
    };
    let tokenizer_is_approximate = model_args.tokenizer_is_approximate();
    let backends = model_args.connect().await.map_err(err)?;
    let context_length = backends.actor.context_length();

    let config = TurnConfig {
        profile: backends.profile,
        ..TurnConfig::default()
    };
    let engine = TurnEngine::new(session, backends.actor, backends.director, config);

    // One forwarder per engine, for the engine's lifetime: every subsequent
    // subscriber would only see events from the moment it subscribed, and a
    // reconnecting frontend must not miss a turn already in flight.
    let mut events = engine.subscribe();
    let forward_campaign_id = campaign_id.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv().await {
            let payload = serde_json::json!({
                "campaignId": forward_campaign_id,
                "event": event,
            });
            let _ = app_handle.emit("turn-event", payload);
        }
    });

    state.insert_engine(campaign_id, engine);

    Ok(ModelsSummaryDto {
        summary: backends.summary,
        context_length,
        tokenizer_is_approximate,
    })
}

/// Whether `model` is reachable and pulled at `url`, for the connect
/// screen's per-field badge (#29) — answered without connecting a backend,
/// so a typo'd model name or a stopped Ollama server shows up before the
/// player clicks Connect, not as an opaque failure mid-turn.
#[tauri::command]
pub async fn check_model_health(url: String, model: String) -> Result<ModelHealthDto, String> {
    inference::probe(&url, &model)
        .await
        .map(ModelHealthDto::from)
        .map_err(err)
}

/// Remove a campaign from Orison: its transcript, knowledge graph, emotions
/// and passages. The vault folder it was compiled from is never touched, so
/// importing it again starts a fresh campaign.
#[tauri::command]
pub fn delete_campaign(state: State<AppState>, campaign_id: String) -> Result<(), String> {
    // Stop its engine first so no in-flight turn writes to a campaign that is
    // being removed.
    if let Some(engine) = state.remove_engine(&campaign_id) {
        engine.shutdown();
    }
    let removed = lock(&state.store)
        .delete_campaign(&campaign_id)
        .map_err(err)?;
    if removed {
        Ok(())
    } else {
        Err(format!("no campaign named {campaign_id}"))
    }
}

/// Whether this campaign already has a running engine, so a resume from the
/// campaigns list can skip the connect screen instead of making the player
/// re-enter models the shell is still connected to.
#[tauri::command]
pub fn is_connected(state: State<AppState>, campaign_id: String) -> bool {
    state.engine_for(&campaign_id).is_some()
}

/// The stored transcript, oldest first. The play screen keeps no history of
/// its own, so it asks for this each time it is shown.
#[tauri::command]
pub fn recent_history(
    state: State<AppState>,
    campaign_id: String,
    limit: usize,
) -> Result<Vec<HistoryLineDto>, String> {
    let entries = lock(&state.store)
        .recent_history(&campaign_id, limit)
        .map_err(err)?;
    Ok(entries
        .into_iter()
        .map(|e| HistoryLineDto {
            role: e.role.as_str().to_string(),
            text: e.content,
            sender: e.sender,
        })
        .collect())
}

fn engine_or_err(
    state: &State<AppState>,
    campaign_id: &str,
) -> Result<std::sync::Arc<TurnEngine>, String> {
    state.engine_for(campaign_id).ok_or_else(|| {
        format!("{campaign_id} has no connected models yet. Call connectModels first.")
    })
}

/// Fire-and-forget: the turn's narrative content arrives as `"turn-event"`
/// events, not as this call's return value (see the module doc).
/// `TurnEngine` cancels any turn already in flight itself, so the desktop
/// shell never has to.
///
/// The one thing the event stream does not carry is `TurnOutcome`'s
/// instrumentation — latency, time-to-first-token, cache reuse — since that
/// only exists once the turn has fully applied. This spawns a task to await
/// it and emit it separately as `"turn-outcome"`, once, rather than making
/// every `TurnEvent` variant carry fields that are meaningless until the
/// last one. A cancelled or failed turn (`FailureKind`, already on
/// `"turn-event"` as `Failed`) has no outcome to report and emits nothing
/// here.
#[tauri::command]
pub fn submit_player_input(
    app: AppHandle,
    state: State<AppState>,
    campaign_id: String,
    text: String,
) -> Result<(), String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    let ticket = engine.submit_player_input(text);
    tauri::async_runtime::spawn(async move {
        if let Ok(outcome) = ticket.join().await {
            let payload = serde_json::json!({
                "campaignId": campaign_id,
                "outcome": {
                    "latencyMs": outcome.latency.as_secs_f64() * 1000.0,
                    "timeToFirstTokenMs": outcome.time_to_first_token.map(|d| d.as_secs_f64() * 1000.0),
                    "promptTokens": outcome.prompt_tokens,
                    "evaluatedPromptTokens": outcome.evaluated_prompt_tokens,
                    "promptEvalTimeMs": outcome.prompt_eval_time.map(|d| d.as_secs_f64() * 1000.0),
                    "completionTokens": outcome.completion_tokens,
                    "retrieved": outcome.retrieved,
                    "directorTriggered": outcome.director_triggered,
                },
            });
            let _ = app.emit("turn-outcome", payload);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_current_turn(state: State<AppState>, campaign_id: String) -> Result<(), String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine.cancel_current_turn(CancelReason::Requested);
    Ok(())
}

#[tauri::command]
pub fn select_character(
    state: State<AppState>,
    campaign_id: String,
    entity_id: String,
) -> Result<(), String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine.select_character(entity_id).map_err(err)
}

#[tauri::command]
pub fn move_to_location(
    state: State<AppState>,
    campaign_id: String,
    name: String,
) -> Result<EntityDto, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine
        .move_to_location(&name)
        .map_err(err)
        .map(|e| EntityDto::from(&e))
}

#[tauri::command]
pub fn current_location(
    state: State<AppState>,
    campaign_id: String,
) -> Result<Option<EntityDto>, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine
        .current_location()
        .map_err(err)
        .map(|opt| opt.as_ref().map(EntityDto::from))
}

#[tauri::command]
pub fn exits(state: State<AppState>, campaign_id: String) -> Result<Vec<EntityDto>, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine
        .exits()
        .map_err(err)
        .map(|es| es.iter().map(EntityDto::from).collect())
}

#[tauri::command]
pub fn characters_present(
    state: State<AppState>,
    campaign_id: String,
) -> Result<Vec<EntityDto>, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine
        .characters_present()
        .map_err(err)
        .map(|es| es.iter().map(EntityDto::from).collect())
}

#[tauri::command]
pub fn locations(state: State<AppState>, campaign_id: String) -> Result<Vec<EntityDto>, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    Ok(engine.locations().iter().map(EntityDto::from).collect())
}

/// The graph's edges, for the Map screen's drawn graph (#34) — see
/// `TurnEngine::graph_edges` for what's excluded (RAPTOR summary nodes) and
/// why a dangling link cannot be told apart from "no edge" once the graph is
/// loaded (it was never stored as one).
#[tauri::command]
pub fn graph_edges(state: State<AppState>, campaign_id: String) -> Result<Vec<EdgeDto>, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    Ok(engine.graph_edges().iter().map(EdgeDto::from).collect())
}

/// A character's current rapport and emotional state, for the character
/// screen (#33). Read-only — see `TurnEngine::character_emotion`.
#[tauri::command]
pub fn character_emotion(
    state: State<AppState>,
    campaign_id: String,
    entity_id: String,
) -> Result<CharacterEmotionDto, String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    let (emotion_state, affinity) = engine.character_emotion(&entity_id).map_err(err)?;
    Ok(CharacterEmotionDto::new(&emotion_state, affinity))
}
