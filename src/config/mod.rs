//! Configuration — TOML settings load/save. Thread: UI.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub mpd_host: String,
    #[serde(default = "default_port")]
    pub mpd_port: u16,
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f64,
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 6600 }
fn default_split_ratio() -> f64 { 0.7 }

impl Default for Config {
    fn default() -> Self {
        Self {
            mpd_host: default_host(),
            mpd_port: default_port(),
            split_ratio: default_split_ratio(),
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
