//! Shell-level preferences — the player's own choices about how Orison looks
//! (migration_plan.md §6.1, issue #35). These aren't campaign data: a theme
//! is a property of the machine's reader, not the story, so it doesn't
//! belong in `CampaignStore` beside the knowledge graph. It lives in its own
//! small JSON file next to `orison.db` instead.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShellSettings {
    /// `"lamplight"` or `"eink"` — mirrors the frontend's `ThemeName`. Kept
    /// as a plain string rather than a Rust enum: this crate never reads the
    /// value, only stores and returns what the frontend already validated
    /// against its own theme list.
    pub theme: String,
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            theme: "lamplight".to_string(),
        }
    }
}

/// The player's last-saved preferences, or the defaults above if the file is
/// missing, unreadable, or holds something that no longer parses — a
/// corrupt or hand-edited settings file should never stop the shell from
/// starting.
pub fn load(path: &Path) -> ShellSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &ShellSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell_settings.json");
        assert_eq!(load(&path), ShellSettings::default());
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell_settings.json");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), ShellSettings::default());
    }

    #[test]
    fn round_trips_a_saved_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell_settings.json");
        let saved = ShellSettings {
            theme: "eink".to_string(),
        };
        save(&path, &saved).unwrap();
        assert_eq!(load(&path), saved);
    }
}
