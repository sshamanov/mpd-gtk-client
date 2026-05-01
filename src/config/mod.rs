//! Configuration — TOML settings load/save. Thread: UI.
//! Also defines [`CliOverrides`] for session-only CLI flag overrides.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    #[serde(default)]
    pub notifications: NotificationsConfig,
    /// Named connection profiles (epic 15). Key = profile name.
    #[serde(default)]
    pub profiles: Option<HashMap<String, ProfileConfig>>,
    /// Profile to use on startup. Falls back to legacy `mpd_host`/`mpd_port` when None.
    pub default_profile: Option<String>,
    /// Last manually selected profile (persisted for next startup).
    pub last_profile: Option<String>,
}

/// A named MPD connection profile — either a Unix socket path or TCP host:port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Unix socket path (e.g., `/run/mpd/socket`) or TCP hostname.
    pub host: String,
    /// TCP port (only used when `host` is not a Unix socket path).
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MprisConfig {
    #[serde(default = "default_mpris_enabled")]
    pub enabled: bool,
}

fn default_mpris_enabled() -> bool { false }

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default = "default_notif_enabled")]
    pub libnotify: bool,
}

fn default_notif_enabled() -> bool { false }

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
            notifications: NotificationsConfig { libnotify: false },
            profiles: None,
            default_profile: None,
            last_profile: None,
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

    /// Load config, optionally selecting a profile's connection parameters.
    ///
    /// If `name` is provided and exists in `profiles`, the returned Config uses
    /// the profile's `host`/`port` as `mpd_host`/`mpd_port` for backward compat.
    /// Falls back to `load()` if the profile doesn't exist.
    pub fn with_profile(name: &str) -> Self {
        let mut cfg = Self::load();
        if let Some(ref profiles) = cfg.profiles {
            if let Some(profile) = profiles.get(name) {
                log::info!("Using profile '{name}': {}", profile.host);
                cfg.mpd_host = profile.host.clone();
                cfg.mpd_port = profile.port;
                cfg.default_profile = Some(name.to_string());
            } else {
                log::warn!("Profile '{name}' not found, using default connection");
            }
        }
        cfg
    }

    /// Resolve the connection target for the configured (or default) profile.
    pub fn connection_target(&self) -> crate::mpd::ConnectionTarget {
        let profile_name = self.default_profile.as_deref().or(self.last_profile.as_deref());

        if let Some(name) = profile_name {
            if let Some(ref profiles) = self.profiles {
                if let Some(profile) = profiles.get(name) {
                    let host = profile.host.trim();
                    if host.starts_with('/') || host.starts_with('~') {
                        return crate::mpd::ConnectionTarget::Unix(host.to_string());
                    }
                    return crate::mpd::ConnectionTarget::Tcp(host.to_string(), profile.port);
                }
            }
        }

        // No profiles — use legacy behavior
        let host = self.mpd_host.trim();
        if host.is_empty() || host == "auto" {
            crate::mpd::ConnectionTarget::Auto
        } else if host.starts_with('/') || host.starts_with('~') {
            crate::mpd::ConnectionTarget::Unix(host.to_string())
        } else {
            crate::mpd::ConnectionTarget::Tcp(host.to_string(), self.mpd_port)
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
