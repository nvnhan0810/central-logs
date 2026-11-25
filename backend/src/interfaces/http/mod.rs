//! HTTP API (axum). Handlers are thin: parse input, call a use case, map output.

mod auth;
mod dto;
mod error;
mod handlers;

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{get, post};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::application::auth::AuthService;
use crate::application::commands::CommandService;
use crate::application::ports::IdentityProvider;
use crate::application::queries::QueryService;
use crate::infrastructure::fs::FsLogFileSystem;
use crate::infrastructure::memory::InMemoryStore;

pub use error::ApiError;

/// Settings the HTTP layer needs from the deployment.
#[derive(Debug, Clone)]
pub struct WebSettings {
    /// Origin of the frontend, where the browser is sent after login (e.g. `https://logs.example.com`).
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
    /// Wires the default adapters: in-memory metadata and sessions, and the host file system.
    pub fn in_memory(identity: Arc<dyn IdentityProvider>, web: WebSettings) -> Self {
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
                store.clone(),
                files,
            )),
            auth: Arc::new(AuthService::new(identity, store.clone(), store)),
            web: Arc::new(web),
        }
    }
}

/// CORS for the frontend origin only, with credentials so the session cookie is sent.
/// The browser keeps the cookie because the frontend and backend must be same-site
/// (e.g. both on `localhost`, or sub-domains of one domain); `SameSite=Lax` is unchanged.
fn cors_layer(frontend_url: &str) -> CorsLayer {
    let origin = frontend_url.trim_end_matches('/');
    let origin = HeaderValue::from_str(origin)
        .unwrap_or_else(|_| panic!("CENTRAL_LOGS_FRONTEND_URL is not a valid origin: {origin}"));
    CorsLayer::new()
        .allow_origin(AllowOrigin::exact(origin))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::ACCEPT, header::CONTENT_TYPE])
}

pub fn router(state: AppState) -> Router {
    let cors = cors_layer(&state.web.frontend_url);
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
        .with_state(state)
        .layer(cors)
}
