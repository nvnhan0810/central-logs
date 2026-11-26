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
    #[error("storage error: {0}")]
    Storage(String),
    /// The external identity provider failed or could not be reached.
    #[error("identity provider error: {0}")]
    Upstream(String),
    /// The identity provider rejected the credentials (for example a refresh token that was revoked).
    #[error("unauthorized: {0}")]
    Unauthorized(String),
}
