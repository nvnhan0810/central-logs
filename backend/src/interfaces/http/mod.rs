//! HTTP API (axum). Handlers are thin: parse input, call a use case, map output.

mod dto;
mod error;
mod handlers;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::application::commands::CommandService;
use crate::application::queries::QueryService;
use crate::infrastructure::fs::FsLogFileSystem;
use crate::infrastructure::memory::InMemoryStore;

pub use error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub commands: Arc<CommandService>,
    pub queries: Arc<QueryService>,
}

impl AppState {
    /// Wires the default adapters: in-memory metadata store and the host file system.
    pub fn in_memory() -> Self {
        let store = Arc::new(InMemoryStore::default());
        let files = Arc::new(FsLogFileSystem);
        Self {
            commands: Arc::new(CommandService::new(
                store.clone(),
                store.clone(),
                files.clone(),
            )),
            queries: Arc::new(QueryService::new(
                store.clone(),
                store.clone(),
                store,
                files,
            )),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/projects",
            get(handlers::list_projects).post(handlers::create_project),
        )
        .route("/api/projects/{id}", get(handlers::get_project))
        .route(
            "/api/projects/{id}/log-repositories",
            get(handlers::list_log_repositories).post(handlers::create_log_repository),
        )
        .route(
            "/api/log-repositories/{id}/entries",
            get(handlers::browse_entries),
        )
        .route("/api/log-repositories/{id}/file", get(handlers::read_file))
        .with_state(state)
}
