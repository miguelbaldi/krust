// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use tracing::*;

use crate::{
    component::settings_dialog::MessagesSortOrder,
    config::{ensure_app_config_dir, ExternalError},
    DATE_TIME_FORMAT, DATE_TIME_WITH_MILLIS_FORMAT,
};

/// Application global settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// cache directory as string.
    pub cache_dir: String,
    pub is_full_timestamp: bool,
    pub messages_sort_column: String,
    pub messages_sort_column_order: String,
    pub threads_number: u8,
    pub default_connection_timeout: usize,
}

impl Settings {
    /// Read from the state file on disk.
    pub fn read() -> Result<Self, ExternalError> {
        let path = settings_path()?;
        serde_json::from_reader(File::open(path).map_err(|e| {
            ExternalError::ConfigurationError(format!("unable to open file: {:?}", e))
        })?)
        .map_err(|e| ExternalError::ConfigurationError(format!("unable to read state: {:?}", e)))
    }

    /// Persist to disk.
    pub fn write(&self) -> Result<(), ExternalError> {
        let path = settings_path()?;

        info!(
            "persisting application state: {:?}, into path: {:?}",
            self, path
        );

        let file = File::create(path).map_err(|op| {
            ExternalError::ConfigurationError(format!(
                "unable to create intermediate directories: {:?}",
                op
            ))
        })?;
        serde_json::to_writer(file, self).map_err(|op| {
            ExternalError::ConfigurationError(format!(
                "unable to create to write state to disk: {:?}",
                op
            ))
        })
    }
    pub fn timestamp_formatter(&self) -> String {
        if self.is_full_timestamp {
            DATE_TIME_WITH_MILLIS_FORMAT
        } else {
            DATE_TIME_FORMAT
        }
        .to_string()
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
            is_full_timestamp: false,
            messages_sort_column: "Offset".to_string(),
            messages_sort_column_order: MessagesSortOrder::Default.to_string(),
            threads_number: 4,
            default_connection_timeout: 5,
        }
    }
}

fn settings_path() -> Result<PathBuf, ExternalError> {
    Ok(ensure_app_config_dir()?.join("settings.json"))
}

fn default_cache_path() -> Result<PathBuf, ExternalError> {
    Ok(ensure_app_config_dir()?.join("cache"))
}
