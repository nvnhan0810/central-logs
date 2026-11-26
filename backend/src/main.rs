use std::io::{Error, ErrorKind};
use std::sync::Arc;

use central_logs_backend::infrastructure::oauth2::{
    DEFAULT_CLIENT_ID, DEFAULT_IDENTITY_URL, OAuth2Config, OAuth2IdentityProvider,
};
use central_logs_backend::infrastructure::postgres::{DbConfig, connect};
use central_logs_backend::infrastructure::session_crypto::AesSecretSealer;
use central_logs_backend::interfaces::http::{AppState, CALLBACK_PATH, WebSettings, router};

const DEFAULT_FRONTEND_URL: &str = "http://localhost:3000";

/// Validates an origin such as `https://logs.example.com` or `http://localhost:5173`.
/// A trailing slash is dropped, because browsers send origins without one.
fn parse_origin(name: &str, raw: &str) -> std::io::Result<String> {
    let s = raw.trim();
    let origin = s.trim_end_matches('/');
    let (scheme, host) = origin.split_once("://").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("{name} is not a valid origin: {s}"),
        )
    })?;
    let valid_scheme = matches!(scheme, "http" | "https");
    let valid_host = !host.is_empty() && !host.contains(['/', '?', '#', ' ']);
    if !valid_scheme || !valid_host {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{name} must be scheme://host[:port] with no path, got: {s}"),
        ));
    }
    Ok(origin.to_string())
}

fn required(name: &str) -> std::io::Result<String> {
    std::env::var(name)
        .map_err(|_| Error::new(ErrorKind::InvalidInput, format!("{name} must be set")))
}

/// Checks that `raw` is an absolute http(s) URL and returns it unchanged.
fn parse_http_url(name: &str, raw: String) -> std::io::Result<String> {
    let url = reqwest::Url::parse(&raw).map_err(|e| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("{name} is not a valid URL: {e}"),
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{name} must be an http or https URL"),
        ));
    }
    Ok(raw)
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Optional local settings file; variables already set in the environment take precedence.
    let _ = dotenvy::dotenv();
    let addr = std::env::var("CENTRAL_LOGS_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());

    let identity = OAuth2IdentityProvider::new(OAuth2Config {
        identity_url: match optional("CENTRAL_LOGS_IDENTITY_URL") {
            Some(url) => parse_http_url("CENTRAL_LOGS_IDENTITY_URL", url)?,
            None => DEFAULT_IDENTITY_URL.into(),
        },
        client_id: optional("CENTRAL_LOGS_IDENTITY_CLIENT_ID")
            .unwrap_or_else(|| DEFAULT_CLIENT_ID.into()),
        // Public origin of this backend; the callback route is served at `CALLBACK_PATH` under it.
        redirect_uri: format!(
            "{}{CALLBACK_PATH}",
            parse_origin(
                "CENTRAL_LOGS_BACKEND_URL",
                &required("CENTRAL_LOGS_BACKEND_URL")?
            )?
        ),
    });
    let web = WebSettings {
        // The one frontend: where the browser is sent after login, and the only origin allowed to call the API.
        frontend_url: parse_origin(
            "CENTRAL_LOGS_FRONTEND_URL",
            &optional("CENTRAL_LOGS_FRONTEND_URL").unwrap_or_else(|| DEFAULT_FRONTEND_URL.into()),
        )?,
        // Secure by default; set CENTRAL_LOGS_COOKIE_SECURE=false only for plain-HTTP local development.
        cookie_secure: optional("CENTRAL_LOGS_COOKIE_SECURE")
            .is_none_or(|v| !matches!(v.as_str(), "false" | "0")),
    };

    // Encrypts refresh tokens at rest. Required: without it, refresh tokens could not be stored safely.
    let sealer = AesSecretSealer::from_base64(&required("CENTRAL_LOGS_SESSION_KEY")?)
        .map_err(|message| Error::new(ErrorKind::InvalidInput, message))?;

    let db = DbConfig::from_env()?;
    // Connecting is part of startup: an unreachable database stops the server before it listens.
    let pool = connect(&db).await.map_err(|e| {
        Error::other(format!(
            "could not connect to PostgreSQL at {}:{}: {e}",
            db.host, db.port
        ))
    })?;

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("central-logs backend listening on http://{addr}");
    axum::serve(
        listener,
        router(AppState::with_postgres(
            Arc::new(identity),
            web,
            Arc::new(sealer),
            pool,
            tokio::runtime::Handle::current(),
        )),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::parse_origin;

    #[test]
    fn drops_a_trailing_slash() {
        assert_eq!(
            parse_origin("X", " https://logs.example.com/ ").unwrap(),
            "https://logs.example.com"
        );
    }

    #[test]
    fn accepts_a_port() {
        assert_eq!(
            parse_origin("X", "http://localhost:5173").unwrap(),
            "http://localhost:5173"
        );
    }

    #[test]
    fn rejects_a_path_a_missing_scheme_or_a_bad_scheme() {
        assert!(parse_origin("X", "http://localhost:5173/app").is_err());
        assert!(parse_origin("X", "localhost:5173").is_err());
        assert!(parse_origin("X", "ftp://example.com").is_err());
        assert!(parse_origin("X", "http://").is_err());
        assert!(parse_origin("X", "").is_err());
    }
}
