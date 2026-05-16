use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use super::user_config::{ConfigValidationError, UserConfigFile};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub(crate) enum UserConfigStorageError {
    #[error("{0}")]
    Validation(#[from] ConfigValidationError),

    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("no {path} on disk and no embedded fallback at {asset}")]
    MissingFallback { path: PathBuf, asset: String },

    #[error("invalid JSON for {name}-config from {source_name}: {source}")]
    InvalidJson {
        name: &'static str,
        source_name: String,
        source: serde_json::Error,
    },

    #[error("failed to serialize {name}-config: {source}")]
    Serialize {
        name: &'static str,
        source: serde_json::Error,
    },

    #[error("failed to create {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to create temporary file {path}: {source}")]
    TempCreate {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write temporary file {path}: {source}")]
    TempWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to flush temporary file {path}: {source}")]
    TempFlush {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to replace {path} with {temp_path}: {source}")]
    Replace {
        path: PathBuf,
        temp_path: PathBuf,
        source: std::io::Error,
    },
}

impl UserConfigStorageError {
    pub(crate) fn is_client_error(&self) -> bool {
        matches!(self, Self::Validation(_) | Self::InvalidJson { .. })
    }
}

pub(crate) fn user_config_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}-config.json", name))
}

pub(crate) fn read_user_config<T>(dir: &Path) -> Result<T, UserConfigStorageError>
where
    T: UserConfigFile + DeserializeOwned,
{
    let dir =
        crate::path_safety::require_safe_user_path(dir).map_err(|e| UserConfigStorageError::Read {
            path: dir.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
        })?;
    let path = user_config_path(&dir, T::NAME);
    let raw = match fs::read_to_string(&path) {
        Ok(content) => (content, path.display().to_string()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let asset = defaults_asset_path(T::NAME);
            let content = crate::web::embedded_ui::read_text(&asset).ok_or_else(|| {
                UserConfigStorageError::MissingFallback {
                    path: path.clone(),
                    asset: asset.clone(),
                }
            })?;
            (content, asset)
        }
        Err(source) => {
            return Err(UserConfigStorageError::Read {
                path: path.clone(),
                source,
            })
        }
    };

    let mut config = serde_json::from_str::<T>(&raw.0).map_err(|source| {
        UserConfigStorageError::InvalidJson {
            name: T::NAME,
            source_name: raw.1,
            source,
        }
    })?;
    config.validate_and_normalize()?;
    Ok(config)
}

pub(crate) fn write_user_config<T>(dir: &Path, mut config: T) -> Result<(), UserConfigStorageError>
where
    T: UserConfigFile,
{
    config.validate_and_normalize()?;
    let dir = crate::path_safety::require_safe_user_path(dir).map_err(|e| {
        UserConfigStorageError::CreateDir {
            path: dir.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
        }
    })?;
    let path = user_config_path(&dir, T::NAME);
    write_json_atomic(T::NAME, &path, &config)
}

pub(crate) fn write_json_atomic<T: Serialize>(
    name: &'static str,
    path: &Path,
    value: &T,
) -> Result<(), UserConfigStorageError> {
    let temp_path = next_temp_path(path);
    write_json_atomic_with_temp(name, path, &temp_path, value)
}

pub(crate) fn write_json_atomic_with_temp<T: Serialize>(
    name: &'static str,
    path: &Path,
    temp_path: &Path,
    value: &T,
) -> Result<(), UserConfigStorageError> {
    let body = serde_json::to_string_pretty(value)
        .map_err(|source| UserConfigStorageError::Serialize { name, source })?
        + "\n";

    let path = crate::path_safety::require_safe_user_path(path).map_err(|e| {
        UserConfigStorageError::CreateDir {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
        }
    })?;
    let temp_path = crate::path_safety::require_safe_user_path(temp_path).map_err(|e| {
        UserConfigStorageError::TempCreate {
            path: temp_path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
        }
    })?;

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|source| UserConfigStorageError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|source| UserConfigStorageError::TempCreate {
            path: temp_path.clone(),
            source,
        })?;

    if let Err(source) = file.write_all(body.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(&temp_path);
        return Err(UserConfigStorageError::TempWrite {
            path: temp_path.clone(),
            source,
        });
    }

    if let Err(source) = file.sync_all() {
        drop(file);
        let _ = fs::remove_file(&temp_path);
        return Err(UserConfigStorageError::TempFlush {
            path: temp_path.clone(),
            source,
        });
    }

    drop(file);
    if let Err(source) = fs::rename(&temp_path, &path) {
        let _ = fs::remove_file(&temp_path);
        return Err(UserConfigStorageError::Replace {
            path: path.clone(),
            temp_path: temp_path.clone(),
            source,
        });
    }

    Ok(())
}

fn defaults_asset_path(name: &str) -> String {
    format!("config/{}-defaults.json", name)
}

fn next_temp_path(path: &Path) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let temp_name = format!(".{}.{}.{}.tmp", file_name, std::process::id(), counter);
    path.with_file_name(temp_name)
}
