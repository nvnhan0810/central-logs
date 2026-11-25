//! HTTP API (axum). Handlers are thin: parse input, call a use case, map output.

mod auth;
mod dto;
mod error;
mod handlers;

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{get, post};
use tokio::runtime::Handle;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::application::auth::AuthService;
use crate::application::commands::CommandService;
use crate::application::ports::{
    IdentityProvider, LogRepositoryReadModel, LogRepositoryRepository, PendingLoginStore,
    ProjectReadModel, ProjectRepository, SessionStore,
};
use crate::application::queries::QueryService;
use crate::infrastructure::fs::FsLogFileSystem;
use crate::infrastructure::memory::InMemoryStore;
use crate::infrastructure::postgres::{PgPool, PgStore};

pub use error::ApiError;

/// Settings the HTTP layer needs from the deployment.
#[derive(Debug, Clone)]
pub struct WebSettings {
    /// Origin of the frontend, where the browser is sent after login (e.g. `https://logs.example.com`).
    pub frontend_url: String,
    /// Set the session cookie with `Secure`. Turn off only for plain-HTTP local development.
    pub cookie_secure: bool,
    /// Origins allowed to call the API from a browser, e.g. `http://localhost:5173`.
    /// Each entry is `scheme://host[:port]` with no path. Requests carry cookies, so origins are never `*`.
    pub cors_origins: Vec<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub commands: Arc<CommandService>,
    pub queries: Arc<QueryService>,
    pub auth: Arc<AuthService>,
    pub web: Arc<WebSettings>,
}

impl AppState {
    /// Wires the in-memory adapters for everything. Used by tests; data is lost on restart.
    pub fn in_memory(identity: Arc<dyn IdentityProvider>, web: WebSettings) -> Self {
        Self::assemble(identity, web, Arc::new(InMemoryStore::default()))
    }

    /// Stores projects, log repositories, login sessions and pending logins in PostgreSQL,
    /// so signed-in users stay signed in across a restart.
    /// `runtime` must be the tokio runtime that created `pool`.
    pub fn with_postgres(
        identity: Arc<dyn IdentityProvider>,
        web: WebSettings,
        pool: PgPool,
        runtime: Handle,
    ) -> Self {
        Self::assemble(identity, web, Arc::new(PgStore::new(pool, runtime)))
    }

    fn assemble<S>(identity: Arc<dyn IdentityProvider>, web: WebSettings, store: Arc<S>) -> Self
    where
        S: ProjectRepository
            + LogRepositoryRepository
            + ProjectReadModel
            + LogRepositoryReadModel
            + PendingLoginStore
            + SessionStore
            + 'static,
    {
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
                store.clone(),
                files,
            )),
            auth: Arc::new(AuthService::new(identity, store.clone(), store)),
            web: Arc::new(web),
        }
    }
}

pub fn router(state: AppState) -> Router {
    // The login-redirect frontend is always allowed too, so an existing `CENTRAL_LOGS_FRONTEND_URL` keeps working.
    let mut origins = state.web.cors_origins.clone();
    origins.push(state.web.frontend_url.trim_end_matches('/').to_string());
    let cors = cors_layer(&origins);
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
        .route("/api/auth/login", get(auth::login))
        .route("/api/auth/callback", get(auth::callback))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/logout", post(auth::logout))
        .layer(cors)
        .with_state(state)
}

/// Lets the configured frontend origins call the API with cookies (`fetch(..., { credentials: "include" })`).
/// Origins are never `*`. The browser keeps the session cookie only when the frontend and backend are same-site
/// (e.g. both on `localhost`, or sub-domains of one domain); `SameSite=Lax` is unchanged.
/// Origins are validated at startup (see `main`), so an invalid one here is a programming error.
fn cors_layer(origins: &[String]) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .map(|o| HeaderValue::from_str(o).expect("CORS origin must be a valid header value"))
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true)
}
