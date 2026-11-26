//! Login, callback, current user and logout. The browser holds only opaque cookies: the session id,
//! and the login `state` while a login is in progress.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::AppState;
use super::error::ApiError;
use super::handlers::blocking;
use crate::application::auth::{CompletedLogin, Identity, LOGIN_TTL, SESSION_TTL};
use crate::application::error::AppError;

pub const SESSION_COOKIE: &str = "central_logs_session";
/// Holds the login `state` from `/login` until `/callback`, so only the browser that started a login can finish it.
pub const OAUTH_COOKIE: &str = "central_logs_oauth";

#[derive(Debug, Deserialize)]
pub struct LoginParams {
    /// Relative path in the frontend to return to after login, e.g. `/projects/1`.
    pub return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    /// Set by the provider when the user cancels or the request is rejected.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

impl From<Identity> for UserResponse {
    fn from(identity: Identity) -> Self {
        Self {
            subject: identity.subject,
            email: identity.email,
            name: identity.name,
        }
    }
}

/// `GET /api/auth/login`: redirects the browser to the identity provider and binds the login to this browser.
pub async fn login(
    State(state): State<AppState>,
    Query(params): Query<LoginParams>,
) -> Result<Response, ApiError> {
    let auth = state.auth.clone();
    let start = blocking(move || auth.begin_login(params.return_to.as_deref())).await?;
    let cookie = set_cookie(
        OAUTH_COOKIE,
        &start.state,
        LOGIN_TTL.as_secs(),
        state.web.cookie_secure,
    );
    redirect(start.authorization_url, vec![cookie])
}

/// `GET /api/auth/callback`: completes the login and redirects the browser to the frontend.
///
/// Every outcome is a redirect to the frontend, as in the agent backend. A failed login goes to
/// `/?auth_error=<reason>` and sets no session.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Result<Response, ApiError> {
    let secure = state.web.cookie_secure;
    let oauth_cookie = cookie_value(&headers, OAUTH_COOKIE);
    let login_state = params.state.clone().unwrap_or_default();
    let code = params.code.clone().unwrap_or_default();
    let provider_error = params.error.clone();

    let auth = state.auth.clone();
    let result = run_blocking(move || match provider_error {
        Some(_) => auth
            .reject_login(&login_state, oauth_cookie.as_deref())
            .map(|()| None),
        None => auth
            .complete_login(&login_state, oauth_cookie.as_deref(), &code)
            .map(Some),
    })
    .await;

    let clear_oauth = set_cookie(OAUTH_COOKIE, "", 0, secure);
    match result {
        Ok(Some(done)) => signed_in(&state, done, clear_oauth),
        other => {
            let reason = failure_reason(&other, params.error.as_deref());
            let frontend = &state.web.frontend_url;
            redirect(
                format!("{frontend}/?auth_error={reason}"),
                vec![clear_oauth],
            )
        }
    }
}

/// `GET /api/auth/me`: the signed-in user, or 401.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserResponse>, ApiError> {
    let Some(session_id) = cookie_value(&headers, SESSION_COOKIE) else {
        return Err(ApiError::unauthenticated());
    };
    let auth = state.auth.clone();
    let identity = blocking(move || auth.current_user(&session_id)).await?;
    identity
        .map(|i| Json(i.into()))
        .ok_or_else(ApiError::unauthenticated)
}

/// `POST /api/auth/logout`: ends the session and clears the cookie. Idempotent.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(session_id) = cookie_value(&headers, SESSION_COOKIE) {
        let auth = state.auth.clone();
        blocking(move || auth.logout(&session_id)).await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        set_cookie(SESSION_COOKIE, "", 0, state.web.cookie_secure),
    );
    Ok(response)
}

fn signed_in(
    state: &AppState,
    done: CompletedLogin,
    clear_oauth: HeaderValue,
) -> Result<Response, ApiError> {
    let location = format!(
        "{}{}",
        state.web.frontend_url.trim_end_matches('/'),
        done.return_to
    );
    let session = set_cookie(
        SESSION_COOKIE,
        &done.session_id,
        SESSION_TTL.as_secs(),
        state.web.cookie_secure,
    );
    redirect(location, vec![session, clear_oauth])
}

/// Runs a blocking use case and keeps its error kind, so the callback can tell a state mismatch from a provider failure.
async fn run_blocking<T, F>(work: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|_| Err(AppError::Storage("background task failed".into())))
}

/// Label shown to the user for a failed login. Labels match the agent backend.
fn failure_reason(
    result: &Result<Option<CompletedLogin>, AppError>,
    provider_error: Option<&str>,
) -> &'static str {
    match (result, provider_error) {
        (Err(AppError::Validation(_)), _) => "invalid_state",
        (Ok(None), Some("access_denied")) => "access_denied",
        (Ok(None), Some("temporarily_unavailable")) => "temporarily_unavailable",
        _ => "login_failed",
    }
}

fn redirect(location: String, cookies: Vec<HeaderValue>) -> Result<Response, ApiError> {
    let location = HeaderValue::try_from(location).map_err(|_| ApiError::internal())?;
    let mut response = (StatusCode::FOUND, [(header::LOCATION, location)]).into_response();
    for cookie in cookies {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    Ok(response)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
        .filter(|value| !value.is_empty())
}

/// A `Set-Cookie` value. Values are hex tokens or empty, so the header is always valid.
fn set_cookie(name: &str, value: &str, max_age: u64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::try_from(format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("cookie made of hex tokens and fixed attributes is a valid header value")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(cookie: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(header::COOKIE, cookie.parse().unwrap());
        map
    }

    #[test]
    fn finds_cookies_among_others() {
        let h = headers("theme=dark; central_logs_session=abc123; central_logs_oauth=st; lang=vi");
        assert_eq!(cookie_value(&h, SESSION_COOKIE).as_deref(), Some("abc123"));
        assert_eq!(cookie_value(&h, OAUTH_COOKIE).as_deref(), Some("st"));
    }

    #[test]
    fn ignores_missing_or_empty_cookies() {
        assert_eq!(cookie_value(&headers("theme=dark"), SESSION_COOKIE), None);
        assert_eq!(
            cookie_value(&headers("central_logs_session="), SESSION_COOKIE),
            None
        );
        assert_eq!(cookie_value(&HeaderMap::new(), SESSION_COOKIE), None);
    }

    #[test]
    fn cookies_are_http_only_and_secure_when_configured() {
        let secure = set_cookie("n", "id", 60, true);
        let secure = secure.to_str().unwrap();
        assert!(secure.starts_with("n=id; Path=/; HttpOnly; SameSite=Lax; Max-Age=60"));
        assert!(secure.ends_with("; Secure"));
        assert!(
            !set_cookie("n", "id", 60, false)
                .to_str()
                .unwrap()
                .contains("Secure")
        );
    }

    #[test]
    fn clearing_a_cookie_expires_it_immediately() {
        assert!(
            set_cookie(SESSION_COOKIE, "", 0, false)
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
    }

    #[test]
    fn failure_reasons_match_the_agent_labels() {
        assert_eq!(
            failure_reason(&Err(AppError::Validation("x".into())), None),
            "invalid_state"
        );
        assert_eq!(
            failure_reason(&Ok(None), Some("access_denied")),
            "access_denied"
        );
        assert_eq!(
            failure_reason(&Ok(None), Some("temporarily_unavailable")),
            "temporarily_unavailable"
        );
        assert_eq!(
            failure_reason(&Err(AppError::Upstream("x".into())), None),
            "login_failed"
        );
    }
}
