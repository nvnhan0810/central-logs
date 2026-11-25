//! Database connection settings, read from the `DB_*` environment variables.

use std::fmt;
use std::io::{Error, ErrorKind};

use sqlx::postgres::PgConnectOptions;

#[derive(Clone, PartialEq, Eq)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub username: String,
    pub password: String,
}

// Hand-written so the password never ends up in logs or panic messages.
impl fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("name", &self.name)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl DbConfig {
    /// Reads `DB_HOST`, `DB_PORT`, `DB_NAME`, `DB_USERNAME` and `DB_PASSWORD`.
    /// All are required; the password may be empty for servers that trust the connection.
    pub fn from_env() -> std::io::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> std::io::Result<Self> {
        let required = |key: &str| {
            lookup(key)
                .filter(|v| !v.trim().is_empty())
                .map(|v| v.trim().to_owned())
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, format!("{key} must be set")))
        };
        let port_raw = required("DB_PORT")?;
        let port = port_raw.parse::<u16>().map_err(|_| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("DB_PORT must be a port number, got {port_raw:?}"),
            )
        })?;
        Ok(Self {
            host: required("DB_HOST")?,
            port,
            name: required("DB_NAME")?,
            username: required("DB_USERNAME")?,
            password: lookup("DB_PASSWORD")
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "DB_PASSWORD must be set"))?,
        })
    }

    /// Connection settings for the application database.
    pub fn connect_options(&self) -> PgConnectOptions {
        self.options_for(&self.name)
    }

    /// Connection settings for another database on the same server, used to create the target.
    pub(crate) fn options_for(&self, database: &str) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(database)
            .username(&self.username)
            .password(&self.password)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn complete() -> Vec<(&'static str, &'static str)> {
        vec![
            ("DB_HOST", "localhost"),
            ("DB_PORT", "5432"),
            ("DB_NAME", "central_logs"),
            ("DB_USERNAME", "central"),
            ("DB_PASSWORD", "secret"),
        ]
    }

    #[test]
    fn reads_all_settings() {
        let config = DbConfig::from_lookup(lookup(&complete())).unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.name, "central_logs");
        assert_eq!(config.username, "central");
        assert_eq!(config.password, "secret");
    }

    #[test]
    fn reports_each_missing_variable() {
        for key in [
            "DB_HOST",
            "DB_PORT",
            "DB_NAME",
            "DB_USERNAME",
            "DB_PASSWORD",
        ] {
            let pairs: Vec<_> = complete().into_iter().filter(|(k, _)| *k != key).collect();
            let err = DbConfig::from_lookup(lookup(&pairs)).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput);
            assert!(err.to_string().contains(key), "{key}: {err}");
        }
    }

    #[test]
    fn empty_password_is_allowed_but_empty_name_is_not() {
        let mut pairs = complete();
        pairs.retain(|(k, _)| *k != "DB_PASSWORD");
        pairs.push(("DB_PASSWORD", ""));
        assert_eq!(DbConfig::from_lookup(lookup(&pairs)).unwrap().password, "");

        let mut pairs = complete();
        pairs.retain(|(k, _)| *k != "DB_NAME");
        pairs.push(("DB_NAME", "   "));
        assert!(DbConfig::from_lookup(lookup(&pairs)).is_err());
    }

    #[test]
    fn rejects_a_port_that_is_not_a_number() {
        let mut pairs = complete();
        pairs.retain(|(k, _)| *k != "DB_PORT");
        pairs.push(("DB_PORT", "not-a-port"));
        let err = DbConfig::from_lookup(lookup(&pairs)).unwrap_err();
        assert!(err.to_string().contains("DB_PORT"));
    }

    #[test]
    fn debug_output_hides_the_password() {
        let config = DbConfig::from_lookup(lookup(&complete())).unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains("secret"));
        assert!(shown.contains("<redacted>"));
    }
}
