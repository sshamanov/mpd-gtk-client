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

/// Current schema version for config migration.
/// Version 0 means "unversioned" (pre-migration). Version 1 is the first versioned schema.
/// Version 2: `notifications.libnotify` (bool) → `notifications.mode` (string enum).
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Window geometry persisted for session restoral.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub mpd_host: String,
    #[serde(default = "default_port")]
    pub mpd_port: u16,
    #[serde(default)]
    pub mpris: MprisConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    /// Cover art cache settings.
    #[serde(default)]
    pub cover_cache: CoverCacheConfig,
    /// Memory monitoring settings.
    #[serde(default)]
    pub memory: MemoryConfig,
    /// Layout profile for export/import (not used at runtime, persisted for portability).
    #[serde(default)]
    pub layout_profile: Option<LayoutProfile>,
    /// Named connection profiles (epic 15). Key = profile name.
    #[serde(default)]
    pub profiles: Option<HashMap<String, ProfileConfig>>,
    /// Profile to use on startup. Falls back to legacy `mpd_host`/`mpd_port` when None.
    pub default_profile: Option<String>,
    /// Last manually selected profile (persisted for next startup).
    pub last_profile: Option<String>,
    /// Schema version for migration. Defaults to 0 if absent (unversioned).
    #[serde(default)]
    pub schema_version: u32,
    /// Window geometry for session restoral (saved on graceful shutdown).
    #[serde(default)]
    pub window_geometry: Option<WindowGeometry>,
    /// High contrast mode for accessibility.
    #[serde(default = "default_false")]
    pub high_contrast: bool,
    /// Auto-start on desktop login via XDG autostart.
    #[serde(default = "default_false")]
    pub auto_start: bool,
    /// Group by AlbumArtist (true) or Artist (false) in Artists view.
    #[serde(default = "default_true")]
    pub use_album_artist: bool,
    /// Maximum search results to display in Album Mode (default: 100).
    #[serde(default = "default_album_cap")]
    pub search_track_cap_album: u32,
    /// Maximum search results to display in Folder Mode (default: 500).
    #[serde(default = "default_folder_cap")]
    pub search_track_cap_folder: u32,
}

fn default_false() -> bool { false }
fn default_true() -> bool { true }
fn default_album_cap() -> u32 { 100 }
fn default_folder_cap() -> u32 { 500 }

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

/// Desktop notification routing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationMode {
    /// In-app toast overlay only — no desktop notifications.
    Toast,
    /// Desktop notification via `org.freedesktop.Notifications` — no in-app overlay.
    Desktop,
    /// Both in-app toast overlay AND desktop notification.
    Both,
}

impl Default for NotificationMode {
    fn default() -> Self {
        Self::Toast
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    /// Notification routing mode. `"toast"` (default), `"desktop"`, or `"both"`.
    #[serde(default)]
    pub mode: NotificationMode,
}

/// Cover art cache settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverCacheConfig {
    /// Maximum cache size in megabytes before LRU eviction kicks in (default: 1000).
    #[serde(default = "default_cache_max_size_mb")]
    pub max_size_mb: u64,
    /// Monthly data cap for online cover lookups in megabytes (0 = unlimited, default: 500).
    #[serde(default = "default_monthly_data_cap_mb")]
    pub monthly_data_cap_mb: u64,
    /// Total bytes downloaded this month from online cover lookups.
    #[serde(default)]
    pub monthly_bytes_downloaded: u64,
    /// Month number (1-12) when the byte counter was last reset.
    #[serde(default = "current_month")]
    pub last_reset_month: u32,
}

fn default_cache_max_size_mb() -> u64 { 1000 }
fn default_monthly_data_cap_mb() -> u64 { 500 }

pub(crate) fn current_month() -> u32 {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    civil_month_from_days((now.as_secs() / 86400) as i64)
}

