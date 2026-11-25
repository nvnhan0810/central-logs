//! Write side of CQRS: state-changing use cases.

use std::sync::Arc;

use crate::application::error::AppError;
use crate::application::ports::{LogFileSystem, LogRepositoryRepository, ProjectRepository};
use crate::domain::{LogRepository, LogRepositoryId, Project, ProjectId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProject {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateLogRepository {
    pub project_id: ProjectId,
    pub name: String,
    pub directories: Vec<String>,
}

pub struct CommandService {
    projects: Arc<dyn ProjectRepository>,
    log_repositories: Arc<dyn LogRepositoryRepository>,
    files: Arc<dyn LogFileSystem>,
}

impl CommandService {
    pub fn new(
        projects: Arc<dyn ProjectRepository>,
        log_repositories: Arc<dyn LogRepositoryRepository>,
        files: Arc<dyn LogFileSystem>,
    ) -> Self {
        Self {
            projects,
            log_repositories,
            files,
        }
    }

    pub fn create_project(&self, command: CreateProject) -> Result<ProjectId, AppError> {
        let project = Project::create(ProjectId::new(), &command.name, &command.description)?;
        if self.projects.name_taken(project.name())? {
            return Err(AppError::Conflict(format!(
                "project name already exists: {}",
                project.name().as_str()
            )));
        }
        self.projects.save(&project)?;
        Ok(project.id())
    }

    pub fn create_log_repository(
        &self,
        command: CreateLogRepository,
    ) -> Result<LogRepositoryId, AppError> {
        if self.projects.find(command.project_id)?.is_none() {
            return Err(AppError::NotFound(format!(
                "project {}",
                command.project_id.as_uuid()
            )));
        }
        let directories: Vec<&str> = command.directories.iter().map(String::as_str).collect();
        let repository = LogRepository::create(
            LogRepositoryId::new(),
            command.project_id,
            &command.name,
            &directories,
        )?;
        if let Some(missing) = repository
            .directories()
            .iter()
            .find(|dir| !self.files.is_directory(dir))
        {
            return Err(AppError::Validation(format!(
                "directory does not exist: {}",
                missing.as_path().display()
            )));
        }
        self.log_repositories.save(&repository)?;
        Ok(repository.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::AppError;
    use crate::application::ports::{LogRepositoryRepository, ProjectRepository};
    use crate::application::test_support::harness;
    use crate::domain::DomainError;

    fn new_project(name: &str) -> CreateProject {
        CreateProject {
            name: name.into(),
            description: "desc".into(),
        }
    }

    #[test]
    fn create_project_persists_and_returns_id() {
        let h = harness(&[]);
        let id = h.commands.create_project(new_project("Payments")).unwrap();
        let stored = ProjectRepository::find(h.store.as_ref(), id).unwrap().expect("project saved");
        assert_eq!(stored.name().as_str(), "Payments");
    }

    #[test]
    fn create_project_rejects_duplicate_name() {
        let h = harness(&[]);
        h.commands.create_project(new_project("Payments")).unwrap();
        let err = h.commands.create_project(new_project(" Payments ")).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[test]
    fn create_project_rejects_blank_name() {
        let h = harness(&[]);
        let err = h.commands.create_project(new_project("  ")).unwrap_err();
        assert_eq!(err, AppError::Domain(DomainError::EmptyName));
    }

    #[test]
    fn create_log_repository_requires_existing_project() {
        let h = harness(&["/var/log/api"]);
        let err = h
            .commands
            .create_log_repository(CreateLogRepository {
                project_id: ProjectId::new(),
                name: "api".into(),
                directories: vec!["/var/log/api".into()],
            })
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn create_log_repository_rejects_missing_directory() {
        let h = harness(&["/var/log/api"]);
        let project = h.commands.create_project(new_project("Payments")).unwrap();
        let err = h
            .commands
            .create_log_repository(CreateLogRepository {
                project_id: project,
                name: "api".into(),
                directories: vec!["/var/log/missing".into()],
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn create_log_repository_persists_directories() {
        let h = harness(&["/var/log/api", "/srv/app/logs"]);
        let project = h.commands.create_project(new_project("Payments")).unwrap();
        let repo_id = h
            .commands
            .create_log_repository(CreateLogRepository {
                project_id: project,
                name: "api".into(),
                directories: vec!["/var/log/api".into(), "/srv/app/logs".into()],
            })
            .unwrap();
        let stored = LogRepositoryRepository::find(h.store.as_ref(), repo_id).unwrap().expect("repository saved");
        assert_eq!(stored.project_id(), project);
        assert_eq!(stored.directories().len(), 2);
    }
}
