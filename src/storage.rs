//! JSON persistence. Two roots, both resolved here and nowhere else:
//! - [`app_dir`]: app-level config (`settings.json`, the workspace registry). `~/Library/Application
//!   Support/Buffer` on macOS, or `$BUFFER_DATA_DIR`.
//! - [`data_dir`]: the active workspace's root, which holds all content (tasks, notes, focus,
//!   images). Set by [`crate::workspaces`]; the default workspace is the app dir itself.
//!
//! Content paths are always relative to [`data_dir`] (`load`, `save`, `path`, …).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::RwLock,
};

use anyhow::Context as _;
use serde::{Serialize, de::DeserializeOwned};

use crate::APP_NAME;

/// The app was called Workdesk before 0.11; its data folder is moved on first launch.
const LEGACY_NAME: &str = "Workdesk";

/// Active workspace root; `None` until [`set_root`] = the app dir.
static ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// App-level config folder. `BUFFER_DATA_DIR` (or the pre-rename `WORKDESK_DATA_DIR`) overrides
/// the location, for testing against throwaway data.
pub fn app_dir() -> PathBuf {
    if let Some(dir) =
        std::env::var_os("BUFFER_DATA_DIR").or_else(|| std::env::var_os("WORKDESK_DATA_DIR"))
    {
        return dir.into();
    }
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(APP_NAME);
    let legacy = base.join(LEGACY_NAME);
    // Not migrated (the move failed): keep using the old folder rather than start empty.
    if !dir.exists() && legacy.exists() {
        return legacy;
    }
    dir
}

/// Root folder of the active workspace: every content file lives under it.
pub fn data_dir() -> PathBuf {
    ROOT.read()
        .ok()
        .and_then(|root| root.clone())
        .unwrap_or_else(app_dir)
}

/// Points all content loads and saves at another workspace root.
pub fn set_root(dir: PathBuf) {
    if let Ok(mut root) = ROOT.write() {
        *root = Some(dir);
    }
}

/// Moves the pre-rename data folder to its new name, once. Call before anything loads.
pub fn migrate_legacy_dir() {
    if std::env::var_os("BUFFER_DATA_DIR").is_some()
        || std::env::var_os("WORKDESK_DATA_DIR").is_some()
    {
        return;
    }
    let Some(base) = dirs::data_dir() else { return };
    let (dir, legacy) = (base.join(APP_NAME), base.join(LEGACY_NAME));
    if !dir.exists()
        && legacy.exists()
        && let Err(err) = fs::rename(&legacy, &dir)
    {
        eprintln!(
            "couldn't move {} to {}: {err}",
            legacy.display(),
            dir.display()
        );
    }
}

/// Absolute path of `rel` inside the data dir.
pub fn path(rel: &str) -> PathBuf {
    data_dir().join(rel)
}

/// Loads `file`, falling back to `T::default()` when it doesn't exist yet.
/// A file that can't be parsed is moved aside (never overwritten) so no data is lost.
pub fn load<T: DeserializeOwned + Default>(file: &str) -> T {
    load_from(&path(file))
}

/// Writes atomically: a crash mid-write leaves the previous file intact.
pub fn save<T: Serialize>(file: &str, value: &T) -> anyhow::Result<()> {
    write_atomic(&path(file), &serde_json::to_vec_pretty(value)?)
}

/// [`load`] for app-level config in [`app_dir`] (not workspace content).
pub fn load_app<T: DeserializeOwned + Default>(file: &str) -> T {
    load_from(&app_dir().join(file))
}

/// [`save`] for app-level config in [`app_dir`] (not workspace content).
pub fn save_app<T: Serialize>(file: &str, value: &T) -> anyhow::Result<()> {
    write_atomic(&app_dir().join(file), &serde_json::to_vec_pretty(value)?)
}

/// [`save`] to a path resolved earlier with [`path`], for writes that finish on another thread
/// after the active workspace may have changed.
pub fn save_at<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

/// [`save_app`] readable only by the user (0600): for secrets.
pub fn save_app_private<T: Serialize>(file: &str, value: &T) -> anyhow::Result<()> {
    write_atomic_mode(
        &app_dir().join(file),
        &serde_json::to_vec_pretty(value)?,
        0o600,
    )
}

fn load_from<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(err) => {
                let backup = quarantine(path);
                eprintln!(
                    "{} is unreadable ({err}); moved to {}",
                    path.display(),
                    backup.display()
                );
                T::default()
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => T::default(),
        Err(err) => {
            eprintln!("failed to read {}: {err}", path.display());
            T::default()
        }
    }
}

/// Reads a text file; missing or unreadable → empty.
pub fn load_text(file: &str) -> String {
    let path = path(file);
    match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            eprintln!("failed to read {}: {err}", path.display());
            String::new()
        }
    }
}

/// Writes `text` atomically; empty text removes the file instead.
pub fn save_text(file: &str, text: &str) -> anyhow::Result<()> {
    if text.is_empty() {
        return match fs::remove_file(path(file)) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => {
                Err(err).with_context(|| format!("removing {file}"))
            }
            _ => Ok(()),
        };
    }
    write_atomic(&path(file), text.as_bytes())
}

/// Removes a directory and everything in it; missing is fine.
pub fn remove_dir(dir: &str) -> anyhow::Result<()> {
    match fs::remove_dir_all(path(dir)) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => {
            Err(err).with_context(|| format!("removing {dir}"))
        }
        _ => Ok(()),
    }
}

/// Names of the subfolders of `dir`, sorted; missing → none.
pub fn list_dirs(dir: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(path(dir)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// Copies `from` to `to` (both relative to the data dir).
pub fn copy(from: &str, to: &str) -> anyhow::Result<()> {
    fs::copy(path(from), path(to)).with_context(|| format!("copying {from} to {to}"))?;
    Ok(())
}

/// A crash mid-write leaves the previous file intact.
fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    write_atomic_mode(path, bytes, 0o644)
}

/// [`write_atomic`] with the file's Unix permissions (applied before it replaces the old one).
fn write_atomic_mode(path: &Path, bytes: &[u8], mode: u32) -> anyhow::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let mut tmp = path.to_path_buf().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = fs::remove_file(&tmp);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)
        .and_then(|mut f| f.write_all(bytes))
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

fn quarantine(path: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_extension(format!("corrupt-{stamp}.json"));
    if let Err(err) = fs::rename(path, &backup) {
        eprintln!("failed to back up {}: {err}", path.display());
    }
    backup
}
