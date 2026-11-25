//! Domain layer: pure business rules, no I/O and no framework dependencies.

pub mod error;
pub mod log_path;
pub mod log_repository;
pub mod name;
pub mod project;

pub use error::DomainError;
pub use log_path::{LogDirectory, RelativeLogPath};
pub use log_repository::{LogRepository, LogRepositoryId};
pub use name::Name;
pub use project::{Project, ProjectId};
