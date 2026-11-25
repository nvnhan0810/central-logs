//! Repository and read-model ports implemented on PostgreSQL.

use std::future::Future;
use std::time::Duration;

use sqlx::Row;
use sqlx::postgres::{PgPoolOptions, PgRow};
use tokio::runtime::Handle;
use uuid::Uuid;

use super::DbConfig;
use crate::application::error::AppError;
use crate::application::ports::{
    LogRepositoryReadModel, LogRepositoryRepository, ProjectReadModel, ProjectRepository,
};
use crate::application::views::{LogRepositoryView, ProjectView};
use crate::domain::{LogRepository, LogRepositoryId, Name, Project, ProjectId};

pub type PgPool = sqlx::PgPool;

const POOL_SIZE: u32 = 8;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Opens the connection pool. Fails if the server cannot be reached within the timeout.
pub async fn connect(config: &DbConfig) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(POOL_SIZE)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(config.connect_options())
        .await
}

pub struct PgStore {
    pool: PgPool,
    runtime: Handle,
}

impl PgStore {
    /// `runtime` is the tokio runtime that owns the pool. The ports are synchronous, so each
    /// query is driven to completion with `runtime.block_on`. Call the store from a
    /// `spawn_blocking` task or a plain thread, never from an async task.
    pub fn new(pool: PgPool, runtime: Handle) -> Self {
        Self { pool, runtime }
    }

    fn run<T>(&self, query: impl Future<Output = Result<T, sqlx::Error>>) -> Result<T, AppError> {
        self.runtime.block_on(query).map_err(storage)
    }
}

fn storage(error: sqlx::Error) -> AppError {
    AppError::Storage(error.to_string())
}

/// Maps a failed write. A unique-constraint violation becomes a conflict, so a race between
/// the name check and the insert still reports a duplicate name.
fn write_error(error: sqlx::Error, conflict: impl FnOnce() -> String) -> AppError {
    match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Conflict(conflict()),
        _ => storage(error),
    }
}

fn project_id_of(id: Uuid) -> Result<ProjectId, AppError> {
    Ok(ProjectId::parse(&id.to_string())?)
}

fn repository_id_of(id: Uuid) -> Result<LogRepositoryId, AppError> {
    Ok(LogRepositoryId::parse(&id.to_string())?)
}

fn project_from_row(row: &PgRow) -> Result<Project, AppError> {
    let id = project_id_of(row.try_get("id").map_err(storage)?)?;
    let name: String = row.try_get("name").map_err(storage)?;
    let description: String = row.try_get("description").map_err(storage)?;
    Ok(Project::create(id, &name, &description)?)
}

impl ProjectRepository for PgStore {
    fn save(&self, project: &Project) -> Result<(), AppError> {
        let name = project.name().as_str();
        self.runtime
            .block_on(
                sqlx::query(
                    "INSERT INTO projects (id, name, description) VALUES ($1, $2, $3)
                     ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, description = EXCLUDED.description",
                )
                .bind(project.id().as_uuid())
                .bind(name)
                .bind(project.description())
                .execute(&self.pool),
            )
            .map(|_| ())
            .map_err(|e| write_error(e, || format!("project name already exists: {name}")))
    }

    fn find(&self, id: ProjectId) -> Result<Option<Project>, AppError> {
        let row = self.run(
            sqlx::query("SELECT id, name, description FROM projects WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool),
        )?;
        row.as_ref().map(project_from_row).transpose()
    }

    fn name_taken(&self, name: &Name) -> Result<bool, AppError> {
        self.run(
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM projects WHERE name = $1)")
                .bind(name.as_str())
                .fetch_one(&self.pool),
        )
    }
}

