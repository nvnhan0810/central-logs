use thiserror::Error;

/// Violations of domain invariants.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("name must be at most {max} characters")]
    NameTooLong { max: usize },
    #[error("directory must be an absolute path: {0}")]
    RelativeDirectory(String),
    #[error("path must not contain '..': {0}")]
    ParentTraversal(String),
    #[error("log repository must have at least one directory")]
    NoDirectories,
    #[error("directory is already registered: {0}")]
    DuplicateDirectory(String),
    #[error("log path must be relative and stay inside its directory: {0}")]
    InvalidLogPath(String),
    #[error("invalid identifier: {0}")]
    InvalidId(String),
}
