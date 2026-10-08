//! The LanPilot PC agent (spec section 5).

pub mod config;
pub mod devices;
pub mod fsutil;
pub mod pairing;
pub mod paths;
pub mod secret;
pub mod session;

use lanpilot_core::pairing::password::PasswordError;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("config: {0}")]
    Config(String),
    #[error("secret storage: {0}")]
    Secret(String),
    #[error(transparent)]
    Password(#[from] PasswordError),
    #[error("{0}")]
    Core(String),
}
