//! OAuth 2.0 authorization code adapter for the identity provider port.
//!
//! Endpoints are fixed paths under the identity base URL (no discovery). Login uses PKCE (S256),
//! exchanges the code at the token endpoint, and reads the user's claims from the userinfo endpoint
//! with the access token. Refresh uses the `refresh_token` grant.
//!
//! Methods perform blocking HTTP and must be called from `spawn_blocking`, like the other ports.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::application::auth::{Identity, IdentityTokens};
use crate::application::error::AppError;
use crate::application::ports::IdentityProvider;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
/// Used when the provider omits `expires_in`, the same default as the agent backend.
const DEFAULT_EXPIRES_IN_SECS: i64 = 900;

/// The identity service that hosts the SSO endpoints. Same provider as the agent backend.
pub const DEFAULT_IDENTITY_URL: &str = "https://backend.nvnhan0810.com";
/// Client registered with the identity service for central-logs.
pub const DEFAULT_CLIENT_ID: &str = "central-logs";
const AUTHORIZE_PATH: &str = "/auth/sso/authorize";
const TOKEN_PATH: &str = "/auth/sso/token";
const USERINFO_PATH: &str = "/auth/me";
const SCOPE: &str = "profile";

#[derive(Debug, Clone)]
pub struct OAuth2Config {
    /// Base URL of the identity service, without a trailing path, e.g. `https://backend.nvnhan0810.com`.
    pub identity_url: String,
    pub client_id: String,
    /// Must match a redirect URI registered with the provider, e.g. `https://logs.example.com/api/auth/callback`.
    pub redirect_uri: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "default_expires_in")]
    expires_in: i64,
}

fn default_expires_in() -> i64 {
    DEFAULT_EXPIRES_IN_SECS
}

#[derive(Deserialize)]
struct UserInfo {
    /// The agent backend's profile endpoint names this field `id`.
    #[serde(alias = "id")]
    sub: String,
    email: Option<String>,
    name: Option<String>,
}

pub struct OAuth2IdentityProvider {
    config: OAuth2Config,
}

impl OAuth2IdentityProvider {
    pub fn new(config: OAuth2Config) -> Self {
        Self { config }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.config.identity_url.trim_end_matches('/'))
    }

    /// Calls the token endpoint, then reads the claims. `refreshing` turns the provider's
    /// rejections into `Unauthorized`, so the caller can end the session instead of retrying.
    fn request_tokens(
        &self,
        form: &[(&str, &str)],
        refreshing: bool,
    ) -> Result<IdentityTokens, AppError> {
        let http = client()?;

        let response = http
            .post(self.endpoint(TOKEN_PATH))
            .form(form)
            .send()
            .map_err(|e| upstream("token request", e))?;
        if refreshing && matches!(response.status().as_u16(), 400 | 401 | 403) {
            return Err(AppError::Unauthorized(
                "refresh token was rejected by the identity provider".into(),
            ));
        }
        let tokens: TokenResponse = response
            .error_for_status()
            .and_then(Response::json)
            .map_err(|e| upstream("token request", e))?;
        if tokens.expires_in <= 0 {
            return Err(AppError::Upstream(
                "token response has a non-positive expires_in".into(),
            ));
        }

        let response = http
            .get(self.endpoint(USERINFO_PATH))
            .bearer_auth(&tokens.access_token)
            .send()
            .map_err(|e| upstream("userinfo", e))?;
        if refreshing && matches!(response.status().as_u16(), 401 | 403) {
            return Err(AppError::Unauthorized(
                "identity provider no longer grants access".into(),
            ));
        }
        let info: UserInfo = response
            .error_for_status()
            .and_then(Response::json)
            .map_err(|e| upstream("userinfo", e))?;
        if info.sub.trim().is_empty() {
            return Err(AppError::Upstream(
                "userinfo response has no subject".into(),
            ));
        }
        Ok(IdentityTokens {
            identity: Identity {
                subject: info.sub,
                email: info.email,
                name: info.name,
            },
            expires_in: Duration::from_secs(tokens.expires_in as u64),
            refresh_token: tokens.refresh_token,
        })
    }
}

impl IdentityProvider for OAuth2IdentityProvider {
    fn authorization_url(&self, state: &str, code_verifier: &str) -> Result<String, AppError> {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let params = vec![
            ("response_type", "code"),
            ("client_id", self.config.client_id.as_str()),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("state", state),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("scope", SCOPE),
        ];
        reqwest::Url::parse_with_params(&self.endpoint(AUTHORIZE_PATH), &params)
            .map(String::from)
            .map_err(|e| AppError::Upstream(format!("invalid authorization endpoint: {e}")))
    }

    fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<IdentityTokens, AppError> {
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("client_id", self.config.client_id.as_str()),
            ("code_verifier", code_verifier),
        ];
        self.request_tokens(&form, false)
    }

    fn refresh(&self, refresh_token: &str) -> Result<IdentityTokens, AppError> {
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.as_str()),
        ];
        self.request_tokens(&form, true)
    }
}

fn client() -> Result<Client, AppError> {
    // Built per call: login and refresh are infrequent, and a blocking client must not be dropped inside the async runtime.
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| AppError::Storage(format!("http client: {e}")))
}

fn upstream(step: &str, error: reqwest::Error) -> AppError {
    AppError::Upstream(format!("{step} failed: {error}"))
}
