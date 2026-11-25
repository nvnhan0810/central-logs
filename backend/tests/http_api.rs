//! End-to-end API tests: real router, real file system, in-memory metadata.

use std::fs;
use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use central_logs_backend::interfaces::http::{AppState, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState::in_memory())
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
async fn duplicate_project_name_is_conflict() {
    let app = app();
    create_project(&app, "Payments").await;
    let (status, body) = send(
        &app,
        "POST",
        "/api/projects",
        Some(json!({"name": "Payments"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
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
