//! Login flow end to end: real router, real OAuth 2.0 adapter, and a fake identity provider over HTTP.
//!
//! The fake provider issues rotating refresh tokens and a configurable access-token lifetime, so the
//! refresh path can be exercised with real HTTP calls.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use central_logs_backend::infrastructure::oauth2::{OAuth2Config, OAuth2IdentityProvider};
use central_logs_backend::infrastructure::session_crypto::AesSecretSealer;
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tower::ServiceExt;

const REDIRECT_URI: &str = "http://backend.test/api/auth/callback";
const FRONTEND: &str = "http://frontend.test";
const SESSION: &str = "central_logs_session";
const OAUTH: &str = "central_logs_oauth";

#[derive(Default)]
struct IdpState {
    /// Lifetime of every access token the provider issues, in seconds.
    access_ttl: u64,
    /// authorization code -> PKCE challenge it was issued for
    codes: Mutex<HashMap<String, String>>,
    /// access tokens accepted by userinfo
    access_tokens: Mutex<HashSet<String>>,
    /// the only refresh token still accepted; rotation replaces it
    refresh_token: Mutex<Option<String>>,
    /// when set, every refresh is rejected, as if the user's grant was revoked
    revoked: Mutex<bool>,
    /// number of accepted refresh grants, for assertions
    refreshes: Mutex<u32>,
    /// counter that makes every issued token unique
    issued: Mutex<u32>,
}

type Idp = Arc<IdpState>;

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

/// Issues a new access token and a new refresh token, and makes the refresh token the valid one.
fn issue(idp: &IdpState) -> Response {
    let mut counter = idp.issued.lock().unwrap();
    *counter += 1;
    let access = format!("at-{counter}");
    let refresh = format!("rt-{counter}");
    idp.access_tokens.lock().unwrap().insert(access.clone());
    *idp.refresh_token.lock().unwrap() = Some(refresh.clone());
    Json(json!({
        "access_token": access,
        "refresh_token": refresh,
        "token_type": "Bearer",
        "expires_in": idp.access_ttl,
    }))
    .into_response()
}

fn invalid_grant() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "invalid_grant"})),
    )
        .into_response()
}

async fn token(State(idp): State<Idp>, Form(form): Form<HashMap<String, String>>) -> Response {
    match form.get("grant_type").map(String::as_str) {
        Some("authorization_code") => {
            assert_eq!(form["redirect_uri"], REDIRECT_URI);
            let Some(challenge) = idp.codes.lock().unwrap().remove(&form["code"]) else {
                return invalid_grant();
            };
            // PKCE: the verifier must hash to the challenge sent in the authorize request.
            let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes()));
            if expected != challenge {
                return invalid_grant();
            }
            issue(&idp)
        }
        Some("refresh_token") => {
            let valid = !*idp.revoked.lock().unwrap()
                && idp.refresh_token.lock().unwrap().as_deref()
                    == Some(form["refresh_token"].as_str());
            if !valid {
                return invalid_grant();
            }
            *idp.refreshes.lock().unwrap() += 1;
            issue(&idp)
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "unsupported_grant_type"})),
        )
            .into_response(),
    }
}

async fn userinfo(State(idp): State<Idp>, headers: HeaderMap) -> Response {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_owned();
    if !idp.access_tokens.lock().unwrap().contains(&bearer) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // The agent backend's profile endpoint names the subject `id`, so use that name here.
    Json(json!({"id": "user-42", "email": "user@example.com", "name": "Test User"})).into_response()
}

/// Starts the fake provider on a random port and returns its base URL and shared state.
async fn start_idp(access_ttl: u64) -> (String, Idp) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let idp: Idp = Arc::new(IdpState {
        access_ttl,
        ..Default::default()
    });
    let app = Router::new()
        .route("/auth/sso/authorize", get(authorize))
        .route("/auth/sso/token", post(token))
        .route("/auth/me", get(userinfo))
        .with_state(idp.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (base, idp)
}

