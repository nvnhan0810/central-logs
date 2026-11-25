//! Login, callback, current user and logout. The browser holds only an opaque session cookie.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::AppState;
use super::error::ApiError;
use super::handlers::blocking;
use crate::application::auth::{Identity, SESSION_TTL};
use crate::application::error::AppError;

pub const SESSION_COOKIE: &str = "central_logs_session";

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

/// `GET /api/auth/login`: redirects the browser to the identity provider.
pub async fn login(
    State(state): State<AppState>,
    Query(params): Query<LoginParams>,
) -> Result<Response, ApiError> {
    let auth = state.auth.clone();
    let url = blocking(move || auth.begin_login(params.return_to.as_deref())).await?;
    Ok((StatusCode::FOUND, [(header::LOCATION, url)]).into_response())
}

/// `GET /api/auth/callback`: completes the login, sets the session cookie and redirects to the frontend.
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Result<Response, ApiError> {
    if params.error.is_some() {
        return Err(AppError::Validation("login was not completed".into()).into());
    }
    let (Some(code), Some(login_state)) = (params.code, params.state) else {
        return Err(AppError::Validation("missing code or state".into()).into());
    };

    let auth = state.auth.clone();
    let done = blocking(move || auth.complete_login(&login_state, &code)).await?;

    let location = format!(
        "{}{}",
        state.web.frontend_url.trim_end_matches('/'),
        done.return_to
    );
    let cookie = session_cookie(&done.session_id, state.web.cookie_secure);
    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, location), (header::SET_COOKIE, cookie)],
    )
        .into_response())
}

/// `GET /api/auth/me`: the signed-in user, or 401.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserResponse>, ApiError> {
    let Some(session_id) = session_id_from(&headers) else {
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
    if let Some(session_id) = session_id_from(&headers) {
        let auth = state.auth.clone();
        blocking(move || auth.logout(&session_id)).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            clear_session_cookie(state.web.cookie_secure),
        )],
    )
        .into_response())
}

fn session_id_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
        .filter(|value| !value.is_empty())
}

fn session_cookie(session_id: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={session_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_TTL.as_secs(),
        secure_attr(secure)
    )
}

fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        secure_attr(secure)
    )
}

fn secure_attr(secure: bool) -> &'static str {
    if secure { "; Secure" } else { "" }
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
    fn finds_session_cookie_among_others() {
        let h = headers("theme=dark; central_logs_session=abc123; lang=vi");
        assert_eq!(session_id_from(&h).as_deref(), Some("abc123"));
    }

    #[test]
    fn ignores_missing_or_empty_session_cookie() {
        assert_eq!(session_id_from(&headers("theme=dark")), None);
        assert_eq!(session_id_from(&headers("central_logs_session=")), None);
        assert_eq!(session_id_from(&HeaderMap::new()), None);
    }

    #[test]
    fn session_cookie_is_http_only_and_secure_when_configured() {
        let secure = session_cookie("id", true);
        assert!(secure.starts_with("central_logs_session=id; Path=/; HttpOnly; SameSite=Lax"));
        assert!(secure.ends_with("; Secure"));
        assert!(!session_cookie("id", false).contains("Secure"));
    }

    #[test]
    fn clear_cookie_expires_immediately() {
        assert!(clear_session_cookie(false).contains("Max-Age=0"));
    }
}
