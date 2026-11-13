//! JSON persistence under the OS data dir (`~/Library/Application Support/Workdesk` on macOS).

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use serde::{Serialize, de::DeserializeOwned};

use crate::APP_NAME;

/// `WORKDESK_DATA_DIR` overrides the location (useful for testing against throwaway data).
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WORKDESK_DATA_DIR") {
        return dir.into();
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
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
    let dir = data_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(file);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
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