/// `provider` is the fake provider's base URL, where the SSO endpoints live.
fn app(provider: &str, cookie_secure: bool) -> Router {
    let identity = OAuth2IdentityProvider::new(OAuth2Config {
        identity_url: provider.into(),
        client_id: "central-logs".into(),
        redirect_uri: REDIRECT_URI.into(),
    });
    let web = WebSettings {
        frontend_url: FRONTEND.into(),
        cookie_secure,
    };
    router(AppState::in_memory(
        Arc::new(identity),
        web,
        Arc::new(AesSecretSealer::new([7; 32])),
    ))
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

/// The `name=value` pair of the named cookie in this response's `Set-Cookie` headers.
fn cookie_pair(response: &Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|c| c.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no {name} cookie in response"))
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

/// Plays the browser: login -> identity provider -> callback. Returns the callback response.
/// The browser keeps the login cookie it received at `/login` and sends it back at the callback.
async fn sign_in(app: &Router, return_to: &str) -> Response {
    let start = get_raw(app, &format!("/api/auth/login?return_to={return_to}"), None).await;
    assert_eq!(start.status(), StatusCode::FOUND);
    let login_cookie = cookie_pair(&start, OAUTH);
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
    get_raw(app, &callback, Some(&login_cookie)).await
}

#[tokio::test]
async fn login_flow_sets_session_cookie_and_exposes_the_user() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, true);

    let callback = sign_in(&app, "/projects/7").await;

    assert_eq!(callback.status(), StatusCode::FOUND);
    assert_eq!(location(&callback), format!("{FRONTEND}/projects/7"));
    let set: Vec<String> = callback
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    let session_set = set.iter().find(|c| c.starts_with(SESSION)).unwrap();
    assert!(session_set.contains("HttpOnly"));
    assert!(session_set.contains("SameSite=Lax"));
    assert!(session_set.contains("Secure"));
    // The login cookie is cleared once the login completes.
    let oauth_set = set.iter().find(|c| c.starts_with(OAUTH)).unwrap();
    assert!(oauth_set.contains("Max-Age=0"));

    let cookie = cookie_pair(&callback, SESSION);
    let me = get_raw(&app, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(me.status(), StatusCode::OK);
    assert_eq!(
        body_json(me).await,
        json!({"subject": "user-42", "email": "user@example.com", "name": "Test User"})
    );
}

#[tokio::test]
async fn me_requires_a_session() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);

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
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);
    let start = get_raw(&app, "/api/auth/login", None).await;
    let login_cookie = cookie_pair(&start, OAUTH);
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
        get_raw(&app, &callback, Some(&login_cookie)).await.status(),
        StatusCode::FOUND
    );

    // Replaying the same callback (same code, state and cookie) must fail.
    let replay = get_raw(&app, &callback, Some(&login_cookie)).await;
    assert_eq!(replay.status(), StatusCode::FOUND);
    assert_eq!(
        location(&replay),
        format!("{FRONTEND}/?auth_error=invalid_state")
    );
}

#[tokio::test]
async fn callback_from_a_browser_that_did_not_start_the_login_is_rejected() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);
    let start = get_raw(&app, "/api/auth/login", None).await;
    let state = query_of(&location(&start))["state"].clone();

    // No login cookie, and a login cookie from some other browser, both fail without a session.
    for cookie in [None, Some("central_logs_oauth=someone-else")] {
        let callback = get_raw(
            &app,
            &format!("/api/auth/callback?code=x&state={state}"),
            cookie,
        )
        .await;
        assert_eq!(callback.status(), StatusCode::FOUND);
        assert_eq!(
            location(&callback),
            format!("{FRONTEND}/?auth_error=invalid_state")
        );
        assert!(callback.headers().get(header::SET_COOKIE).is_some());
        assert!(
            callback
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .all(|c| !c.to_str().unwrap().starts_with(SESSION)),
            "no session cookie may be set"
        );
    }
}

#[tokio::test]
async fn callback_reports_forged_state_missing_parameters_and_provider_errors() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);

    let forged = get_raw(&app, "/api/auth/callback?code=x&state=nope", None).await;
    assert_eq!(
        location(&forged),
        format!("{FRONTEND}/?auth_error=invalid_state")
    );

    let missing = get_raw(&app, "/api/auth/callback", None).await;
    assert_eq!(missing.status(), StatusCode::FOUND);
    assert_eq!(
        location(&missing),
        format!("{FRONTEND}/?auth_error=invalid_state")
    );

    // A provider error for a real login is reported as that error, and the login is consumed.
    let start = get_raw(&app, "/api/auth/login", None).await;
    let login_cookie = cookie_pair(&start, OAUTH);
    let state = query_of(&location(&start))["state"].clone();
    let denied = get_raw(
        &app,
        &format!("/api/auth/callback?error=access_denied&state={state}"),
        Some(&login_cookie),
    )
    .await;
    assert_eq!(
        location(&denied),
        format!("{FRONTEND}/?auth_error=access_denied")
    );
    let retry = get_raw(
        &app,
        &format!("/api/auth/callback?code=x&state={state}"),
        Some(&login_cookie),
    )
    .await;
    assert_eq!(
        location(&retry),
        format!("{FRONTEND}/?auth_error=invalid_state")
    );
}

