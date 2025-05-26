// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use chrono::Utc;
use directories::ProjectDirs;
use rdkafka::error::KafkaError;
use ron::de::SpannedError;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::PathBuf;
use thiserror::Error;
use tracing::*;

use crate::{KRUST_APPLICATION, KRUST_ORGANIZATION, KRUST_QUALIFIER};

#[derive(Error, Debug)]
pub enum ExternalError {
    #[error(transparent)]
    ParallelismError(#[from] tokio::task::JoinError),
    #[error(transparent)]
    FileSystemError(#[from] std::io::Error),
    #[error(transparent)]
    DatabaseError(#[from] rusqlite::Error),
    #[error(transparent)]
    DatabaseMigrationError(#[from] rusqlite_migration::Error),
    #[error(transparent)]
    KafkaUnexpectedError(#[from] KafkaError),
    #[error("headers serialization error")]
    HeadersError(#[from] SpannedError),
    #[error("configuration error: `{0}`")]
    ConfigurationError(String),
    #[error("error caching messages for topic {0}, duration: {1}")]
    CachingError(String, String),
    #[error("Error {0}: {1}")]
    DisplayError(String, String),
}

/// Application state that is not intended to be directly configurable by the user. The state is
/// converted to and from JSON, and stored in the platform's application directory. It is not
/// updated during application execution.
///
/// We could use [`gio::Settings`] for this, but for now this is simpler than installing and
/// managing schemas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Width of the main window at startup.
    pub width: i32,

    /// Height of the main window at startup.
    pub height: i32,

    /// Whether the window should be maximized at startup.
    pub is_maximized: bool,
}

impl State {
    /// Read from the state file on disk.
    pub fn read() -> Result<Self, ExternalError> {
        let path = state_path()?;
        serde_json::from_reader(File::open(path).map_err(|e| {
            ExternalError::ConfigurationError(format!("unable to read state: {:?}", e))
        })?)
        .map_err(|e| ExternalError::ConfigurationError(format!("unable to read state: {:?}", e)))
    }

    /// Persist to disk.
    pub fn write(&self) -> Result<(), ExternalError> {
        let path = state_path()?;

        trace!(
            "persisting application state: {:?}, into path: {:?}",
            self,
            path
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
}

impl Default for State {
    fn default() -> Self {
        let width: i32 = 900;
        State {
            width,
            height: 600,
            is_maximized: false,
        }
    }
}
pub fn database_connection() -> Result<Connection, ExternalError> {
    database_connection_with_name(&"application".to_string())
}
pub fn database_connection_with_name(database_name: &String) -> Result<Connection, ExternalError> {
    let path = &ensure_app_config_dir()?;
    let data_file = ensure_path_dir(path)?.join(format!("{}.db", database_name));
    database_connection_from_file(&data_file)
}
pub fn database_connection_with_name_and_path(
    path: &PathBuf,
    database_name: &String,
) -> Result<Connection, ExternalError> {
    let data_file = ensure_path_dir(path)?.join(format!("{}.db", database_name));
    Connection::open_with_flags(
        data_file,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(ExternalError::DatabaseError)
}

pub fn database_connection_from_file(file: &PathBuf) -> Result<Connection, ExternalError> {
    Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(ExternalError::DatabaseError)
}
pub fn backup_database() -> Result<String, ExternalError> {
    backup_database_with_name(&ensure_app_config_dir()?, &"application".to_string())
}
pub fn backup_database_with_name(
    path: &PathBuf,
    database_name: &String,
) -> Result<String, ExternalError> {
    let data_file = ensure_path_dir(path)?.join(format!("{}.db", database_name));
    let backup_name = format!("{}-{}.bak", database_name, Utc::now().timestamp_millis());
    let backup_filename = format!("{}.db", backup_name);
    let backup_file = path.join(backup_filename);
    fs::copy(data_file, backup_file.clone())
        .map(|_| backup_name)
        .map_err(ExternalError::FileSystemError)
}
pub fn destroy_database_with_name(
    path: PathBuf,
    database_name: &String,
) -> Result<(), ExternalError> {
    let data_file = path.join(format!("{}.db", database_name));
    fs::remove_file(data_file).map_err(ExternalError::FileSystemError)
}
pub fn destroy_database() -> Result<(), ExternalError> {
    destroy_database_with_name(ensure_app_config_dir()?, &"application".to_string())
}
pub fn app_config_dir() -> Result<PathBuf, ExternalError> {
    let dirs = ProjectDirs::from(KRUST_QUALIFIER, KRUST_ORGANIZATION, KRUST_APPLICATION)
        .ok_or_else(|| {
            ExternalError::ConfigurationError("unable to find user home directory".into())
        })?;
    Ok(dirs.data_local_dir().to_path_buf())
}

fn state_path() -> Result<PathBuf, ExternalError> {
    Ok(ensure_app_config_dir()?.join("state.json"))
}

pub fn ensure_path_dir(path: &PathBuf) -> Result<PathBuf, ExternalError> {
    trace!("ensuring path: {:?}", path);
    fs::create_dir_all(path).map_err(|op| {
        ExternalError::ConfigurationError(format!(
            "unable to create intermediate directories: {:?}",
            op
        ))
    })?;
    Ok(path.clone())
}

pub fn ensure_app_config_dir() -> Result<PathBuf, ExternalError> {
    let app_config_path = app_config_dir()?;
    trace!("app config path: {:?}", app_config_path);
    ensure_path_dir(&app_config_path)
}
