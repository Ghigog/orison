//! Walking a vault directory.
//!
//! Ports `_scan_dir_recursive()`, including its one good decision: skip every
//! directory whose name starts with a dot, which covers `.obsidian`, `.git`
//! and `.trash` in one rule rather than a list that goes stale.

use std::path::{Path, PathBuf};

use super::error::IngestError;

/// Everything in a vault worth looking at, with paths recorded relative to the
/// vault root so they are portable between machines — `orison_audit.md` B-12
/// is about absolute paths escaping into stored data.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VaultFiles {
    pub notes: Vec<PathBuf>,
    pub images: Vec<PathBuf>,
    pub audio: Vec<PathBuf>,
}

const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
const AUDIO_EXTENSIONS: [&str; 4] = ["ogg", "mp3", "wav", "flac"];

pub fn scan(vault_root: &Path) -> Result<VaultFiles, IngestError> {
    if !vault_root.is_dir() {
        return Err(IngestError::VaultNotFound(vault_root.display().to_string()));
    }
    let mut files = VaultFiles::default();
    walk(vault_root, vault_root, &mut files)?;
    files.notes.sort();
    files.images.sort();
    files.audio.sort();
    Ok(files)
}

fn walk(root: &Path, dir: &Path, out: &mut VaultFiles) -> Result<(), IngestError> {
    let entries = std::fs::read_dir(dir).map_err(|e| IngestError::Io {
        path: dir.display().to_string(),
        detail: e.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| IngestError::Io {
            path: dir.display().to_string(),
            detail: e.to_string(),
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if ext == "md" {
            out.notes.push(relative.to_path_buf());
        } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            out.images.push(relative.to_path_buf());
        } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            out.audio.push(relative.to_path_buf());
        }
    }
    Ok(())
}

/// Vault-relative path as a `/`-separated string, whatever the platform.
pub fn relative_string(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
