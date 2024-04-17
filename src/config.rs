use directories::ProjectDirs;
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
    FileSystemError(#[from] std::io::Error),
    #[error(transparent)]
    DatabaseError(#[from] rusqlite::Error),
    #[error("headers serialization error")]
    HeadersError(#[from] SpannedError),
    #[error("configuration error: `{0}`")]
    ConfigurationError(String),
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

    /// Panned separator position
    pub separator_position: i32,

    /// Whether the window should be maximized at startup.
    pub is_maximized: bool,
}

impl State {
    /// Read from the state file on disk.
    pub fn read() -> Result<Self, ExternalError> {
        let path = state_path()?;
        Ok(serde_json::from_reader(File::open(path).map_err(|e| {
            ExternalError::ConfigurationError(format!("unable to read state: {:?}", e).into())
        })?)
        .map_err(|e| {
            ExternalError::ConfigurationError(format!("unable to read state: {:?}", e).into())
        })?)
    }

    /// Persist to disk.
    pub fn write(&self) -> Result<(), ExternalError> {
        let path = state_path()?;

        info!(
            "persisting application state: {:?}, into path: {:?}",
            self, path
        );

        let file = File::create(path).map_err(|op| {
            ExternalError::ConfigurationError(
                format!("unable to create intermediate directories: {:?}", op).into(),
            )
        })?;
        Ok(serde_json::to_writer(file, self).map_err(|op| {
            ExternalError::ConfigurationError(
                format!("unable to create to write state to disk: {:?}", op).into(),
            )
        })?)
    }
}

impl Default for State {
    fn default() -> Self {
        let width: i32 = 900;
        State {
            width: width,
            height: 600,
            separator_position: ((width as f32) * 0.25).round() as i32,
            is_maximized: false,
        }
    }
}

pub fn database_connection() -> Result<Connection, ExternalError> {
    database_connection_with_name(&ensure_app_config_dir()?,&"application".to_string())
}
pub fn database_connection_with_name(path: &PathBuf, database_name: &String) -> Result<Connection, ExternalError> {
    let data_file = ensure_path_dir(path)?.join(format!("{}.db", database_name));
    Connection::open_with_flags(
        data_file,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(ExternalError::DatabaseError)
}
pub fn destroy_database_with_name(path: PathBuf, database_name: &String) -> Result<(), ExternalError> {
    let data_file = path.join(format!("{}.db", database_name));
    fs::remove_file(data_file)
        .map_err(ExternalError::FileSystemError)

}

pub fn app_config_dir() -> Result<PathBuf, ExternalError> {
    let dirs = ProjectDirs::from(KRUST_QUALIFIER, KRUST_ORGANIZATION, KRUST_APPLICATION).ok_or_else(|| {
        ExternalError::ConfigurationError("unable to find user home directory".into())
    })?;
    Ok(dirs.data_local_dir().to_path_buf())
}

fn state_path() -> Result<PathBuf, ExternalError> {
    Ok(ensure_app_config_dir()?.join("state.json"))
}

pub fn ensure_path_dir(path: &PathBuf) -> Result<PathBuf, ExternalError> {
    info!("ensuring path: {:?}", path);
    fs::create_dir_all(&path).map_err(|op| {
        ExternalError::ConfigurationError(
            format!("unable to create intermediate directories: {:?}", op).into(),
        )
    })?;
    Ok(path.clone())
}

pub fn ensure_app_config_dir() -> Result<PathBuf, ExternalError> {
    let app_config_path = app_config_dir()?;
    info!("app config path: {:?}", app_config_path);
    ensure_path_dir(&app_config_path)
}
