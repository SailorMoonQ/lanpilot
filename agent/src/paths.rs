//! Where the agent keeps its config and state (spec 5.5).

use crate::AgentError;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_file: PathBuf,
    pub state_dir: PathBuf,
}

impl Paths {
    /// Windows: `%APPDATA%\LanPilot\config.toml`, state in `%LOCALAPPDATA%\LanPilot`.
    /// Linux: `~/.config/lanpilot/config.toml`, state in `~/.local/state/lanpilot`.
    pub fn default_for_user() -> Result<Self, AgentError> {
        let missing = || AgentError::Config("cannot determine the user's home directories".into());
        if cfg!(windows) {
            Ok(Self {
                config_file: dirs::config_dir()
                    .ok_or_else(missing)?
                    .join("LanPilot")
                    .join("config.toml"),
                state_dir: dirs::data_local_dir().ok_or_else(missing)?.join("LanPilot"),
            })
        } else {
            let state_base = dirs::state_dir()
                .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
                .ok_or_else(missing)?;
            Ok(Self {
                config_file: dirs::config_dir()
                    .ok_or_else(missing)?
                    .join("lanpilot")
                    .join("config.toml"),
                state_dir: state_base.join("lanpilot"),
            })
        }
    }

    pub fn under(root: &Path) -> Self {
        Self {
            config_file: root.join("config.toml"),
            state_dir: root.join("state"),
        }
    }

    pub fn identity_file(&self) -> PathBuf {
        self.state_dir.join("identity.key")
    }

    pub fn password_file(&self) -> PathBuf {
        self.state_dir.join("pairing-password")
    }

    pub fn devices_file(&self) -> PathBuf {
        self.state_dir.join("devices.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_root_layout() {
        let p = Paths::under(Path::new("/tmp/lp"));
        assert_eq!(p.config_file, Path::new("/tmp/lp/config.toml"));
        assert_eq!(p.identity_file(), Path::new("/tmp/lp/state/identity.key"));
        assert_eq!(
            p.password_file(),
            Path::new("/tmp/lp/state/pairing-password")
        );
        assert_eq!(p.devices_file(), Path::new("/tmp/lp/state/devices.json"));
    }

    #[test]
    fn default_paths_end_with_lanpilot() {
        let p = Paths::default_for_user().unwrap();
        assert!(p.config_file.ends_with("config.toml"));
        let parent = p.config_file.parent().unwrap();
        assert!(parent.ends_with("LanPilot") || parent.ends_with("lanpilot"));
        let expected = if cfg!(windows) {
            "LanPilot"
        } else {
            "lanpilot"
        };
        assert!(p.state_dir.ends_with(expected), "{:?}", p.state_dir);
    }
}
