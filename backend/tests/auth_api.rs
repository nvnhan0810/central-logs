//! Login flow end to end: real router, real OIDC adapter, and a fake identity provider over HTTP.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use central_logs_backend::infrastructure::oidc::{OidcConfig, OidcIdentityProvider};
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tower::ServiceExt;

const REDIRECT_URI: &str = "http://backend.test/api/auth/callback";
const FRONTEND: &str = "http://frontend.test";

#[derive(Default)]
struct IdpState {
    /// Issuer URL of this fake provider.
    base: String,
    /// authorization code -> PKCE challenge it was issued for
    codes: Mutex<HashMap<String, String>>,
    /// token endpoint requests that were accepted, for assertions
    token_requests: Mutex<u32>,
}

type Idp = Arc<IdpState>;

async fn discovery(State(idp): State<Idp>) -> Json<Value> {
    let base = &idp.base;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "userinfo_endpoint": format!("{base}/userinfo"),
    }))
}

async fn authorize(State(idp): State<Idp>, Query(q): Query<HashMap<String, String>>) -> Response {
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["code_challenge_method"], "S256");
    let code = format!("code-{}", idp.codes.lock().unwrap().len() + 1);
    idp.codes
        .lock()
        .unwrap()
        .insert(code.clone(), q["code_challenge"].clone());
    let location = format!("{}?code={code}&state={}", q["redirect_uri"], q["state"]);
    Redirect::to(&location).into_response()
}

async fn token(State(idp): State<Idp>, Form(form): Form<HashMap<String, String>>) -> Response {
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["redirect_uri"], REDIRECT_URI);
    let Some(challenge) = idp.codes.lock().unwrap().remove(&form["code"]) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    };
    // PKCE: the verifier must hash to the challenge sent in the authorize request.
    let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes()));
    if expected != challenge {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    }
    *idp.token_requests.lock().unwrap() += 1;
    Json(json!({"access_token": "at-42", "token_type": "Bearer"})).into_response()
}

async fn userinfo(headers: HeaderMap) -> Response {
    let ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        == Some("Bearer at-42");
    if !ok {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"sub": "user-42", "email": "user@example.com", "name": "Test User"}))
        .into_response()
}

/// Starts the fake provider on a random port and returns its base URL.
async fn start_idp() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let idp: Idp = Arc::new(IdpState {
        base: base.clone(),
        ..Default::default()
    });
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/userinfo", get(userinfo))
        .with_state(idp);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

fn app(issuer: &str, cookie_secure: bool) -> Router {
    let identity = OidcIdentityProvider::new(OidcConfig {
        issuer: issuer.into(),
        client_id: "central-logs".into(),
        client_secret: Some("shh".into()),
        redirect_uri: REDIRECT_URI.into(),
    });
    let web = WebSettings {
        frontend_url: FRONTEND.into(),
        cookie_secure,
    };
    router(AppState::in_memory(Arc::new(identity), web))
}

async fn get_raw(app: &Router, uri: &str, cookie: Option<&str>) -> Response {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    }
}

fn location(response: &Response) -> String {
    response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned()
}