impl LogRepositoryRepository for PgStore {
    fn save(&self, repository: &LogRepository) -> Result<(), AppError> {
        // Directory positions are stored as INTEGER, so the count must fit in one.
        let directories = repository
            .directories()
            .iter()
            .enumerate()
            .map(|(position, directory)| {
                let position = i32::try_from(position)
                    .map_err(|_| AppError::Storage("too many directories".into()))?;
                Ok((position, directory.as_path().to_string_lossy().into_owned()))
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        self.run(save_repository(&self.pool, repository, &directories))
    }

    fn find(&self, id: LogRepositoryId) -> Result<Option<LogRepository>, AppError> {
        let key = id.as_uuid();
        let Some(row) = self.run(
            sqlx::query("SELECT id, project_id, name FROM log_repositories WHERE id = $1")
                .bind(key)
                .fetch_optional(&self.pool),
        )?
        else {
            return Ok(None);
        };
        let directories: Vec<String> = self.run(
            sqlx::query_scalar(
                "SELECT path FROM log_repository_directories
                 WHERE log_repository_id = $1 ORDER BY position",
            )
            .bind(key)
            .fetch_all(&self.pool),
        )?;
        let directory_refs: Vec<&str> = directories.iter().map(String::as_str).collect();
        let name: String = row.try_get("name").map_err(storage)?;
        let repository = LogRepository::create(
            repository_id_of(row.try_get("id").map_err(storage)?)?,
            project_id_of(row.try_get("project_id").map_err(storage)?)?,
            &name,
            &directory_refs,
        )?;
        Ok(Some(repository))
    }
}

/// Writes the repository row and replaces its directories in one transaction.
async fn save_repository(
    pool: &PgPool,
    repository: &LogRepository,
    directories: &[(i32, String)],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id = repository.id().as_uuid();
    sqlx::query(
        "INSERT INTO log_repositories (id, project_id, name) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET project_id = EXCLUDED.project_id, name = EXCLUDED.name",
    )
    .bind(id)
    .bind(repository.project_id().as_uuid())
    .bind(repository.name().as_str())
    .execute(&mut *tx)
    .await?;
    // Directories are replaced as a set: simpler than diffing, and the list is short.
    sqlx::query("DELETE FROM log_repository_directories WHERE log_repository_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (position, path) in directories {
        sqlx::query(
            "INSERT INTO log_repository_directories (log_repository_id, position, path)
             VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(position)
        .bind(path)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

impl ProjectReadModel for PgStore {
    fn list(&self) -> Result<Vec<ProjectView>, AppError> {
        // `COLLATE "C"` sorts by bytes, matching the in-memory store and keeping order locale-independent.
        let rows = self.run(
            sqlx::query("SELECT id, name, description FROM projects ORDER BY name COLLATE \"C\"")
                .fetch_all(&self.pool),
        )?;
        rows.iter()
            .map(|row| project_from_row(row).map(|p| ProjectView::from(&p)))
            .collect()
    }

    fn get(&self, id: ProjectId) -> Result<Option<ProjectView>, AppError> {
        Ok(ProjectRepository::find(self, id)?
            .as_ref()
            .map(ProjectView::from))
    }
}

impl LogRepositoryReadModel for PgStore {
    fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<LogRepositoryView>, AppError> {
        let rows = self.run(
            sqlx::query(
                "SELECT r.id, r.project_id, r.name,
                        COALESCE(array_agg(d.path ORDER BY d.position)
                                 FILTER (WHERE d.path IS NOT NULL), '{}') AS directories
                 FROM log_repositories r
                 LEFT JOIN log_repository_directories d ON d.log_repository_id = r.id
                 WHERE r.project_id = $1
                 GROUP BY r.id
                 ORDER BY r.name COLLATE \"C\"",
            )
            .bind(project_id.as_uuid())
            .fetch_all(&self.pool),
        )?;
        rows.iter()
            .map(|row| {
                Ok(LogRepositoryView {
                    id: repository_id_of(row.try_get("id").map_err(storage)?)?,
                    project_id: project_id_of(row.try_get("project_id").map_err(storage)?)?,
                    name: row.try_get("name").map_err(storage)?,
                    directories: row.try_get("directories").map_err(storage)?,
                })
            })
            .collect()
    }
}
