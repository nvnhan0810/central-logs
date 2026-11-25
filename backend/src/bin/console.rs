//! Operator console: maintenance commands for the backend, such as database migrations.
//!
//! Run from the `backend` directory, reading `DB_*` settings from `.env` if present:
//!
//! ```sh
//! cargo run --bin console -- migrate
//! ```

use std::process::ExitCode;

use central_logs_backend::infrastructure::postgres::{DbConfig, migrate};

const USAGE: &str = "usage: console <command>

commands:
  migrate   create the database if it is missing, then apply pending migrations";

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    match std::env::args().nth(1).as_deref() {
        Some("migrate") => run_migrate().await,
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

async fn run_migrate() -> ExitCode {
    let config = match DbConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    match migrate(&config).await {
        Ok(applied) if applied.is_empty() => {
            println!("database {} is up to date", config.name);
            ExitCode::SUCCESS
        }
        Ok(applied) => {
            for name in applied {
                println!("applied {name}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