fn query_of(url: &str) -> HashMap<String, String> {
    reqwest::Url::parse(url)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn session_cookie(response: &Response) -> String {
    let set = response.headers()[header::SET_COOKIE].to_str().unwrap();
    set.split(';').next().unwrap().to_owned()
}

/// Plays the browser: login -> identity provider -> callback. Returns the callback response.
async fn sign_in(app: &Router, return_to: &str) -> Response {
    let start = get_raw(app, &format!("/api/auth/login?return_to={return_to}"), None).await;
    assert_eq!(start.status(), StatusCode::FOUND);
    let authorize_url = location(&start);

    // The browser follows the authorize redirect to the provider, which sends it back with a code.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let idp_response = client.get(&authorize_url).send().await.unwrap();
    assert_eq!(idp_response.status(), reqwest::StatusCode::SEE_OTHER);
    let back = idp_response.headers()[reqwest::header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    let params = query_of(&back);
    let callback = format!(
        "/api/auth/callback?code={}&state={}",
        params["code"], params["state"]
    );
    get_raw(app, &callback, None).await
}

#[tokio::test]
async fn login_flow_sets_session_cookie_and_exposes_the_user() {
    let issuer = start_idp().await;
    let app = app(&issuer, true);

    let callback = sign_in(&app, "/projects/7").await;

    assert_eq!(callback.status(), StatusCode::FOUND);
    assert_eq!(location(&callback), format!("{FRONTEND}/projects/7"));
    let set = callback.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(set.starts_with("central_logs_session="));
    assert!(set.contains("HttpOnly"));
    assert!(set.contains("SameSite=Lax"));
    assert!(set.contains("Secure"));
    let cookie = session_cookie(&callback);

    let me = get_raw(&app, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(me.status(), StatusCode::OK);
    assert_eq!(
        body_json(me).await,
        json!({"subject": "user-42", "email": "user@example.com", "name": "Test User"})
    );
}

#[tokio::test]
async fn me_requires_a_session() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);

    let anonymous = get_raw(&app, "/api/auth/me", None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(anonymous).await["error"]["code"],
        "unauthenticated"
    );

    let forged = get_raw(&app, "/api/auth/me", Some("central_logs_session=forged")).await;
    assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn callback_state_can_be_used_only_once() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);
    let first = sign_in(&app, "/").await;
    assert_eq!(first.status(), StatusCode::FOUND);

    // Replaying the same callback (same code and state) must fail.
    let start = get_raw(&app, "/api/auth/login", None).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let back = client.get(location(&start)).send().await.unwrap();
    let params = query_of(back.headers()[reqwest::header::LOCATION].to_str().unwrap());
    let callback = format!(
        "/api/auth/callback?code={}&state={}",
        params["code"], params["state"]
    );
    assert_eq!(
        get_raw(&app, &callback, None).await.status(),
        StatusCode::FOUND
    );
    let replay = get_raw(&app, &callback, None).await;
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn callback_rejects_forged_state_and_provider_errors() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);

    let forged = get_raw(&app, "/api/auth/callback?code=x&state=nope", None).await;
    assert_eq!(forged.status(), StatusCode::BAD_REQUEST);

    let denied = get_raw(&app, "/api/auth/callback?error=access_denied", None).await;
    assert_eq!(denied.status(), StatusCode::BAD_REQUEST);

    let missing = get_raw(&app, "/api/auth/callback", None).await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn bad_authorization_code_is_reported_as_identity_unavailable() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);
    let start = get_raw(&app, "/api/auth/login", None).await;
    let state = query_of(&location(&start))["state"].clone();

    let callback = get_raw(
        &app,
        &format!("/api/auth/callback?code=not-issued&state={state}"),
        None,
    )
    .await;

    assert_eq!(callback.status(), StatusCode::BAD_GATEWAY);
    let body = body_json(callback).await;
    assert_eq!(body["error"]["code"], "identity_unavailable");
    // Provider details must not leak to the client.
    assert!(!body.to_string().contains("invalid_grant"));
}

#[tokio::test]
async fn unreachable_provider_is_reported_without_crashing() {
    let app = app("http://127.0.0.1:9", false);

    let start = get_raw(&app, "/api/auth/login", None).await;

    assert_eq!(start.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        body_json(start).await["error"]["code"],
        "identity_unavailable"
    );
}

#[tokio::test]
async fn open_redirect_in_return_to_is_rejected() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);

    let response = get_raw(&app, "/api/auth/login?return_to=https://evil.test", None).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logout_ends_the_session_and_clears_the_cookie() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);
    let cookie = session_cookie(&sign_in(&app, "/").await);

    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let cleared = logout.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cleared.contains("Max-Age=0"));
    assert_eq!(
        get_raw(&app, "/api/auth/me", Some(&cookie)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn existing_metadata_api_is_unaffected_by_auth_routes() {
    let issuer = start_idp().await;
    let app = app(&issuer, false);
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"name": "Payments"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
}
