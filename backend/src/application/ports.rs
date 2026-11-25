//! Ports: interfaces the application needs from the outside world.

use crate::application::error::AppError;
use crate::application::views::{LogChunk, LogEntry, LogRepositoryView, ProjectView};
use crate::domain::{
    LogDirectory, LogRepository, LogRepositoryId, Name, Project, ProjectId, RelativeLogPath,
};

pub const DEFAULT_MAX_LINES: usize = 200;
pub const MAX_LINES_LIMIT: usize = 5_000;
pub const DEFAULT_MAX_BYTES: usize = 256 * 1024;
pub const MAX_BYTES_LIMIT: usize = 4 * 1024 * 1024;

/// Upper bounds for a single page read, so memory use is bounded regardless of file size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadWindow {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl ReadWindow {
    pub fn new(max_lines: Option<usize>, max_bytes: Option<usize>) -> Result<Self, AppError> {
        let max_lines = max_lines.unwrap_or(DEFAULT_MAX_LINES);
        let max_bytes = max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
        if max_lines == 0 {
            return Err(AppError::Validation("limit must be at least 1".into()));
        }
        if max_bytes == 0 {
            return Err(AppError::Validation("max_bytes must be at least 1".into()));
        }
        Ok(Self {
            max_lines: max_lines.min(MAX_LINES_LIMIT),
            max_bytes: max_bytes.min(MAX_BYTES_LIMIT),
        })
    }
}

// ---- Write side -------------------------------------------------------------

pub trait ProjectRepository: Send + Sync {
    fn save(&self, project: &Project) -> Result<(), AppError>;
    fn find(&self, id: ProjectId) -> Result<Option<Project>, AppError>;
    fn name_taken(&self, name: &Name) -> Result<bool, AppError>;
}

pub trait LogRepositoryRepository: Send + Sync {
    fn save(&self, repository: &LogRepository) -> Result<(), AppError>;
    fn find(&self, id: LogRepositoryId) -> Result<Option<LogRepository>, AppError>;
}

// ---- Read side --------------------------------------------------------------

pub trait ProjectReadModel: Send + Sync {
    fn list(&self) -> Result<Vec<ProjectView>, AppError>;
    fn get(&self, id: ProjectId) -> Result<Option<ProjectView>, AppError>;
}

pub trait LogRepositoryReadModel: Send + Sync {
    fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<LogRepositoryView>, AppError>;
}

// ---- Log files --------------------------------------------------------------

pub trait LogFileSystem: Send + Sync {
    /// Whether the directory exists and is a directory on the host.
    fn is_directory(&self, directory: &LogDirectory) -> bool;

    /// Lists the direct children of `path` inside `directory`.
    fn list_entries(
        &self,
        directory: &LogDirectory,
        path: &RelativeLogPath,
    ) -> Result<Vec<LogEntry>, AppError>;

    /// Reads one bounded page of a log file starting at byte `offset`.
    fn read_chunk(
        &self,
        directory: &LogDirectory,
        path: &RelativeLogPath,
        offset: u64,
        window: ReadWindow,
    ) -> Result<LogChunk, AppError>;
}
