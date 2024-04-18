use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use tracing::*;

use crate::config::{ensure_app_config_dir, ExternalError};

/// Application global settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// cache directory as string.
    pub cache_dir: String,
}

impl Settings {
    /// Read from the state file on disk.
    pub fn read() -> Result<Self, ExternalError> {
        let path = settings_path()?;
        serde_json::from_reader(File::open(path).map_err(|e| {
            ExternalError::ConfigurationError(format!("unable to open file: {:?}", e))
        })?)
        .map_err(|e| {
            ExternalError::ConfigurationError(format!("unable to read state: {:?}", e))
        })
    }

    /// Persist to disk.
    pub fn write(&self) -> Result<(), ExternalError> {
        let path = settings_path()?;

        info!(
            "persisting application state: {:?}, into path: {:?}",
            self, path
        );

        let file = File::create(path).map_err(|op| {
            ExternalError::ConfigurationError(
                format!("unable to create intermediate directories: {:?}", op),
            )
        })?;
        serde_json::to_writer(file, self).map_err(|op| {
            ExternalError::ConfigurationError(
                format!("unable to create to write state to disk: {:?}", op),
            )
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        let default_cache_dir = default_cache_path()
            .ok()
            .and_then(move |pathbuf| pathbuf.to_str().map(|path_str| path_str.to_string()))
            .expect("should get default cache path");
        Settings {
            cache_dir: default_cache_dir,
        }
    }
}

fn settings_path() -> Result<PathBuf, ExternalError> {
    Ok(ensure_app_config_dir()?.join("settings.json"))
}

fn default_cache_path() -> Result<PathBuf, ExternalError> {
    Ok(ensure_app_config_dir()?.join("cache"))
}
