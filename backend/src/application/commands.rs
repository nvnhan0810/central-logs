//! Write side of CQRS: state-changing use cases.

use std::sync::Arc;

use crate::application::error::AppError;
use crate::application::ports::{LogFileSystem, LogRepositoryRepository, ProjectRepository};
use crate::application::views::{LogRepositoryView, ProjectView};
use crate::domain::{LogRepository, LogRepositoryId, Project, ProjectId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProject {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateProject {
    pub id: ProjectId,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateLogRepository {
    pub project_id: ProjectId,
    pub name: String,
    pub directories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateLogRepository {
    pub id: LogRepositoryId,
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
        self.projects.save(&project)?;
        Ok(project.id())
    }

    pub fn update_project(&self, command: UpdateProject) -> Result<ProjectView, AppError> {
        self.require_project(command.id)?;
        let project = Project::create(command.id, &command.name, &command.description)?;
        self.projects.save(&project)?;
        Ok(ProjectView::from(&project))
    }

    /// Deletes the project together with its log repositories.
    pub fn delete_project(&self, id: ProjectId) -> Result<(), AppError> {
        self.require_project(id)?;
        self.projects.delete(id)
    }

    pub fn create_log_repository(
        &self,
        command: CreateLogRepository,
    ) -> Result<LogRepositoryId, AppError> {
        self.require_project(command.project_id)?;
        let repository = self.build_repository(
            LogRepositoryId::new(),
            command.project_id,
            &command.name,
            &command.directories,
        )?;
        self.log_repositories.save(&repository)?;
        Ok(repository.id())
    }

    pub fn update_log_repository(
        &self,
        command: UpdateLogRepository,
    ) -> Result<LogRepositoryView, AppError> {
        let existing = self.require_repository(command.id)?;
        let repository = self.build_repository(
            command.id,
            existing.project_id(),
            &command.name,
            &command.directories,
        )?;
        self.log_repositories.save(&repository)?;
        Ok(LogRepositoryView::from(&repository))
    }

    pub fn delete_log_repository(&self, id: LogRepositoryId) -> Result<(), AppError> {
        self.require_repository(id)?;
        self.log_repositories.delete(id)
    }

    fn require_project(&self, id: ProjectId) -> Result<Project, AppError> {
        self.projects
            .find(id)?
            .ok_or_else(|| AppError::NotFound(format!("project {}", id.as_uuid())))
    }

    fn require_repository(&self, id: LogRepositoryId) -> Result<LogRepository, AppError> {
        self.log_repositories
            .find(id)?
            .ok_or_else(|| AppError::NotFound(format!("log repository {}", id.as_uuid())))
    }

    /// Validates the input and checks that every directory exists on the host.
    fn build_repository(
        &self,
        id: LogRepositoryId,
        project_id: ProjectId,
        name: &str,
        directories: &[String],
    ) -> Result<LogRepository, AppError> {
        let directories: Vec<&str> = directories.iter().map(String::as_str).collect();
        let repository = LogRepository::create(id, project_id, name, &directories)?;
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
        Ok(repository)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::AppError;
    use crate::application::ports::{LogRepositoryRepository, ProjectRepository};
    use crate::application::test_support::{Harness, harness};
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
        let stored = ProjectRepository::find(h.store.as_ref(), id)
            .unwrap()
            .expect("project saved");
        assert_eq!(stored.name().as_str(), "Payments");
    }

    #[test]
    fn create_project_allows_names_already_in_use() {
        let h = harness(&[]);
        let first = h.commands.create_project(new_project("Payments")).unwrap();
        let second = h
            .commands
            .create_project(new_project(" Payments "))
            .unwrap();
        assert_ne!(first, second);
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
        let stored = LogRepositoryRepository::find(h.store.as_ref(), repo_id)
            .unwrap()
            .expect("repository saved");
        assert_eq!(stored.project_id(), project);
        assert_eq!(stored.directories().len(), 2);
    }

    fn create_repo(h: &Harness, project: ProjectId, dirs: &[&str]) -> LogRepositoryId {
        h.commands
            .create_log_repository(CreateLogRepository {
                project_id: project,
                name: "api".into(),
                directories: dirs.iter().map(|d| d.to_string()).collect(),
            })
            .unwrap()
    }

    #[test]
    fn update_project_keeps_id_and_replaces_fields() {
        let h = harness(&[]);
        let id = h.commands.create_project(new_project("Payments")).unwrap();
        let view = h
            .commands
            .update_project(UpdateProject {
                id,
                name: " Billing ".into(),
                description: "invoices".into(),
            })
            .unwrap();
        assert_eq!(view.id, id);
        assert_eq!(view.name, "Billing");
        assert_eq!(view.description, "invoices");
        let stored = ProjectRepository::find(h.store.as_ref(), id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.name().as_str(), "Billing");
    }

    #[test]
    fn update_project_requires_existing_project() {
        let h = harness(&[]);
        let err = h
            .commands
            .update_project(UpdateProject {
                id: ProjectId::new(),
                name: "Payments".into(),
                description: "".into(),
            })
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn delete_project_removes_its_log_repositories() {
        let h = harness(&["/var/log/api"]);
        let project = h.commands.create_project(new_project("Payments")).unwrap();
        let repo = create_repo(&h, project, &["/var/log/api"]);
        h.commands.delete_project(project).unwrap();
        assert!(
            ProjectRepository::find(h.store.as_ref(), project)
                .unwrap()
                .is_none()
        );
        assert!(
            LogRepositoryRepository::find(h.store.as_ref(), repo)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn delete_project_requires_existing_project() {
        let h = harness(&[]);
        let err = h.commands.delete_project(ProjectId::new()).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn update_log_repository_keeps_its_project_and_checks_directories() {
        let h = harness(&["/var/log/api", "/srv/app/logs"]);
        let project = h.commands.create_project(new_project("Payments")).unwrap();
        let repo = create_repo(&h, project, &["/var/log/api"]);

        let view = h
            .commands
            .update_log_repository(UpdateLogRepository {
                id: repo,
                name: "api-v2".into(),
                directories: vec!["/srv/app/logs".into()],
            })
            .unwrap();
        assert_eq!(view.id, repo);
        assert_eq!(view.project_id, project);
        assert_eq!(view.name, "api-v2");
        assert_eq!(view.directories, vec!["/srv/app/logs"]);

        let err = h
            .commands
            .update_log_repository(UpdateLogRepository {
                id: repo,
                name: "api-v2".into(),
                directories: vec!["/var/log/missing".into()],
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn update_and_delete_log_repository_require_existing_repository() {
        let h = harness(&["/var/log/api"]);
        let missing = LogRepositoryId::new();
        let update = h
            .commands
            .update_log_repository(UpdateLogRepository {
                id: missing,
                name: "api".into(),
                directories: vec!["/var/log/api".into()],
            })
            .unwrap_err();
        assert!(matches!(update, AppError::NotFound(_)));
        let delete = h.commands.delete_log_repository(missing).unwrap_err();
        assert!(matches!(delete, AppError::NotFound(_)));
    }

    #[test]
    fn delete_log_repository_removes_only_that_repository() {
        let h = harness(&["/var/log/api"]);
        let project = h.commands.create_project(new_project("Payments")).unwrap();
        let gone = create_repo(&h, project, &["/var/log/api"]);
        let kept = create_repo(&h, project, &["/var/log/api"]);
        h.commands.delete_log_repository(gone).unwrap();
        assert!(
            LogRepositoryRepository::find(h.store.as_ref(), gone)
                .unwrap()
                .is_none()
        );
        assert!(
            LogRepositoryRepository::find(h.store.as_ref(), kept)
                .unwrap()
                .is_some()
        );
    }
}