#[tokio::test]
async fn bad_authorization_code_redirects_with_login_failed_without_leaking_details() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);
    let start = get_raw(&app, "/api/auth/login", None).await;
    let login_cookie = cookie_pair(&start, OAUTH);
    let state = query_of(&location(&start))["state"].clone();

    let callback = get_raw(
        &app,
        &format!("/api/auth/callback?code=not-issued&state={state}"),
        Some(&login_cookie),
    )
    .await;

    assert_eq!(callback.status(), StatusCode::FOUND);
    assert_eq!(
        location(&callback),
        format!("{FRONTEND}/?auth_error=login_failed")
    );
    assert!(
        callback
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .all(|c| !c.to_str().unwrap().starts_with(SESSION)),
        "a failed login must not set a session cookie"
    );
}

#[tokio::test]
async fn login_does_not_contact_the_provider_and_binds_the_state_to_this_browser() {
    // No discovery: the provider is only reached when the browser returns with a code.
    let app = app("http://127.0.0.1:9", false);

    let start = get_raw(&app, "/api/auth/login", None).await;

    assert_eq!(start.status(), StatusCode::FOUND);
    assert!(location(&start).starts_with("http://127.0.0.1:9/auth/sso/authorize?"));
    assert_eq!(query_of(&location(&start))["scope"], "profile");
    let state = query_of(&location(&start))["state"].clone();
    assert_eq!(cookie_pair(&start, OAUTH), format!("{OAUTH}={state}"));
}

#[tokio::test]
async fn unreachable_provider_is_reported_without_crashing() {
    let app = app("http://127.0.0.1:9", false);
    let start = get_raw(&app, "/api/auth/login", None).await;
    let login_cookie = cookie_pair(&start, OAUTH);
    let state = query_of(&location(&start))["state"].clone();

    let callback = get_raw(
        &app,
        &format!("/api/auth/callback?code=any&state={state}"),
        Some(&login_cookie),
    )
    .await;

    assert_eq!(callback.status(), StatusCode::FOUND);
    assert_eq!(
        location(&callback),
        format!("{FRONTEND}/?auth_error=login_failed")
    );
}

#[tokio::test]
async fn open_redirect_in_return_to_is_rejected() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);

    let response = get_raw(&app, "/api/auth/login?return_to=https://evil.test", None).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn expired_access_token_is_refreshed_and_the_refresh_token_rotated() {
    // The provider issues one-second access tokens, so the next request after a pause must refresh.
    let (provider, idp) = start_idp(1).await;
    let app = app(&provider, false);
    let cookie = cookie_pair(&sign_in(&app, "/").await, SESSION);

    tokio::time::sleep(Duration::from_millis(1100)).await;
    let refreshed = get_raw(&app, "/api/auth/me", Some(&cookie)).await;

    assert_eq!(refreshed.status(), StatusCode::OK);
    assert_eq!(body_json(refreshed).await["subject"], "user-42");
    assert_eq!(*idp.refreshes.lock().unwrap(), 1);

    // The new access token is valid, so no further refresh happens.
    assert_eq!(
        get_raw(&app, "/api/auth/me", Some(&cookie)).await.status(),
        StatusCode::OK
    );
    assert_eq!(*idp.refreshes.lock().unwrap(), 1);
}

#[tokio::test]
async fn refresh_rejected_by_the_provider_signs_the_user_out() {
    let (provider, idp) = start_idp(1).await;
    let app = app(&provider, false);
    let cookie = cookie_pair(&sign_in(&app, "/").await, SESSION);
    *idp.revoked.lock().unwrap() = true;

    tokio::time::sleep(Duration::from_millis(1100)).await;

    let first = get_raw(&app, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(first.status(), StatusCode::UNAUTHORIZED);
    // The session is gone, so it stays signed out even if the provider recovers.
    *idp.revoked.lock().unwrap() = false;
    let second = get_raw(&app, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_ends_the_session_and_clears_the_cookie() {
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);
    let cookie = cookie_pair(&sign_in(&app, "/").await, SESSION);

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
    let (provider, _) = start_idp(3600).await;
    let app = app(&provider, false);
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