/// Convert days since Unix epoch to month number (1-12).
/// Uses Howard Hinnant's civil_from_days algorithm (public domain).
fn civil_month_from_days(days: i64) -> u32 {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = mp as i32 + if mp < 10 { 3 } else { -9 };
    m as u32
}

impl Default for CoverCacheConfig {
    fn default() -> Self {
        Self {
            max_size_mb: 1000,
            monthly_data_cap_mb: 500,
            monthly_bytes_downloaded: 0,
            last_reset_month: current_month(),
        }
    }
}

/// Memory monitoring settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// RSS memory warning threshold in megabytes (default: 500).
    #[serde(default = "default_warning_threshold_mb")]
    pub warning_threshold_mb: u64,
}

fn default_warning_threshold_mb() -> u64 { 500 }

impl Default for MemoryConfig {
    fn default() -> Self {
        Self { warning_threshold_mb: 500 }
    }
}

/// Layout profile for export/import — split ratio, rail widths, mode proportions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutProfile {
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// Layout settings bundled for portability.
    pub layout: LayoutSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutSettings {
    /// Split ratio between left pane and right rail (0.0–1.0, default 0.7).
    pub split_ratio: f64,
    /// Minimum right rail width in pixels (default 320).
    pub rail_width_min: u32,
    /// Maximum right rail width in pixels (default 420).
    pub rail_width_max: u32,
    /// Album mode proportions: [now_playing, current_album, queue].
    pub album_mode_proportions: [f64; 3],
    /// Folder mode proportions: [now_playing, queue].
    pub folder_mode_proportions: [f64; 2],
}

impl Default for LayoutProfile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            layout: LayoutSettings::default(),
        }
    }
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            split_ratio: 0.7,
            rail_width_min: 320,
            rail_width_max: 420,
            album_mode_proportions: [0.4, 0.2, 0.4],
            folder_mode_proportions: [0.55, 0.45],
        }
    }
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 6600 }

impl Default for Config {
    fn default() -> Self {
        Self {
            mpd_host: default_host(),
            mpd_port: default_port(),
            mpris: MprisConfig { enabled: false },
            notifications: NotificationsConfig::default(),
            cover_cache: CoverCacheConfig::default(),
            memory: MemoryConfig::default(),
            layout_profile: None,
            profiles: None,
            default_profile: None,
            last_profile: None,
            schema_version: CURRENT_SCHEMA_VERSION,
            window_geometry: None,
            high_contrast: false,
            auto_start: false,
            use_album_artist: true,
            search_track_cap_album: default_album_cap(),
            search_track_cap_folder: default_folder_cap(),
        }
    }
}

/// Migration functions indexed by source version.
/// `migrations[0]` transitions version 0 → 1, `migrations[1]` transitions 1 → 2, etc.
fn migrations() -> Vec<fn(&mut Config)> {
    vec![
        migrate_0_to_1,
        migrate_1_to_2,
    ]
}

/// Migration 0 → 1: First versioned schema.
/// Current config structure is identical to what was being written before schema_version was added.
fn migrate_0_to_1(_cfg: &mut Config) {
    // No structural changes needed — the Config struct already matches v1.
    // Future migrations will modify fields here.
}

