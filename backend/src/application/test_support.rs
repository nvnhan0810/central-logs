//! Shared fixtures for application-layer tests.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::application::commands::CommandService;
use crate::application::error::AppError;
use crate::application::ports::{LogFileSystem, ReadWindow};
use crate::application::queries::QueryService;
use crate::application::views::{EntryKind, FileDetails, LogChunk, LogEntry};
use crate::domain::{LogDirectory, RelativeLogPath};
use crate::infrastructure::memory::InMemoryStore;

/// File system double: knows which directories exist and records the last read window.
#[derive(Default)]
pub struct FakeFiles {
    dirs: HashSet<PathBuf>,
    pub last_window: Mutex<Option<ReadWindow>>,
}

impl FakeFiles {
    pub fn with_dirs(dirs: &[&str]) -> Self {
        Self {
            dirs: dirs.iter().map(PathBuf::from).collect(),
            last_window: Mutex::new(None),
        }
    }
}

impl LogFileSystem for FakeFiles {
    fn is_directory(&self, directory: &LogDirectory) -> bool {
        self.dirs.contains(directory.as_path())
    }

    fn list_entries(
        &self,
        _directory: &LogDirectory,
        _path: &RelativeLogPath,
    ) -> Result<Vec<LogEntry>, AppError> {
        Ok(vec![LogEntry {
            name: "app.log".into(),
            relative_path: "app.log".into(),
            kind: EntryKind::File,
            size: Some(10),
        }])
    }

    fn read_chunk(
        &self,
        _directory: &LogDirectory,
        _path: &RelativeLogPath,
        offset: u64,
        window: ReadWindow,
    ) -> Result<LogChunk, AppError> {
        *self.last_window.lock().unwrap() = Some(window);
        Ok(LogChunk {
            file: FileDetails {
                size: 10,
                modified_unix: None,
            },
            start_offset: offset,
            next_offset: offset,
            eof: true,
            lines: vec![],
        })
    }
}

pub struct Harness {
    pub store: Arc<InMemoryStore>,
    pub files: Arc<FakeFiles>,
    pub commands: CommandService,
    pub queries: QueryService,
}

pub fn harness(existing_dirs: &[&str]) -> Harness {
    let store = Arc::new(InMemoryStore::default());
    let files = Arc::new(FakeFiles::with_dirs(existing_dirs));
    let commands = CommandService::new(store.clone(), store.clone(), files.clone());
    let queries = QueryService::new(store.clone(), store.clone(), store.clone(), files.clone());
    Harness {
        store,
        files,
        commands,
        queries,
    }
}
