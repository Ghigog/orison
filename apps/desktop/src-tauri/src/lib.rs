//! Orison's Tauri 2 desktop shell (migration_plan.md §6.1): a typed command
//! layer over `orison-core`/`orison-cli`, streaming turn events over Tauri's
//! event channel. `orison-core` never knows what is rendering it — this
//! crate is the one place that does.

mod commands;
mod dto;
mod state;

use std::sync::{Arc, Mutex};

use tauri::Manager;

use orison_core::state::CampaignStore;

use state::AppState;

/// Where the campaign database lives on this machine.
///
/// `orison_cli::campaign::default_database` falls back to a relative
/// `orison.sqlite3` for a terminal's working directory, which is right for a
/// CLI and wrong for a window with no working directory the player chose.
/// The desktop shell asks Tauri for the OS-appropriate app-data directory
/// instead — `~/Library/Application Support/orison` on macOS, the Settings
/// screen's own answer to "what leaves this machine": nothing, and here is
/// where what stays lives.
fn database_path(app: &tauri::App) -> Result<std::path::PathBuf, String> {
    if let Some(path) = std::env::var_os("ORISON_DB") {
        return Ok(path.into());
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve the app data directory: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("orison.db"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db_path = database_path(app)?;
            let store: Arc<Mutex<CampaignStore>> = Arc::new(Mutex::new(
                CampaignStore::open(&db_path).map_err(|e| e.to_string())?,
            ));
            app.manage(AppState::new(store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_campaigns,
            commands::create_campaign,
            commands::import_vault,
            commands::connect_models,
            commands::submit_player_input,
            commands::cancel_current_turn,
            commands::select_character,
            commands::move_to_location,
            commands::current_location,
            commands::exits,
            commands::characters_present,
            commands::locations,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
