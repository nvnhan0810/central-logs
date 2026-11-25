//! OpenID Connect adapter for the identity provider port.
//!
//! Uses discovery to find the endpoints, the authorization code flow with PKCE (S256), and the
//! `userinfo` endpoint to read the user's claims. The token response is received over TLS directly
//! from the provider, so its ID token is not re-verified here.
//!
//! Methods perform blocking HTTP and must be called from `spawn_blocking`, like the other ports.

use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::application::auth::Identity;
use crate::application::error::AppError;
use crate::application::ports::IdentityProvider;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// Issuer identifier, e.g. `https://identity.example.com`.
    pub issuer: String,
    pub client_id: String,
    /// Optional for public clients that rely on PKCE alone.
    pub client_secret: Option<String>,
    /// Must match a redirect URI registered with the provider, e.g. `https://logs.example.com/api/auth/callback`.
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct UserInfo {
    sub: String,
    email: Option<String>,
    name: Option<String>,
}

pub struct OidcIdentityProvider {
    config: OidcConfig,
    /// Discovery is fetched on first use, so the server can start while the provider is down.
    discovery: Mutex<Option<Discovery>>,
}

impl OidcIdentityProvider {
    pub fn new(config: OidcConfig) -> Self {
        Self {
            config,
            discovery: Mutex::new(None),
        }
    }

    fn discovery(&self) -> Result<Discovery, AppError> {
        let mut cached = self
            .discovery
            .lock()
            .map_err(|_| AppError::Storage("discovery lock poisoned".into()))?;
        if let Some(found) = cached.as_ref() {
            return Ok(found.clone());
        }

        let url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer.trim_end_matches('/')
        );
        let found: Discovery = client()?
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| upstream("discovery", e))?;
        if found.issuer.trim_end_matches('/') != self.config.issuer.trim_end_matches('/') {
            return Err(AppError::Upstream(
                "discovery issuer does not match configured issuer".into(),
            ));
        }
        *cached = Some(found.clone());
        Ok(found)
    }
}

impl IdentityProvider for OidcIdentityProvider {
    fn authorization_url(&self, state: &str, code_verifier: &str) -> Result<String, AppError> {
        let endpoint = self.discovery()?.authorization_endpoint;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        reqwest::Url::parse_with_params(
            &endpoint,
            &[
                ("response_type", "code"),
                ("client_id", self.config.client_id.as_str()),
                ("redirect_uri", self.config.redirect_uri.as_str()),
                ("scope", "openid email profile"),
                ("state", state),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
            ],
        )
        .map(String::from)
        .map_err(|e| AppError::Upstream(format!("invalid authorization endpoint: {e}")))
    }

    fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<Identity, AppError> {
        let discovery = self.discovery()?;
        let http = client()?;

        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("client_id", self.config.client_id.as_str()),
            ("code_verifier", code_verifier),
        ];
        if let Some(secret) = self.config.client_secret.as_deref() {
            form.push(("client_secret", secret));
        }
        let tokens: TokenResponse = http
            .post(&discovery.token_endpoint)
            .form(&form)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| upstream("token exchange", e))?;

        let info: UserInfo = http
            .get(&discovery.userinfo_endpoint)
            .bearer_auth(&tokens.access_token)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| upstream("userinfo", e))?;
        if info.sub.trim().is_empty() {
            return Err(AppError::Upstream(
                "userinfo response has no subject".into(),
            ));
        }
        Ok(Identity {
            subject: info.sub,
            email: info.email,
            name: info.name,
        })
    }
}

fn client() -> Result<Client, AppError> {
    // Built per call: login is infrequent, and a blocking client must not be dropped inside the async runtime.
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| AppError::Storage(format!("http client: {e}")))
}

fn upstream(step: &str, error: reqwest::Error) -> AppError {
    AppError::Upstream(format!("{step} failed: {error}"))
}
