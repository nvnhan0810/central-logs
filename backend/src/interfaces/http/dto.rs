use serde::{Deserialize, Serialize};

use crate::application::views::{
    EntryKind, FileDetails, LogChunk, LogEntry, LogLine, LogRepositoryView, ProjectView,
};

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLogRepositoryRequest {
    pub name: String,
    pub directories: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct IdResponse {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl From<ProjectView> for ProjectResponse {
    fn from(v: ProjectView) -> Self {
        Self {
            id: v.id.as_uuid().to_string(),
            name: v.name,
            description: v.description,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LogRepositoryResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub directories: Vec<String>,
}

impl From<LogRepositoryView> for LogRepositoryResponse {
    fn from(v: LogRepositoryView) -> Self {
        Self {
            id: v.id.as_uuid().to_string(),
            project_id: v.project_id.as_uuid().to_string(),
            name: v.name,
            directories: v.directories,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EntryResponse {
    pub name: String,
    pub relative_path: String,
    pub kind: &'static str,
    pub size: Option<u64>,
}

impl From<LogEntry> for EntryResponse {
    fn from(e: LogEntry) -> Self {
        Self {
            name: e.name,
            relative_path: e.relative_path,
            kind: match e.kind {
                EntryKind::File => "file",
                EntryKind::Directory => "directory",
            },
            size: e.size,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileDetailsResponse {
    pub size: u64,
    pub modified_unix: Option<u64>,
}

impl From<FileDetails> for FileDetailsResponse {
    fn from(f: FileDetails) -> Self {
        Self {
            size: f.size,
            modified_unix: f.modified_unix,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LogLineResponse {
    pub text: String,
    pub truncated: bool,
}

impl From<LogLine> for LogLineResponse {
    fn from(l: LogLine) -> Self {
        Self {
            text: l.text,
            truncated: l.truncated,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LogChunkResponse {
    pub file: FileDetailsResponse,
    pub start_offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub lines: Vec<LogLineResponse>,
}

impl From<LogChunk> for LogChunkResponse {
    fn from(c: LogChunk) -> Self {
        Self {
            file: c.file.into(),
            start_offset: c.start_offset,
            next_offset: c.next_offset,
            eof: c.eof,
            lines: c.lines.into_iter().map(Into::into).collect(),
        }
    }
}
