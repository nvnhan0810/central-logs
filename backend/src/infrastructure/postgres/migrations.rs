//! Schema migrations (`backend/migrations/*.sql`), applied by the `console migrate` command.

use std::collections::HashSet;

use sqlx::migrate::{Migrate, Migrator};
use sqlx::{Connection, PgConnection};
use thiserror::Error;

use super::DbConfig;

/// Table where sqlx records which migrations have run (its default name).
const TABLE: &str = "_sqlx_migrations";

/// Migration files are embedded at compile time.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

/// Creates the database when it is missing, then applies pending migrations.
/// Returns the migrations applied in this run, as `<version> <description>`.
///
/// Creating the database needs a role with `CREATEDB`. If the database already exists, the
/// role only needs access to it.
pub async fn migrate(config: &DbConfig) -> Result<Vec<String>, MigrateError> {
    ensure_database(config).await?;

    let mut conn = PgConnection::connect_with(&config.connect_options()).await?;
    conn.ensure_migrations_table(TABLE).await?;
    let before: HashSet<i64> = conn
        .list_applied_migrations(TABLE)
        .await?
        .into_iter()
        .map(|m| m.version)
        .collect();
    MIGRATOR.run(&mut conn).await?;
    conn.close().await?;

    Ok(MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration() && !before.contains(&m.version))
        .map(|m| format!("{:04} {}", m.version, m.description))
        .collect())
}

async fn ensure_database(config: &DbConfig) -> Result<(), sqlx::Error> {
    // The `postgres` maintenance database always exists, so it is used to look up and create the target.
    let mut admin = PgConnection::connect_with(&config.options_for("postgres")).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.name)
            .fetch_one(&mut admin)
            .await?;
    if !exists {
        // Identifiers cannot be bound as parameters, so the name is quoted instead.
        let statement = format!("CREATE DATABASE {}", quote_identifier(&config.name));
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&mut admin)
            .await?;
    }
    admin.close().await
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::quote_identifier;

    #[test]
    fn quotes_identifiers_and_escapes_double_quotes() {
        assert_eq!(quote_identifier("central_logs"), "\"central_logs\"");
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
    }
}
