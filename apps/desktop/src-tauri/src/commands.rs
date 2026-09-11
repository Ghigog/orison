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
use orison_core::state::CampaignSummary;
use orison_core::turn::{CancelReason, TurnConfig, TurnEngine};

use crate::dto::{EntityDto, IngestReportDto, ModelArgsDto, ModelsSummaryDto};
use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
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

fn engine_or_err(
    state: &State<AppState>,
    campaign_id: &str,
) -> Result<std::sync::Arc<TurnEngine>, String> {
    state.engine_for(campaign_id).ok_or_else(|| {
        format!("{campaign_id} has no connected models yet. Call connectModels first.")
    })
}

/// Fire-and-forget: the turn's content arrives as `"turn-event"` events, not
/// as this call's return value (see the module doc). `TurnEngine` cancels any
/// turn already in flight itself, so the desktop shell never has to.
#[tauri::command]
pub fn submit_player_input(
    state: State<AppState>,
    campaign_id: String,
    text: String,
) -> Result<(), String> {
    let engine = engine_or_err(&state, &campaign_id)?;
    engine.submit_player_input(text);
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
