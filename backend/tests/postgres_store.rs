//! Tests against a real PostgreSQL server. They are ignored by default.
//!
//! Run them against a dedicated test database (its tables are truncated before each test):
//!
//! ```sh
//! DB_NAME=central_logs_test cargo test --test postgres_store -- --ignored --test-threads=1
//! ```
//!
//! The database name must end in `_test`, so the tests never truncate a development database.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use central_logs_backend::application::error::AppError;
use central_logs_backend::application::ports::{
    LogRepositoryReadModel, LogRepositoryRepository, ProjectReadModel, ProjectRepository,
};
use central_logs_backend::domain::{LogRepository, LogRepositoryId, Name, Project, ProjectId};
use central_logs_backend::infrastructure::oidc::{OidcConfig, OidcIdentityProvider};
use central_logs_backend::infrastructure::postgres::{DbConfig, PgPool, PgStore, connect, migrate};
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::runtime::{Handle, Runtime};
use tower::ServiceExt;

/// A runtime that owns the pool, plus a database that has been migrated and emptied.
struct Fixture {
    runtime: Runtime,
    pool: PgPool,
}

impl Fixture {
    fn new() -> Self {
        let config = DbConfig::from_env().expect("set DB_* variables to run these tests");
        assert!(
            config.name.ends_with("_test"),
            "refusing to truncate database {:?}: its name must end in _test",
            config.name
        );
        let runtime = Runtime::new().unwrap();
        let pool = runtime.block_on(async {
            migrate(&config).await.expect("migrations apply");
            let pool = connect(&config).await.expect("connect to test database");
            sqlx::query("TRUNCATE log_repositories, log_repository_directories, projects CASCADE")
                .execute(&pool)
                .await
                .expect("truncate");
            pool
        });
        Self { runtime, pool }
    }

    /// Borrow the fixture for the whole test: dropping it shuts the runtime down.
    fn store(&self) -> PgStore {
        PgStore::new(self.pool.clone(), self.runtime.handle().clone())
    }
}

fn project(name: &str, description: &str) -> Project {
    Project::create(ProjectId::new(), name, description).unwrap()
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn projects_round_trip_and_list_in_name_order() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let beta = project("beta", "second");
    let alpha = project("Alpha", "first");
    ProjectRepository::save(&store, &beta).unwrap();
    ProjectRepository::save(&store, &alpha).unwrap();

    assert_eq!(
        ProjectRepository::find(&store, alpha.id()).unwrap(),
        Some(alpha.clone())
    );
    assert_eq!(
        ProjectRepository::find(&store, ProjectId::new()).unwrap(),
        None
    );
    assert!(store.name_taken(&Name::parse("beta").unwrap()).unwrap());
    assert!(!store.name_taken(&Name::parse("gamma").unwrap()).unwrap());

    let names: Vec<String> = store.list().unwrap().into_iter().map(|p| p.name).collect();
    // Byte order, like the in-memory store: "Alpha" sorts before "beta".
    assert_eq!(names, vec!["Alpha", "beta"]);
    assert_eq!(store.get(beta.id()).unwrap().unwrap().description, "second");
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn saving_an_existing_project_updates_it() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let original = project("payments", "old");
    ProjectRepository::save(&store, &original).unwrap();
    let renamed = Project::create(original.id(), "payments-core", "new").unwrap();
    ProjectRepository::save(&store, &renamed).unwrap();

    assert_eq!(
        ProjectRepository::find(&store, original.id()).unwrap(),
        Some(renamed)
    );
    assert_eq!(store.list().unwrap().len(), 1);
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn duplicate_project_name_is_a_conflict() {
    let fixture = Fixture::new();
    let store = fixture.store();
    ProjectRepository::save(&store, &project("shared", "")).unwrap();
    let err = ProjectRepository::save(&store, &project("shared", "")).unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "{err:?}");
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn log_repositories_keep_directory_order_and_replace_on_save() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let owner = project("payments", "");
    ProjectRepository::save(&store, &owner).unwrap();

    let repo = LogRepository::create(
        LogRepositoryId::new(),
        owner.id(),
        "api",
        &["/var/log/api", "/srv/app/logs"],
    )
    .unwrap();
    LogRepositoryRepository::save(&store, &repo).unwrap();
    assert_eq!(
        LogRepositoryRepository::find(&store, repo.id()).unwrap(),
        Some(repo.clone())
    );

    let views = store.list_by_project(owner.id()).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "api");
    assert_eq!(views[0].directories, vec!["/var/log/api", "/srv/app/logs"]);

    let replaced = LogRepository::create(repo.id(), owner.id(), "api-v2", &["/opt/logs"]).unwrap();
    LogRepositoryRepository::save(&store, &replaced).unwrap();
    assert_eq!(
        LogRepositoryRepository::find(&store, repo.id()).unwrap(),
        Some(replaced)
    );
    let views = store.list_by_project(owner.id()).unwrap();
    assert_eq!(views[0].directories, vec!["/opt/logs"]);
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn data_survives_a_new_connection_pool() {
    let fixture = Fixture::new();
    let owner = project("durable", "still here");
    ProjectRepository::save(&fixture.store(), &owner).unwrap();

    let config = DbConfig::from_env().unwrap();
    let pool = fixture.runtime.block_on(connect(&config)).unwrap();
    let reopened = PgStore::new(pool, fixture.runtime.handle().clone());
    assert_eq!(
        ProjectRepository::find(&reopened, owner.id()).unwrap(),
        Some(owner)
    );
}

fn identity() -> OidcIdentityProvider {
    // The identity provider is not contacted by these tests.
    OidcIdentityProvider::new(OidcConfig {
        issuer: "http://127.0.0.1:9".into(),
        client_id: "central-logs".into(),
        client_secret: None,
        redirect_uri: "http://127.0.0.1:8080/api/auth/callback".into(),
    })
}

fn web() -> WebSettings {
    WebSettings {
        frontend_url: "http://localhost:3000".into(),
        cookie_secure: false,
    }
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(json) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(json.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn api_writes_are_visible_to_a_restarted_server() {
    let fixture = Fixture::new();
    let pool = fixture.pool.clone();
    let handle: Handle = fixture.runtime.handle().clone();
    fixture.runtime.block_on(async move {
        let first = router(AppState::with_postgres(
            Arc::new(identity()),
            web(),
            pool.clone(),
            handle.clone(),
        ));
        let (status, created) = send(
            &first,
            "POST",
            "/api/projects",
            Some(json!({ "name": "payments" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let id = created["id"].as_str().unwrap().to_owned();

        let (status, duplicate) = send(
            &first,
            "POST",
            "/api/projects",
            Some(json!({ "name": "payments" })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");

        // A second server built on the same database, as after a restart.
        let second = router(AppState::with_postgres(
            Arc::new(identity()),
            web(),
            pool,
            handle,
        ));
        let (status, body) = send(&second, "GET", &format!("/api/projects/{id}"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "payments");
    });
}
