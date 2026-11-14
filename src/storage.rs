//! JSON persistence under the OS data dir (`~/Library/Application Support/Buffer` on macOS).

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use serde::{Serialize, de::DeserializeOwned};

use crate::APP_NAME;

/// The app was called Workdesk before 0.11; its data folder is moved on first launch.
const LEGACY_NAME: &str = "Workdesk";

/// `BUFFER_DATA_DIR` (or the pre-rename `WORKDESK_DATA_DIR`) overrides the location, for
/// testing against throwaway data.
pub fn data_dir() -> PathBuf {
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
    let path = data_dir().join(file);
    match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(err) => {
                let backup = quarantine(&path);
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

/// Writes atomically: a crash mid-write leaves the previous file intact.
pub fn save<T: Serialize>(file: &str, value: &T) -> anyhow::Result<()> {
    write_atomic(file, &serde_json::to_vec_pretty(value)?)
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
    write_atomic(file, text.as_bytes())
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

/// Copies `from` to `to` (both relative to the data dir).
pub fn copy(from: &str, to: &str) -> anyhow::Result<()> {
    fs::copy(path(from), path(to)).with_context(|| format!("copying {from} to {to}"))?;
    Ok(())
}

/// A crash mid-write leaves the previous file intact.
fn write_atomic(file: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let path = path(file);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
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
