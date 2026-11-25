//! PostgreSQL adapter: persists projects and log repositories.
//!
//! The ports are synchronous and are called from `spawn_blocking`, so the adapter drives the
//! async `sqlx` driver with `Handle::block_on`. Schema changes live in `migrations/` and are
//! applied with the `console` binary.

mod config;
mod migrations;
mod store;

pub use config::DbConfig;
pub use migrations::{MigrateError, migrate};
pub use store::{PgPool, PgStore, connect};
