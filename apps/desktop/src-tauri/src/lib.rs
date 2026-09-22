//! Orison's Tauri 2 desktop shell (migration_plan.md §6.1): a typed command
//! layer over `orison-core`/`orison-cli`, streaming turn events over Tauri's
//! event channel. `orison-core` never knows what is rendering it — this
//! crate is the one place that does.

mod commands;
mod dto;
mod settings;
mod state;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use orison_core::state::CampaignStore;

use state::AppState;

/// The OS-appropriate app-data directory — `~/Library/Application
/// Support/orison` on macOS — the Settings screen's own answer to "what
/// leaves this machine": nothing, and here is where what stays lives.
/// Shared by `database_path` and `settings_path` below, each of which adds
/// its own filename.
fn app_data_dir(app: &tauri::App) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve the app data directory: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Where the campaign database lives on this machine.
///
/// `orison_cli::campaign::default_database` falls back to a relative
/// `orison.sqlite3` for a terminal's working directory, which is right for a
/// CLI and wrong for a window with no working directory the player chose.
fn database_path(app: &tauri::App) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("ORISON_DB") {
        return Ok(path.into());
    }
    Ok(app_data_dir(app)?.join("orison.db"))
}

/// Where the player's shell-level preferences (theme, #35) are stored —
/// beside the database, not inside it (see `settings.rs`).
fn settings_path(app: &tauri::App) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("shell_settings.json"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let db_path = database_path(app)?;
            let store: Arc<Mutex<CampaignStore>> = Arc::new(Mutex::new(
                CampaignStore::open(&db_path).map_err(|e| e.to_string())?,
            ));
            app.manage(AppState::new(store, settings_path(app)?));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_campaigns,
            commands::create_campaign,
            commands::import_vault,
            commands::scan_vault_folders,
            commands::connect_models,
            commands::check_model_health,
            commands::delete_campaign,
            commands::is_connected,
            commands::recent_history,
            commands::submit_player_input,
            commands::cancel_current_turn,
            commands::select_character,
            commands::move_to_location,
            commands::current_location,
            commands::exits,
            commands::characters_present,
            commands::locations,
            commands::graph_edges,
            commands::character_emotion,
            commands::generate_starters,
            commands::pick_starter,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
