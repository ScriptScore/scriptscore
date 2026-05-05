// SPDX-License-Identifier: AGPL-3.0-only
use thiserror::Error;

pub type HostResult<T> = Result<T, HostError>;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Protocol(String),
    #[error("{0}")]
    Worker(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Project(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
