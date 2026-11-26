//! CORS: the configured frontend origin may call the API from the browser, with cookies.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use central_logs_backend::infrastructure::oauth2::{OAuth2Config, OAuth2IdentityProvider};
use central_logs_backend::infrastructure::session_crypto::AesSecretSealer;
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};
use tower::ServiceExt;

const FRONTEND: &str = "http://localhost:5173";
const OTHER_FRONTEND: &str = "https://logs.example.com";

fn app(frontend_url: &str) -> Router {
    let identity = OAuth2IdentityProvider::new(OAuth2Config {
        identity_url: "http://127.0.0.1:9".into(),
        client_id: "central-logs".into(),
        redirect_uri: "http://127.0.0.1:8080/api/auth/callback".into(),
    });
    let web = WebSettings {
        frontend_url: frontend_url.into(),
        cookie_secure: false,
    };
    router(AppState::in_memory(
        Arc::new(identity),
        web,
        Arc::new(AesSecretSealer::new([7; 32])),
    ))
}

fn header_value(response: &axum::response::Response, name: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .map(|v: &HeaderValue| v.to_str().unwrap().to_string())
}

async fn preflight(app: &Router, origin: &str) -> axum::response::Response {
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/projects")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

async fn simple_request(app: &Router, origin: &str) -> axum::response::Response {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/projects")
        .header(header::ORIGIN, origin)
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn preflight_from_frontend_allows_credentials_and_json() {
    let app = app(FRONTEND);
    let response = preflight(&app, FRONTEND).await;

    assert!(response.status().is_success());
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
        Some(FRONTEND)
    );
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_CREDENTIALS).as_deref(),
        Some("true")
    );
    let methods = header_value(&response, header::ACCESS_CONTROL_ALLOW_METHODS).unwrap();
    assert!(methods.contains("POST"), "methods: {methods}");
    let headers = header_value(&response, header::ACCESS_CONTROL_ALLOW_HEADERS).unwrap();
    assert!(
        headers.to_ascii_lowercase().contains("content-type"),
        "headers: {headers}"
    );
}

#[tokio::test]
async fn actual_request_from_frontend_gets_cors_headers() {
    let app = app(FRONTEND);
    let response = simple_request(&app, FRONTEND).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
        Some(FRONTEND)
    );
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_CREDENTIALS).as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn unknown_origin_gets_no_cors_headers() {
    let app = app(FRONTEND);
    let response = simple_request(&app, "https://evil.example").await;

    assert!(header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn other_origin_gets_no_cors_grant() {
    let app = app(FRONTEND);
    let response = simple_request(&app, OTHER_FRONTEND).await;

    assert!(header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn trailing_slash_in_frontend_url_still_matches_browser_origin() {
    // Browsers send origins without a trailing slash; the router strips one from the setting.
    let app = app(&format!("{FRONTEND}/"));
    let response = simple_request(&app, FRONTEND).await;

    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
        Some(FRONTEND)
    );
}
