//! Read side of CQRS: use cases that never change state.

use std::sync::Arc;

use crate::application::error::AppError;
use crate::application::ports::{
    LogFileSystem, LogRepositoryReadModel, LogRepositoryRepository, ProjectReadModel, ReadWindow,
};
use crate::application::views::{LogChunk, LogEntry, LogRepositoryView, ProjectView};
use crate::domain::{LogDirectory, LogRepositoryId, ProjectId, RelativeLogPath};

pub struct QueryService {
    projects: Arc<dyn ProjectReadModel>,
    log_repository_views: Arc<dyn LogRepositoryReadModel>,
    log_repositories: Arc<dyn LogRepositoryRepository>,
    files: Arc<dyn LogFileSystem>,
}

impl QueryService {
    pub fn new(
        projects: Arc<dyn ProjectReadModel>,
        log_repository_views: Arc<dyn LogRepositoryReadModel>,
        log_repositories: Arc<dyn LogRepositoryRepository>,
        files: Arc<dyn LogFileSystem>,
    ) -> Self {
        Self {
            projects,
            log_repository_views,
            log_repositories,
            files,
        }
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectView>, AppError> {
        self.projects.list()
    }

    pub fn get_project(&self, id: ProjectId) -> Result<ProjectView, AppError> {
        self.projects
            .get(id)?
            .ok_or_else(|| AppError::NotFound(format!("project {}", id.as_uuid())))
    }

    pub fn list_log_repositories(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<LogRepositoryView>, AppError> {
        self.get_project(project_id)?;
        self.log_repository_views.list_by_project(project_id)
    }

    /// Lists the entries of `path` inside one directory of a log repository.
    pub fn browse(
        &self,
        repository_id: LogRepositoryId,
        directory: &str,
        path: &str,
    ) -> Result<Vec<LogEntry>, AppError> {
        let (directory, path) = self.resolve(repository_id, directory, path)?;
        self.files.list_entries(&directory, &path)
    }

    /// Reads one bounded page of a log file. Never loads the whole file.
    pub fn read_file(
        &self,
        repository_id: LogRepositoryId,
        directory: &str,
        path: &str,
        offset: u64,
        max_lines: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<LogChunk, AppError> {
        let window = ReadWindow::new(max_lines, max_bytes)?;
        let (directory, path) = self.resolve(repository_id, directory, path)?;
        if path.is_root() {
            return Err(AppError::Validation("path must point to a file".into()));
        }
        self.files.read_chunk(&directory, &path, offset, window)
    }

    /// Ensures the requested directory belongs to the repository before touching the file system.
    fn resolve(
        &self,
        repository_id: LogRepositoryId,
        directory: &str,
        path: &str,
    ) -> Result<(LogDirectory, RelativeLogPath), AppError> {
        let repository = self
            .log_repositories
            .find(repository_id)?
            .ok_or_else(|| AppError::NotFound(format!("log repository {}", repository_id.as_uuid())))?;
        let directory = LogDirectory::parse(directory)?;
        if !repository.contains_directory(&directory) {
            return Err(AppError::NotFound(format!(
                "directory {} in this log repository",
                directory.as_path().display()
            )));
        }
        let path = RelativeLogPath::parse(path)?;
        Ok((directory, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::commands::{CreateLogRepository, CreateProject};
    use crate::application::ports::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, MAX_BYTES_LIMIT, MAX_LINES_LIMIT};
    use crate::application::test_support::{harness, Harness};

    struct Seeded {
        h: Harness,
        project: ProjectId,
        repo: LogRepositoryId,
    }

    fn seeded() -> Seeded {
        let h = harness(&["/var/log/api"]);
        let project = h
            .commands
            .create_project(CreateProject {
                name: "Payments".into(),
                description: "".into(),
            })
            .unwrap();
        let repo = h
            .commands
            .create_log_repository(CreateLogRepository {
                project_id: project,
                name: "api".into(),
                directories: vec!["/var/log/api".into()],
            })
            .unwrap();
        Seeded { h, project, repo }
    }

    #[test]
    fn lists_projects_sorted_by_name() {
        let s = seeded();
        s.h.commands
            .create_project(CreateProject {
                name: "Accounts".into(),
                description: "".into(),
            })
            .unwrap();
        let names: Vec<String> = s
            .h
            .queries
            .list_projects()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["Accounts", "Payments"]);
    }

    #[test]
    fn get_unknown_project_is_not_found() {
        let s = seeded();
        let err = s.h.queries.get_project(ProjectId::new()).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn lists_log_repositories_of_project() {
        let s = seeded();
        let repos = s.h.queries.list_log_repositories(s.project).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].id, s.repo);
        assert_eq!(repos[0].directories, vec!["/var/log/api".to_string()]);
    }

    #[test]
    fn browse_rejects_directory_not_owned_by_repository() {
        let s = seeded();
        let err = s
            .h
            .queries
            .browse(s.repo, "/etc", "")
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn browse_rejects_path_traversal() {
        let s = seeded();
        let err = s
            .h
            .queries
            .browse(s.repo, "/var/log/api", "../secret")
            .unwrap_err();
        assert!(matches!(err, AppError::Domain(_)));
    }

    #[test]
    fn browse_returns_entries_for_owned_directory() {
        let s = seeded();
        let entries = s.h.queries.browse(s.repo, "/var/log/api", "").unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn read_file_rejects_directory_root_as_file() {
        let s = seeded();
        let err = s
            .h
            .queries
            .read_file(s.repo, "/var/log/api", "", 0, None, None)
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn read_file_clamps_window_to_hard_limits() {
        let s = seeded();
        s.h.queries
            .read_file(s.repo, "/var/log/api", "app.log", 0, Some(usize::MAX), Some(usize::MAX))
            .unwrap();
        let window = s.h.files.last_window.lock().unwrap().expect("window recorded");
        assert_eq!(window.max_lines, MAX_LINES_LIMIT);
        assert_eq!(window.max_bytes, MAX_BYTES_LIMIT);
    }

    #[test]
    fn read_file_applies_defaults_when_limits_absent() {
        let s = seeded();
        s.h.queries
            .read_file(s.repo, "/var/log/api", "app.log", 0, None, None)
            .unwrap();
        let window = s.h.files.last_window.lock().unwrap().expect("window recorded");
        assert_eq!(window.max_lines, DEFAULT_MAX_LINES);
        assert_eq!(window.max_bytes, DEFAULT_MAX_BYTES);
    }

    #[test]
    fn read_file_rejects_zero_line_limit() {
        let s = seeded();
        let err = s
            .h
            .queries
            .read_file(s.repo, "/var/log/api", "app.log", 0, Some(0), None)
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
