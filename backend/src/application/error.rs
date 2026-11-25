use thiserror::Error;

use crate::domain::DomainError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("{0}")]
    Validation(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("storage error: {0}")]
    Storage(String),
}
