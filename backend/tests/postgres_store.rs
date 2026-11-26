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
use std::time::{Duration, SystemTime};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use central_logs_backend::application::auth::{
    AuthService, Identity, IdentityTokens, PendingLogin, Session,
};
use central_logs_backend::application::error::AppError;
use central_logs_backend::application::ports::{
    IdentityProvider, LogRepositoryReadModel, LogRepositoryRepository, PendingLoginStore,
    ProjectReadModel, ProjectRepository, SessionStore,
};
use central_logs_backend::domain::{LogRepository, LogRepositoryId, Project, ProjectId};
use central_logs_backend::infrastructure::oauth2::{OAuth2Config, OAuth2IdentityProvider};
use central_logs_backend::infrastructure::postgres::{DbConfig, PgPool, PgStore, connect, migrate};
use central_logs_backend::infrastructure::session_crypto::AesSecretSealer;
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
            sqlx::query(
                "TRUNCATE log_repositories, log_repository_directories, projects,
                          auth_sessions, pending_logins CASCADE",
            )
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
fn project_names_do_not_have_to_be_unique() {
    let fixture = Fixture::new();
    let store = fixture.store();
    ProjectRepository::save(&store, &project("shared", "")).unwrap();
    ProjectRepository::save(&store, &project("shared", "")).unwrap();
    assert_eq!(store.list().unwrap().len(), 2);
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
fn deleting_a_project_cascades_to_its_log_repositories() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let owner = project("payments", "");
    ProjectRepository::save(&store, &owner).unwrap();
    let repo = LogRepository::create(LogRepositoryId::new(), owner.id(), "api", &["/var/log/api"])
        .unwrap();
    LogRepositoryRepository::save(&store, &repo).unwrap();

    ProjectRepository::delete(&store, owner.id()).unwrap();
    assert_eq!(ProjectRepository::find(&store, owner.id()).unwrap(), None);
    assert_eq!(
        LogRepositoryRepository::find(&store, repo.id()).unwrap(),
        None
    );
    assert!(store.list_by_project(owner.id()).unwrap().is_empty());
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn deleting_a_log_repository_keeps_its_project() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let owner = project("payments", "");
    ProjectRepository::save(&store, &owner).unwrap();
    let repo = LogRepository::create(LogRepositoryId::new(), owner.id(), "api", &["/var/log/api"])
        .unwrap();
    LogRepositoryRepository::save(&store, &repo).unwrap();

    LogRepositoryRepository::delete(&store, repo.id()).unwrap();
    assert_eq!(
        LogRepositoryRepository::find(&store, repo.id()).unwrap(),
        None
    );
    assert!(
        ProjectRepository::find(&store, owner.id())
            .unwrap()
            .is_some()
    );
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

fn identity() -> OAuth2IdentityProvider {
    // The identity provider is not contacted by these tests.
    OAuth2IdentityProvider::new(OAuth2Config {
        identity_url: "http://127.0.0.1:9".into(),
        client_id: "central-logs".into(),
        redirect_uri: "http://127.0.0.1:8080/api/auth/callback".into(),
    })
}

fn sealer() -> Arc<AesSecretSealer> {
    Arc::new(AesSecretSealer::new([7; 32]))
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
            sealer(),
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

        let (status, _) = send(
            &first,
            "POST",
            "/api/projects",
            Some(json!({ "name": "payments" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        // A second server built on the same database, as after a restart.
        let second = router(AppState::with_postgres(
            Arc::new(identity()),
            web(),
            sealer(),
            pool,
            handle,
        ));
        let (status, body) = send(&second, "GET", &format!("/api/projects/{id}"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "payments");
    });
}

/// A time `secs` from now, truncated to whole milliseconds: the store keeps milliseconds only,
/// so an unaligned time would not compare equal after a round trip.
fn future(secs: u64) -> SystemTime {
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    SystemTime::UNIX_EPOCH + Duration::from_millis(now_ms + secs * 1000)
}

fn session_for(subject: &str) -> Session {
    Session {
        identity: Identity {
            subject: subject.into(),
            email: Some(format!("{subject}@example.test")),
            name: None,
        },
        expires_at: future(8 * 60 * 60),
        access_expires_at: future(15 * 60),
        refresh_credentials: Some("sealed-refresh".into()),
    }
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn sessions_round_trip_and_survive_a_new_pool() {
    let fixture = Fixture::new();
    let session = session_for("alice");
    SessionStore::save(&fixture.store(), "sid-1", session.clone()).unwrap();

    let config = DbConfig::from_env().unwrap();
    let pool = fixture.runtime.block_on(connect(&config)).unwrap();
    let reopened = PgStore::new(pool, fixture.runtime.handle().clone());

    assert_eq!(
        SessionStore::find(&reopened, "sid-1").unwrap(),
        Some(session)
    );
    assert_eq!(SessionStore::find(&reopened, "unknown").unwrap(), None);

    SessionStore::delete(&reopened, "sid-1").unwrap();
    assert_eq!(SessionStore::find(&reopened, "sid-1").unwrap(), None);
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn expired_sessions_are_pruned_when_another_session_is_saved() {
    let fixture = Fixture::new();
    let store = fixture.store();
    SessionStore::save(
        &store,
        "stale",
        Session {
            expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
            ..session_for("bob")
        },
    )
    .unwrap();

    SessionStore::save(&store, "fresh", session_for("carol")).unwrap();

    assert_eq!(SessionStore::find(&store, "stale").unwrap(), None);
    assert!(SessionStore::find(&store, "fresh").unwrap().is_some());
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn pending_login_is_taken_once() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let login = PendingLogin {
        code_verifier: "verifier".into(),
        return_to: "/projects/1".into(),
        expires_at: future(600),
    };
    PendingLoginStore::save(&store, "state-1", login.clone()).unwrap();

    assert_eq!(
        PendingLoginStore::take(&store, "state-1").unwrap(),
        Some(login)
    );
    assert_eq!(PendingLoginStore::take(&store, "state-1").unwrap(), None);
}

/// Identity provider that is never contacted by the refresh path: any code is accepted as a user named after it.
struct FakeIdp;

impl IdentityProvider for FakeIdp {
    fn authorization_url(&self, state: &str, _code_verifier: &str) -> Result<String, AppError> {
        Ok(format!("https://idp.test/authorize?state={state}"))
    }

    fn exchange_code(&self, code: &str, _code_verifier: &str) -> Result<IdentityTokens, AppError> {
        Ok(IdentityTokens {
            identity: Identity {
                subject: format!("user-{code}"),
                email: Some("user@example.test".into()),
                name: None,
            },
            expires_in: Duration::from_secs(900),
            refresh_token: Some(format!("refresh-{code}")),
        })
    }

    fn refresh(&self, _refresh_token: &str) -> Result<IdentityTokens, AppError> {
        Err(AppError::Upstream(
            "refresh is not used by these tests".into(),
        ))
    }
}

fn auth_on(store: PgStore) -> AuthService {
    let store = Arc::new(store);
    AuthService::new(Arc::new(FakeIdp), store.clone(), store, sealer())
}

#[test]
#[ignore = "needs a test database, see module docs"]
fn sign_in_started_before_a_restart_completes_and_stays_signed_in() {
    let fixture = Fixture::new();
    let config = DbConfig::from_env().unwrap();
    let handle = fixture.runtime.handle().clone();

    // Server 1 starts the login, then goes away.
    let start = auth_on(fixture.store())
        .begin_login(Some("/projects/1"))
        .unwrap();
    let state = start.state;

    // Server 2, on a new pool, receives the callback.
    let pool = fixture.runtime.block_on(connect(&config)).unwrap();
    let second = auth_on(PgStore::new(pool, handle.clone()));
    let done = second
        .complete_login(&state, Some(&state), "code-1")
        .unwrap();
    assert_eq!(done.return_to, "/projects/1");

    // Server 3, after another restart, still knows the user.
    let pool = fixture.runtime.block_on(connect(&config)).unwrap();
    let third = auth_on(PgStore::new(pool, handle));
    let user = third.current_user(&done.session_id).unwrap().unwrap();
    assert_eq!(user.subject, "user-code-1");

    // The state was consumed by server 2, so a replayed callback is rejected.
    assert!(matches!(
        third.complete_login(&state, Some(&state), "code-1"),
        Err(AppError::Validation(_))
    ));

    third.logout(&done.session_id).unwrap();
    assert_eq!(third.current_user(&done.session_id).unwrap(), None);
}
