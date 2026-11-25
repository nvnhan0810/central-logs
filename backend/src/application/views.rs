//! Read-side DTOs returned by queries. They carry no behaviour.

use crate::domain::{LogRepository, LogRepositoryId, Project, ProjectId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectView {
    pub id: ProjectId,
    pub name: String,
    pub description: String,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            id: project.id(),
            name: project.name().as_str().to_owned(),
            description: project.description().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRepositoryView {
    pub id: LogRepositoryId,
    pub project_id: ProjectId,
    pub name: String,
    pub directories: Vec<String>,
}

impl From<&LogRepository> for LogRepositoryView {
    fn from(repo: &LogRepository) -> Self {
        Self {
            id: repo.id(),
            project_id: repo.project_id(),
            name: repo.name().as_str().to_owned(),
            directories: repo
                .directories()
                .iter()
                .map(|d| d.as_path().display().to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub name: String,
    /// Path relative to the log directory, using `/` separators.
    pub relative_path: String,
    pub kind: EntryKind,
    /// Size in bytes for files; `None` for directories.
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDetails {
    pub size: u64,
    /// Last modification time, in seconds since the Unix epoch.
    pub modified_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// UTF-8 text with invalid bytes replaced and the line terminator removed.
    pub text: String,
    /// `true` when the line was cut by the byte budget; the rest is returned by the next page.
    pub truncated: bool,
}

/// One bounded page of a log file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogChunk {
    pub file: FileDetails,
    pub start_offset: u64,
    /// Pass this as `offset` to fetch the following page.
    pub next_offset: u64,
    pub eof: bool,
    pub lines: Vec<LogLine>,
}
