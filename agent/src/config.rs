//! `config.toml` (spec 5.5). M1 covers `[general]` and `[pairing]`; later
//! milestones add `[builtin]` and `[[commands]]`.

use crate::AgentError;
use crate::fsutil::write_atomic;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub pairing: Pairing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub name: String,
    pub port: u16,
}

impl Default for General {
    fn default() -> Self {
        Self {
            name: gethostname::gethostname().to_string_lossy().into_owned(),
            port: lanpilot_core::transport::DEFAULT_PORT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Pairing {
    pub password_enabled: bool,
}

impl Config {
    pub fn load_or_default(path: &Path) -> Result<Self, AgentError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| AgentError::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), AgentError> {
        let text = toml::to_string_pretty(self).map_err(|e| AgentError::Config(e.to_string()))?;
        write_atomic(path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load_or_default(&dir.path().join("config.toml")).unwrap();
        assert_eq!(c.general.port, lanpilot_core::transport::DEFAULT_PORT);
        assert!(!c.general.name.is_empty());
        assert!(!c.pairing.password_enabled);
    }

    #[test]
    fn partial_file_fills_defaults_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nname = \"书房台式机\"\n").unwrap();
        let mut c = Config::load_or_default(&path).unwrap();
        assert_eq!(c.general.name, "书房台式机");
        assert_eq!(c.general.port, lanpilot_core::transport::DEFAULT_PORT);
        c.pairing.password_enabled = true;
        c.save(&path).unwrap();
        assert_eq!(Config::load_or_default(&path).unwrap(), c);
    }

    #[test]
    fn invalid_toml_is_a_config_error_with_location() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nport = \"x\"\n").unwrap();
        match Config::load_or_default(&path) {
            Err(AgentError::Config(msg)) => assert!(msg.contains("port"), "{msg}"),
            other => panic!("expected config error, got {other:?}"),
        }
    }
}
