//! In-memory implementation of the repository and read-model ports.
//! Data lives for the lifetime of the process.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use crate::application::error::AppError;
use crate::application::ports::{
    LogRepositoryReadModel, LogRepositoryRepository, ProjectReadModel, ProjectRepository,
};
use crate::application::views::{LogRepositoryView, ProjectView};
use crate::domain::{LogRepository, LogRepositoryId, Name, Project, ProjectId};

#[derive(Default)]
struct State {
    projects: HashMap<ProjectId, Project>,
    log_repositories: HashMap<LogRepositoryId, LogRepository>,
}

#[derive(Default)]
pub struct InMemoryStore {
    state: Mutex<State>,
}

impl InMemoryStore {
    fn lock(&self) -> Result<MutexGuard<'_, State>, AppError> {
        self.state
            .lock()
            .map_err(|_| AppError::Storage("store lock poisoned".into()))
    }
}

impl ProjectRepository for InMemoryStore {
    fn save(&self, project: &Project) -> Result<(), AppError> {
        self.lock()?.projects.insert(project.id(), project.clone());
        Ok(())
    }

    fn find(&self, id: ProjectId) -> Result<Option<Project>, AppError> {
        Ok(self.lock()?.projects.get(&id).cloned())
    }

    fn name_taken(&self, name: &Name) -> Result<bool, AppError> {
        Ok(self
            .lock()?
            .projects
            .values()
            .any(|p| p.name() == name))
    }
}

impl LogRepositoryRepository for InMemoryStore {
    fn save(&self, repository: &LogRepository) -> Result<(), AppError> {
        self.lock()?
            .log_repositories
            .insert(repository.id(), repository.clone());
        Ok(())
    }

    fn find(&self, id: LogRepositoryId) -> Result<Option<LogRepository>, AppError> {
        Ok(self.lock()?.log_repositories.get(&id).cloned())
    }
}

impl ProjectReadModel for InMemoryStore {
    fn list(&self) -> Result<Vec<ProjectView>, AppError> {
        let mut views: Vec<ProjectView> = self.lock()?.projects.values().map(ProjectView::from).collect();
        views.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(views)
    }

    fn get(&self, id: ProjectId) -> Result<Option<ProjectView>, AppError> {
        Ok(self.lock()?.projects.get(&id).map(ProjectView::from))
    }
}

impl LogRepositoryReadModel for InMemoryStore {
    fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<LogRepositoryView>, AppError> {
        let mut views: Vec<LogRepositoryView> = self
            .lock()?
            .log_repositories
            .values()
            .filter(|r| r.project_id() == project_id)
            .map(LogRepositoryView::from)
            .collect();
        views.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(views)
    }
}
