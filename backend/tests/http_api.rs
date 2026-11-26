//! End-to-end API tests: real router, real file system, in-memory metadata.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use central_logs_backend::infrastructure::oauth2::{OAuth2Config, OAuth2IdentityProvider};
use central_logs_backend::infrastructure::session_crypto::AesSecretSealer;
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

fn app() -> axum::Router {
    // The identity provider is never contacted by these tests.
    let identity = OAuth2IdentityProvider::new(OAuth2Config {
        identity_url: "http://127.0.0.1:9".into(),
        client_id: "central-logs".into(),
        redirect_uri: "http://127.0.0.1:8080/api/auth/callback".into(),
    });
    let web = WebSettings {
        frontend_url: "http://localhost:3000".into(),
        cookie_secure: false,
    };
    router(AppState::in_memory(
        Arc::new(identity),
        web,
        Arc::new(AesSecretSealer::new([7; 32])),
    ))
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

fn log_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("2026/11")).unwrap();
    fs::write(dir.path().join("2026/11/app.log"), "one\ntwo\nthree\n").unwrap();
    dir
}

fn encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('/', "%2F")
}

async fn create_project(app: &axum::Router, name: &str) -> String {
    let (status, body) = send(app, "POST", "/api/projects", Some(json!({"name": name}))).await;
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().unwrap().to_string()
}

