use std::io::{Error, ErrorKind};
use std::sync::Arc;

use central_logs_backend::infrastructure::oidc::{OidcConfig, OidcIdentityProvider};
use central_logs_backend::infrastructure::postgres::{DbConfig, connect};
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};

const DEFAULT_ISSUER: &str = "https://identity.nvnhan0810.com";
const DEFAULT_FRONTEND_URL: &str = "http://localhost:3000";

/// Validates the frontend origin, e.g. `https://logs.example.com` or `http://localhost:5173`.
/// A trailing slash is dropped, because browsers send origins without one.
fn parse_frontend_url(raw: &str) -> std::io::Result<String> {
    let s = raw.trim();
    let origin = s.trim_end_matches('/');
    let (scheme, host) = origin.split_once("://").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("invalid frontend URL: {s}"),
        )
    })?;
    let valid_scheme = matches!(scheme, "http" | "https");
    let valid_host = !host.is_empty() && !host.contains(['/', '?', '#', ' ']);
    if !valid_scheme || !valid_host {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("invalid frontend URL (expected scheme://host[:port], no path): {s}"),
        ));
    }
    Ok(origin.to_string())
}

fn required(name: &str) -> std::io::Result<String> {
    std::env::var(name)
        .map_err(|_| Error::new(ErrorKind::InvalidInput, format!("{name} must be set")))
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Optional local settings file; variables already set in the environment take precedence.
    let _ = dotenvy::dotenv();
    let addr = std::env::var("CENTRAL_LOGS_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());

    let identity = OidcIdentityProvider::new(OidcConfig {
        issuer: optional("CENTRAL_LOGS_OIDC_ISSUER").unwrap_or_else(|| DEFAULT_ISSUER.into()),
        client_id: required("CENTRAL_LOGS_OIDC_CLIENT_ID")?,
        client_secret: optional("CENTRAL_LOGS_OIDC_CLIENT_SECRET"),
        redirect_uri: required("CENTRAL_LOGS_OIDC_REDIRECT_URI")?,
    });
    let web = WebSettings {
        // The one frontend: where the browser is sent after login, and the only origin allowed to call the API.
        frontend_url: parse_frontend_url(
            &optional("CENTRAL_LOGS_FRONTEND_URL").unwrap_or_else(|| DEFAULT_FRONTEND_URL.into()),
        )?,
        // Secure by default; set CENTRAL_LOGS_COOKIE_SECURE=false only for plain-HTTP local development.
        cookie_secure: optional("CENTRAL_LOGS_COOKIE_SECURE")
            .is_none_or(|v| !matches!(v.as_str(), "false" | "0")),
    };

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
            pool,
            tokio::runtime::Handle::current(),
        )),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::parse_frontend_url;

    #[test]
    fn drops_a_trailing_slash() {
        assert_eq!(
            parse_frontend_url(" https://logs.example.com/ ").unwrap(),
            "https://logs.example.com"
        );
    }

    #[test]
    fn accepts_a_port() {
        assert_eq!(
            parse_frontend_url("http://localhost:5173").unwrap(),
            "http://localhost:5173"
        );
    }

    #[test]
    fn rejects_a_path_a_missing_scheme_or_a_bad_scheme() {
        assert!(parse_frontend_url("http://localhost:5173/app").is_err());
        assert!(parse_frontend_url("localhost:5173").is_err());
        assert!(parse_frontend_url("ftp://example.com").is_err());
        assert!(parse_frontend_url("http://").is_err());
        assert!(parse_frontend_url("").is_err());
    }
}
