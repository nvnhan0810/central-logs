//! Application layer: CQRS command and query services and the ports they use.

pub mod commands;
pub mod error;
pub mod ports;
pub mod queries;
pub mod views;

#[cfg(test)]
pub(crate) mod test_support;
