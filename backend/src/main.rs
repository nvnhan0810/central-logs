use std::io::{Error, ErrorKind};
use std::sync::Arc;

use central_logs_backend::infrastructure::oidc::{OidcConfig, OidcIdentityProvider};
use central_logs_backend::infrastructure::postgres::{DbConfig, connect};
use central_logs_backend::interfaces::http::{AppState, WebSettings, router};

const DEFAULT_ISSUER: &str = "https://identity.nvnhan0810.com";
const DEFAULT_FRONTEND_URL: &str = "http://localhost:3000";
const DEFAULT_CORS_ORIGINS: &str = "http://localhost:5173";

/// Parses a comma-separated list of origins such as `http://localhost:5173,https://logs.example.com`.
/// A trailing slash is dropped, because browsers send origins without one.
fn parse_cors_origins(raw: &str) -> std::io::Result<Vec<String>> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let origin = s.trim_end_matches('/');
            let (scheme, host) = origin.split_once("://").ok_or_else(|| {
                Error::new(ErrorKind::InvalidInput, format!("invalid CORS origin: {s}"))
            })?;
            let valid_scheme = matches!(scheme, "http" | "https");
            let valid_host = !host.is_empty() && !host.contains(['/', '?', '#', ' ']);
            if !valid_scheme || !valid_host {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid CORS origin (expected scheme://host[:port]): {s}"),
                ));
            }
            Ok(origin.to_string())
        })
        .collect()
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
        frontend_url: optional("CENTRAL_LOGS_FRONTEND_URL")
            .unwrap_or_else(|| DEFAULT_FRONTEND_URL.into()),
        // Secure by default; set CENTRAL_LOGS_COOKIE_SECURE=false only for plain-HTTP local development.
        cookie_secure: optional("CENTRAL_LOGS_COOKIE_SECURE")
            .is_none_or(|v| !matches!(v.as_str(), "false" | "0")),
        cors_origins: parse_cors_origins(
            &optional("CENTRAL_LOGS_CORS_ORIGINS").unwrap_or_else(|| DEFAULT_CORS_ORIGINS.into()),
        )?,
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
    if web.cors_origins.is_empty() {
        println!("CORS: no frontend origins allowed (CENTRAL_LOGS_CORS_ORIGINS is empty)");
    }
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
    use super::parse_cors_origins;

    #[test]
    fn splits_trims_and_drops_trailing_slashes() {
        let origins =
            parse_cors_origins(" http://localhost:5173/ , https://logs.example.com ,").unwrap();
        assert_eq!(
            origins,
            ["http://localhost:5173", "https://logs.example.com"]
        );
    }

    #[test]
    fn empty_value_means_no_origins() {
        assert!(parse_cors_origins("").unwrap().is_empty());
    }

    #[test]
    fn rejects_origins_with_a_path_or_bad_scheme() {
        assert!(parse_cors_origins("http://localhost:5173/app").is_err());
        assert!(parse_cors_origins("localhost:5173").is_err());
        assert!(parse_cors_origins("ftp://example.com").is_err());
        assert!(parse_cors_origins("http://").is_err());
    }
}
