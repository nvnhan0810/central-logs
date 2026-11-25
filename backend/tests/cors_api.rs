//! CORS: the configured frontend origins may call the API from the browser, with cookies.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use central_logs_backend::infrastructure::oidc::{OidcConfig, OidcIdentityProvider};
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};
use tower::ServiceExt;

const FRONTEND: &str = "http://localhost:5173";
const OTHER_FRONTEND: &str = "https://logs.example.com";

fn app(cors_origins: &[&str]) -> Router {
    let identity = OidcIdentityProvider::new(OidcConfig {
        issuer: "http://127.0.0.1:9".into(),
        client_id: "central-logs".into(),
        client_secret: None,
        redirect_uri: "http://127.0.0.1:8080/api/auth/callback".into(),
    });
    let web = WebSettings {
        frontend_url: FRONTEND.into(),
        cookie_secure: false,
        cors_origins: cors_origins.iter().map(|o| o.to_string()).collect(),
    };
    router(AppState::in_memory(Arc::new(identity), web))
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
async fn preflight_from_configured_origin_allows_credentials_and_json() {
    let app = app(&[FRONTEND]);
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
async fn actual_request_from_configured_origin_gets_cors_headers() {
    let app = app(&[FRONTEND]);
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
    let app = app(&[FRONTEND]);
    let response = simple_request(&app, "https://evil.example").await;

    assert!(header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn every_configured_origin_is_allowed() {
    let app = app(&[FRONTEND, OTHER_FRONTEND]);

    for origin in [FRONTEND, OTHER_FRONTEND] {
        let response = simple_request(&app, origin).await;
        assert_eq!(
            header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            Some(origin)
        );
    }
}

#[tokio::test]
async fn no_origins_configured_still_allows_the_login_frontend() {
    // `frontend_url` (the login redirect target) is always allowed, even with an empty list.
    let app = app(&[]);

    let allowed = simple_request(&app, FRONTEND).await;
    assert_eq!(
        header_value(&allowed, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
        Some(FRONTEND)
    );

    let other = simple_request(&app, OTHER_FRONTEND).await;
    assert!(header_value(&other, header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}
