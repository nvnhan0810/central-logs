use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use super::AppState;
use super::dto::{
    CreateLogRepositoryRequest, CreateProjectRequest, EntryResponse, IdResponse, LogChunkResponse,
    LogRepositoryResponse, ProjectResponse,
};
use super::error::ApiError;
use crate::application::commands::{CreateLogRepository, CreateProject};
use crate::application::error::AppError;
use crate::domain::{LogRepositoryId, ProjectId};

/// Runs blocking file-system or lock work off the async runtime.
async fn blocking<T, F>(work: F) -> Result<T, ApiError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result.map_err(ApiError::from),
        Err(_) => Err(ApiError::internal()),
    }
}

fn project_id(raw: &str) -> Result<ProjectId, ApiError> {
    ProjectId::parse(raw).map_err(|e| AppError::from(e).into())
}

fn repository_id(raw: &str) -> Result<LogRepositoryId, ApiError> {
    LogRepositoryId::parse(raw).map_err(|e| AppError::from(e).into())
}

pub async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<IdResponse>), ApiError> {
    let commands = state.commands.clone();
    let id = blocking(move || {
        commands.create_project(CreateProject {
            name: body.name,
            description: body.description.unwrap_or_default(),
        })
    })
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(IdResponse {
            id: id.as_uuid().to_string(),
        }),
    ))
}

pub async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    let queries = state.queries.clone();
    let views = blocking(move || queries.list_projects()).await?;
    Ok(Json(views.into_iter().map(Into::into).collect()))
}

pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let id = project_id(&id)?;
    let queries = state.queries.clone();
    let view = blocking(move || queries.get_project(id)).await?;
    Ok(Json(view.into()))
}

pub async fn create_log_repository(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<CreateLogRepositoryRequest>,
) -> Result<(StatusCode, Json<IdResponse>), ApiError> {
    let project_id = project_id(&project)?;
    let commands = state.commands.clone();
    let id = blocking(move || {
        commands.create_log_repository(CreateLogRepository {
            project_id,
            name: body.name,
            directories: body.directories,
        })
    })
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(IdResponse {
            id: id.as_uuid().to_string(),
        }),
    ))
}

pub async fn list_log_repositories(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> Result<Json<Vec<LogRepositoryResponse>>, ApiError> {
    let project_id = project_id(&project)?;
    let queries = state.queries.clone();
    let views = blocking(move || queries.list_log_repositories(project_id)).await?;
    Ok(Json(views.into_iter().map(Into::into).collect()))
}

#[derive(Debug, Deserialize)]
pub struct BrowseParams {
    pub directory: String,
    #[serde(default)]
    pub path: String,
}

pub async fn browse_entries(
    State(state): State<AppState>,
    Path(repo): Path<String>,
    Query(params): Query<BrowseParams>,
) -> Result<Json<Vec<EntryResponse>>, ApiError> {
    let repo_id = repository_id(&repo)?;
    let queries = state.queries.clone();
    let entries =
        blocking(move || queries.browse(repo_id, &params.directory, &params.path)).await?;
    Ok(Json(entries.into_iter().map(Into::into).collect()))
}

#[derive(Debug, Deserialize)]
pub struct ReadParams {
    pub directory: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub offset: u64,
    pub max_lines: Option<usize>,
    pub max_bytes: Option<usize>,
}

pub async fn read_file(
    State(state): State<AppState>,
    Path(repo): Path<String>,
    Query(params): Query<ReadParams>,
) -> Result<Json<LogChunkResponse>, ApiError> {
    let repo_id = repository_id(&repo)?;
    let queries = state.queries.clone();
    let chunk = blocking(move || {
        queries.read_file(
            repo_id,
            &params.directory,
            &params.path,
            params.offset,
            params.max_lines,
            params.max_bytes,
        )
    })
    .await?;
    Ok(Json(chunk.into()))
}