async fn create_repo(app: &axum::Router, project: &str, dir: &Path) -> String {
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/projects/{project}/log-repositories"),
        Some(json!({"name": "api", "directories": [dir.to_str().unwrap()]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn creates_and_lists_projects() {
    let app = app();
    let id = create_project(&app, "Payments").await;

    let (status, list) = send(&app, "GET", "/api/projects", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["id"], id);
    assert_eq!(list[0]["name"], "Payments");

    let (status, one) = send(&app, "GET", &format!("/api/projects/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["description"], "");
}

#[tokio::test]
async fn project_names_may_repeat() {
    let app = app();
    let first = create_project(&app, "Payments").await;
    let second = create_project(&app, "Payments").await;
    assert_ne!(first, second);
}

#[tokio::test]
async fn blank_project_name_is_bad_request() {
    let app = app();
    let (status, _) = send(&app, "POST", "/api/projects", Some(json!({"name": "  "}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_or_malformed_project_id() {
    let app = app();
    let (status, _) = send(
        &app,
        "GET",
        "/api/projects/00000000-0000-0000-0000-000000000000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, "GET", "/api/projects/not-a-uuid", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn creates_log_repository_only_for_existing_directories() {
    let app = app();
    let project = create_project(&app, "Payments").await;
    let dir = log_dir();

    let (status, _) = send(
        &app,
        "POST",
        &format!("/api/projects/{project}/log-repositories"),
        Some(json!({"name": "api", "directories": ["/definitely/missing"]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let repo = create_repo(&app, &project, dir.path()).await;
    let (status, list) = send(
        &app,
        "GET",
        &format!("/api/projects/{project}/log-repositories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["id"], repo);
}

#[tokio::test]
async fn log_repository_under_unknown_project_is_not_found() {
    let app = app();
    let (status, _) = send(
        &app,
        "POST",
        "/api/projects/00000000-0000-0000-0000-000000000000/log-repositories",
        Some(json!({"name": "api", "directories": ["/tmp"]})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn browses_directories_inside_a_repository() {
    let app = app();
    let project = create_project(&app, "Payments").await;
    let dir = log_dir();
    let repo = create_repo(&app, &project, dir.path()).await;
    let root = dir.path().to_str().unwrap();

    let uri = format!(
        "/api/log-repositories/{repo}/entries?directory={}&path={}",
        encode(root),
        encode("2026/11")
    );
    let (status, entries) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{entries}");
    assert_eq!(entries[0]["name"], "app.log");
    assert_eq!(entries[0]["kind"], "file");
    assert_eq!(entries[0]["relative_path"], "2026/11/app.log");
}

#[tokio::test]
async fn reads_log_file_page_by_page() {
    let app = app();
    let project = create_project(&app, "Payments").await;
    let dir = log_dir();
    let repo = create_repo(&app, &project, dir.path()).await;
    let root = dir.path().to_str().unwrap();
    let file = encode("2026/11/app.log");

    let uri = format!(
        "/api/log-repositories/{repo}/file?directory={}&path={file}&max_lines=2",
        encode(root)
    );
    let (status, first) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["lines"][0]["text"], "one");
    assert_eq!(first["lines"][1]["text"], "two");
    assert_eq!(first["eof"], false);
    assert_eq!(first["file"]["size"], 14);

    let next = first["next_offset"].as_u64().unwrap();
    let uri = format!(
        "/api/log-repositories/{repo}/file?directory={}&path={file}&max_lines=2&offset={next}",
        encode(root)
    );
    let (status, second) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["lines"][0]["text"], "three");
    assert_eq!(second["eof"], true);
}

#[tokio::test]
async fn path_traversal_is_rejected() {
    let app = app();
    let project = create_project(&app, "Payments").await;
    let dir = log_dir();
    let repo = create_repo(&app, &project, dir.path()).await;
    let root = encode(dir.path().to_str().unwrap());

    let uri =
        format!("/api/log-repositories/{repo}/file?directory={root}&path=..%2F..%2Fetc%2Fpasswd");
    let (status, _) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn directory_outside_repository_is_not_found() {
    let app = app();
    let project = create_project(&app, "Payments").await;
    let dir = log_dir();
    let repo = create_repo(&app, &project, dir.path()).await;

    let uri = format!("/api/log-repositories/{repo}/entries?directory=%2Fetc");
    let (status, body) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn zero_line_limit_is_bad_request() {
    let app = app();
    let project = create_project(&app, "Payments").await;
    let dir = log_dir();
    let repo = create_repo(&app, &project, dir.path()).await;
    let root = encode(dir.path().to_str().unwrap());

    let uri = format!(
        "/api/log-repositories/{repo}/file?directory={root}&path=2026%2F11%2Fapp.log&max_lines=0"
    );
    let (status, _) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

async fn cors_headers(app: &axum::Router, method: &str, origin: &str) -> axum::http::HeaderMap {
    let request = Request::builder()
        .method(method)
        .uri("/api/auth/me")
        .header("origin", origin)
        .body(Body::empty())
        .unwrap();
    app.clone()
        .oneshot(request)
        .await
        .unwrap()
        .headers()
        .clone()
}

#[tokio::test]
async fn frontend_origin_may_call_the_api_with_credentials() {
    let app = app();
    let headers = cors_headers(&app, "GET", "http://localhost:3000").await;

    assert_eq!(
        headers["access-control-allow-origin"],
        "http://localhost:3000"
    );
    assert_eq!(headers["access-control-allow-credentials"], "true");
}

#[tokio::test]
async fn other_origins_get_no_cors_grant() {
    let app = app();
    let headers = cors_headers(&app, "GET", "https://evil.example").await;

    // No allow-origin header for an unlisted caller, so the browser rejects the response.
    assert!(headers.get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn preflight_allows_json_posts_from_the_frontend() {
    let app = app();
    let request = Request::builder()
        .method("OPTIONS")
        .uri("/api/auth/logout")
        .header("origin", "http://localhost:3000")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();

    assert!(response.status().is_success());
    let headers = response.headers();
    assert_eq!(
        headers["access-control-allow-origin"],
        "http://localhost:3000"
    );
    assert_eq!(headers["access-control-allow-credentials"], "true");
    assert!(
        headers["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .contains("POST")
    );
}

#[tokio::test]
async fn index_page_says_hello_so_the_backend_can_be_checked_in_a_browser() {
    let response = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response.headers()["content-type"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        content_type.starts_with("text/html"),
        "content-type: {content_type}"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("hello"), "body: {body}");
}
