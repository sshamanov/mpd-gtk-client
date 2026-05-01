//! Configuration — TOML settings load/save. Thread: UI.
//! Also defines [`CliOverrides`] for session-only CLI flag overrides.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session-only overrides parsed from CLI flags — not persisted to config file.
#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    /// Override for `mpd_host` (--mpd-host).
    pub mpd_host: Option<String>,
    /// Override for `mpd_port` (--mpd-port).
    pub mpd_port: Option<u16>,
    /// Profile name for config section selection (--profile).
    pub profile: Option<String>,
    /// Startup mode override (--mode album|folder).
    pub mode: Option<String>,
}

impl CliOverrides {
    /// Apply session-only overrides to a mutable Config reference.
    /// Profile and mode are not config fields and are handled separately.
    pub fn apply_to_config(&self, cfg: &mut Config) {
        if let Some(ref host) = self.mpd_host {
            cfg.mpd_host = host.clone();
        }
        if let Some(port) = self.mpd_port {
            cfg.mpd_port = port;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub mpd_host: String,
    #[serde(default = "default_port")]
    pub mpd_port: u16,
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f64,
    #[serde(default)]
    pub mpris: MprisConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MprisConfig {
    #[serde(default = "default_mpris_enabled")]
    pub enabled: bool,
}

fn default_mpris_enabled() -> bool { false }

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 6600 }
fn default_split_ratio() -> f64 { 0.7 }

impl Default for Config {
    fn default() -> Self {
        Self {
            mpd_host: default_host(),
            mpd_port: default_port(),
            split_ratio: default_split_ratio(),
            mpris: MprisConfig { enabled: false },
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mpd-client")
            .join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Load config for a specific named profile.
    /// Currently a stub — falls back to `load()` until multi-profile support lands (epic 15).
    pub fn with_profile(name: &str) -> Self {
        log::info!("Profile '{name}' requested but multi-profile is not yet implemented; using defaults");
        Self::load()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| {
            std::io::Error::other(e.to_string())
        })?;
        std::fs::write(&path, content)
    }
}