/// Migration 1 → 2: `notifications.libnotify` (bool) → `notifications.mode` (NotificationMode).
fn migrate_1_to_2(cfg: &mut Config) {
    // The old `libnotify` key is deserialized into `NotificationsConfig` via serde.
    // If the file has `libnotify = true`, serde would set `mode = Toast` (default) since
    // `mode` is absent from the TOML. We need to detect this case.
    // However, after deserialization, the bool field is gone. We detect the old format
    // by checking if the serialized form contains the `libnotify` key directly.
    // Simplest approach: read the raw TOML and check for `libnotify`.
    if let Ok(content) = std::fs::read_to_string(Config::config_path()) {
        if let Ok(raw) = content.parse::<toml::Value>() {
            if raw.get("notifications")
                .and_then(|n| n.get("libnotify"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                cfg.notifications.mode = NotificationMode::Desktop;
            }
        }
    }
}

/// Run all pending migrations from the config's current version to CURRENT_SCHEMA_VERSION.
fn run_migrations(cfg: &mut Config) {
    let all = migrations();
    for version in cfg.schema_version..CURRENT_SCHEMA_VERSION {
        if let Some(migrate) = all.get(version as usize) {
            log::info!("Migrating config from version {} to {}", version, version + 1);
            migrate(cfg);
            cfg.schema_version = version + 1;
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

    /// Load config with corruption recovery and schema migration.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                match toml::from_str::<Config>(&content) {
                    Ok(mut cfg) => {
                        if cfg.schema_version < CURRENT_SCHEMA_VERSION {
                            run_migrations(&mut cfg);
                            let _ = cfg.save();
                        }
                        cfg
                    }
                    Err(e) => {
                        // Corrupt config — back up and start fresh
                        let backup_path = path.with_extension("toml.bad");
                        if std::fs::rename(&path, &backup_path).is_ok() {
                            log::warn!(
                                "Config file corrupt ({}), backed up to {}. Using defaults.",
                                e, backup_path.display()
                            );
                        } else {
                            log::warn!(
                                "Config file corrupt ({}), could not back up. Using defaults.",
                                e
                            );
                        }
                        Self::default()
                    }
                }
            }
            Err(e) => {
                log::warn!("Config file unreadable ({}), using defaults.", e);
                Self::default()
            }
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

    /// Export layout profile as JSON string.
    pub fn export_layout_profile(&self) -> Result<String, String> {
        let profile = self.layout_profile.clone().unwrap_or_default();
        serde_json::to_string_pretty(&profile).map_err(|e| format!("JSON serialization failed: {e}"))
    }

    /// Import a layout profile from JSON, storing it in config and persisting to disk.
    /// Validates schema version — only version 1 is accepted.
    pub fn import_layout_profile(&mut self, json: &str) -> Result<(), String> {
        let profile: LayoutProfile = serde_json::from_str(json)
            .map_err(|e| format!("Invalid JSON: {e}"))?;
        if profile.schema_version != 1 {
            return Err(format!(
                "Unsupported schema version {} (expected 1)",
                profile.schema_version
            ));
        }
        self.layout_profile = Some(profile);
        self.save().map_err(|e| format!("Failed to save config: {e}"))
    }

    /// Install XDG autostart .desktop file.
    pub fn install_autostart() -> std::io::Result<()> {
        let exe = std::env::current_exe()
            .map_err(|e| std::io::Error::other(format!("Cannot get exe path: {e}")))?;
        let autostart_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("autostart");
        std::fs::create_dir_all(&autostart_dir)?;
        let path = autostart_dir.join("mpd-client.desktop");
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=mpd-client\n\
             Exec={}\n\
             X-GNOME-Autostart-enabled=true\n\
             X-GNOME-AutostartDelay=0\n",
            exe.display()
        );
        std::fs::write(&path, content)
    }

    /// Remove XDG autostart .desktop file.
    pub fn remove_autostart() {
        let autostart_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("autostart");
        let path = autostart_dir.join("mpd-client.desktop");
        let _ = std::fs::remove_file(path);
    }

    /// Install XDG desktop entry file for application menu integration.
    /// Installs at `~/.local/share/applications/mpd-client.desktop`.
    pub fn install_desktop_file() -> std::io::Result<()> {
        let exe = std::env::current_exe()
            .map_err(|e| std::io::Error::other(format!("Cannot get exe path: {e}")))?;
        let apps_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("applications");
        std::fs::create_dir_all(&apps_dir)?;
        let path = apps_dir.join("mpd-client.desktop");
        let content = crate::strings::desktop_entry(&exe.display().to_string());
        std::fs::write(&path, content)
    }
}
