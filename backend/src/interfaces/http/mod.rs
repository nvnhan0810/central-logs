//! HTTP API (axum). Handlers are thin: parse input, call a use case, map output.

mod auth;
mod dto;
mod error;
mod handlers;

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::response::Html;
use axum::routing::{get, post};
use tokio::runtime::Handle;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::application::auth::AuthService;
use crate::application::commands::CommandService;
use crate::application::ports::{
    IdentityProvider, LogRepositoryReadModel, LogRepositoryRepository, PendingLoginStore,
    ProjectReadModel, ProjectRepository, SecretSealer, SessionStore,
};
use crate::application::queries::QueryService;
use crate::infrastructure::fs::FsLogFileSystem;
use crate::infrastructure::memory::InMemoryStore;
use crate::infrastructure::postgres::{PgPool, PgStore};

pub use error::ApiError;

/// Settings the HTTP layer needs from the deployment.
#[derive(Debug, Clone)]
pub struct WebSettings {
    /// Origin of the frontend (e.g. `https://logs.example.com`). The browser is sent here after login,
    /// and it is the only origin allowed to call the API from a browser. Never `*`, because requests carry cookies.
    pub frontend_url: String,
    /// Set the session cookie with `Secure`. Turn off only for plain-HTTP local development.
    pub cookie_secure: bool,
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
    pub fn in_memory(
        identity: Arc<dyn IdentityProvider>,
        web: WebSettings,
        sealer: Arc<dyn SecretSealer>,
    ) -> Self {
        Self::assemble(identity, web, sealer, Arc::new(InMemoryStore::default()))
    }

    /// Stores projects, log repositories, login sessions and pending logins in PostgreSQL,
    /// so signed-in users stay signed in across a restart.
    /// `runtime` must be the tokio runtime that created `pool`.
    pub fn with_postgres(
        identity: Arc<dyn IdentityProvider>,
        web: WebSettings,
        sealer: Arc<dyn SecretSealer>,
        pool: PgPool,
        runtime: Handle,
    ) -> Self {
        Self::assemble(identity, web, sealer, Arc::new(PgStore::new(pool, runtime)))
    }

    fn assemble<S>(
        identity: Arc<dyn IdentityProvider>,
        web: WebSettings,
        sealer: Arc<dyn SecretSealer>,
        store: Arc<S>,
    ) -> Self
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
            auth: Arc::new(AuthService::new(identity, store.clone(), store, sealer)),
            web: Arc::new(web),
        }
    }
}

/// Index page: a plain "hello" so anyone opening the backend URL can see it is online.
async fn index() -> Html<&'static str> {
    Html(
        "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>central-logs</title></head>\n<body>\nhello\n</body>\n</html>\n",
    )
}

pub fn router(state: AppState) -> Router {
    let cors = cors_layer(state.web.frontend_url.trim_end_matches('/'));
    Router::new()
        .route("/", get(index))
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
        .route(CALLBACK_PATH, get(auth::callback))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/logout", post(auth::logout))
        .layer(cors)
        .with_state(state)
}

/// Path of the OAuth callback route. The backend origin from config plus this path is the `redirect_uri`
/// sent to the identity service, so the route and the registered redirect URI cannot drift apart.
pub const CALLBACK_PATH: &str = "/api/auth/callback";

/// Lets the configured frontend call the API with cookies (`fetch(..., { credentials: "include" })`).
/// The origin is never `*`. The browser keeps the session cookie only when the frontend and backend are same-site
/// (e.g. both on `localhost`, or sub-domains of one domain); `SameSite=Lax` is unchanged.
/// The origin is validated at startup (see `main`), so an invalid one here is a programming error.
fn cors_layer(origin: &str) -> CorsLayer {
    let allowed = HeaderValue::from_str(origin).expect("CORS origin must be a valid header value");
    CorsLayer::new()
        .allow_origin(AllowOrigin::list([allowed]))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true)
}
