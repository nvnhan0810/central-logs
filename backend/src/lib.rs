//! Central Logs backend.
//!
//! Layers (dependencies point inwards):
//! - `domain`: business rules, no I/O.
//! - `application`: CQRS command and query services, plus the ports they need.
//! - `infrastructure`: adapters implementing the ports (in-memory store, file system).
//! - `interfaces`: HTTP API (axum).

pub mod application;
pub mod domain;
pub mod infrastructure;
