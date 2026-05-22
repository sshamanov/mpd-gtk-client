//! Centralized user-facing strings — constants and formatting functions.
//!
//! All text displayed to the user lives here. Inline string literals in widget
//! construction, toast messages, notifications, and help dialogs import from
//! this module. Architecture: architecture.md §1325-1328 (i18n ADR).

// ── Window / title ──

pub const WINDOW_TITLE: &str = "MPD Client";

// ── Mode labels / tooltips ──

pub const MODE_ALBUM_TOOLTIP: &str = "Album Mode (Ctrl+1)";
pub const MODE_FOLDER_TOOLTIP: &str = "Folder Mode (Ctrl+2)";
pub const MODE_ALBUM: &str = "Album Mode";
pub const MODE_FOLDER: &str = "Folder Mode";

// ── Search ──

pub const SEARCH_ALBUMS_PLACEHOLDER: &str = "Search albums...";
pub const SEARCH_FILES_PLACEHOLDER: &str = "Search files...";

// ── Connection status ──

pub const CONNECTING_TO_MPD: &str = "Connecting to MPD...";

// ── Empty states ──

pub const NO_ALBUMS_FOUND: &str = "No albums found";
pub const NO_RESULTS_FOUND: &str = "No results found";
pub const INDEXING: &str = "Indexing\u{2026}"; // …
pub const NO_TRACK_PLAYING: &str = "No track playing";
pub const EMPTY_DIRECTORY: &str = "(empty directory)";
pub const UNKNOWN_ARTIST: &str = "Unknown Artist";

// ── Context menu labels ──

pub const PLAY_NOW: &str = "Play Now";
pub const PLAY_NEXT: &str = "Play Next";
pub const ADD_TO_QUEUE: &str = "Add to Queue";
pub const REMOVE: &str = "Remove";

// ── Button labels / tooltips ──

pub const BTN_ADD_LABEL: &str = "+";
pub const BTN_NEXT_LABEL: &str = "\u{21a9}"; // ↩
pub const BTN_PLAY_LABEL: &str = "\u{25b6}"; // ▶
pub const TOOLTIP_ADD_TO_QUEUE: &str = "Add to queue";
pub const TOOLTIP_PLAY_NEXT: &str = "Play next";
pub const TOOLTIP_CLEAR_AND_PLAY: &str = "Clear queue and play";
pub const TOOLTIP_QUEUE: &str = "Queue";
pub const TOOLTIP_PREV_ALBUM: &str = "Previous Album";
pub const TOOLTIP_PREV_TRACK: &str = "Previous Track";
pub const TOOLTIP_PLAY_PAUSE: &str = "Play/Pause";
pub const TOOLTIP_NEXT_TRACK: &str = "Next Track";
pub const TOOLTIP_NEXT_ALBUM: &str = "Next Album";
pub const TOOLTIP_SETTINGS: &str = "Settings (Ctrl+,)";
pub const TOOLTIP_RESCAN: &str = "Rescan MPD music library";

// ── Settings dialog ──

pub const SETTINGS_TITLE: &str = "Settings";
pub const SETTINGS_MPD_HOST: &str = "MPD Host:";
pub const SETTINGS_MPD_PORT: &str = "MPD Port:";
pub const SETTINGS_INVALID_PORT: &str = "Invalid port number";
pub const SETTINGS_INVALID_PORT_RANGE: &str = "Invalid port (1-65535)";
pub const SETTINGS_PROFILE: &str = "Profile:";
pub const SETTINGS_HIGH_CONTRAST: &str = "High Contrast Mode";
pub const SETTINGS_AUTO_START: &str = "Auto-start on login";
pub const SETTINGS_GROUP_ARTIST: &str = "Group Artists by Album Artist";
pub const SETTINGS_GROUP_ARTIST_TOOLTIP: &str =
    "When enabled, Artists view groups by AlbumArtist tag. When disabled, uses Artist tag.";
pub const SETTINGS_COVER_CAP: &str = "Online Cover Data Cap (MB, 0 = unlimited):";
pub const SETTINGS_MEMORY_LABEL: &str = "Memory Warning Threshold (MB):";
pub const SETTINGS_LAYOUT_LABEL: &str = "Layout Profile:";
pub const SETTINGS_EXPORT_BTN: &str = "Export Layout\u{2026}"; // …
pub const SETTINGS_IMPORT_BTN: &str = "Import Layout\u{2026}"; // …
pub const SETTINGS_EXPORT_TITLE: &str = "Export Layout Profile";
pub const SETTINGS_IMPORT_TITLE: &str = "Import Layout Profile";
pub const SETTINGS_SAVE: &str = "Save";
pub const SETTINGS_OPEN: &str = "Open";
pub const SETTINGS_CANCEL: &str = "Cancel";
pub const SETTINGS_JSON_FILTER: &str = "JSON Files";

// ── Keyboard shortcuts dialog ──

pub const SHORTCUTS_TITLE: &str = "Keyboard Shortcuts";
pub const CLOSE: &str = "Close";

// ── Toast messages ──

pub const TOAST_CONNECTION_LOST: &str = "MPD connection lost \u{2014} retrying..."; // —
pub const TOAST_CONNECTION_FAILED_PREFIX: &str = "MPD connection failed: ";
pub const TOAST_CONNECTION_FAILED_SUFFIX: &str = "\n\nCheck your MPD server and settings.";
pub const TOAST_BATCH_ERROR: &str = "Batch error: unsupported sub-command";
pub const TOAST_METADATA_FAILED: &str = "Metadata connection failed: ";
pub const TOAST_MPD_ERROR_PREFIX: &str = "MPD Error: ";

// ── Notification titles / bodies ──

pub const NOTIF_ERROR: &str = "MPD Error";
pub const NOTIF_WARNING: &str = "MPD Warning";
pub const NOTIF_INFO: &str = "mpd-client";
pub const NOTIF_DISCONNECTED: &str = "MPD Disconnected";
pub const NOTIF_DISCONNECTED_BODY: &str = "Connection lost \u{2014} retrying..."; // —
pub const NOTIF_RECONNECTED: &str = "MPD Reconnected";
pub const NOTIF_RECONNECTED_BODY: &str = "Connection restored.";

// ── Badge text ──

pub const CUE_SHEET_ALBUM: &str = "Cue Sheet Album";
pub const CUE_BADGE: &str = "CUE";
pub const DSD_ALBUM: &str = "DSD Album";
pub const DSD_BADGE: &str = "DSD";

// ── Group view names ──

pub const GROUP_ALBUMS: &str = "Albums";
pub const GROUP_ARTISTS: &str = "Artists";
pub const GROUP_YEARS: &str = "Years";
pub const GROUP_GENRES: &str = "Genres";
pub const TAG_ALBUM_ARTIST: &str = "AlbumArtist";
pub const TAG_ARTIST: &str = "Artist";
pub const TAG_DATE: &str = "Date";
pub const TAG_GENRE: &str = "Genre";

// ── Formatting functions ──

/// "Showing N of M track results" cap note.
pub fn cap_note(showing: usize, total: usize) -> String {
    format!("Showing {showing} of {total} track results")
}

/// Memory warning toast body.
pub fn memory_warning(rss_mb: u64) -> String {
    format!("Memory usage high ({rss_mb} MB) \u{2014} consider closing other applications")
}

/// "MPD Error: {msg}" for toast overlay.
pub fn mpd_error(msg: &str) -> String {
    format!("MPD Error: {msg}")
}

/// Removed-tracks reconciliation toast.
pub fn removed_tracks_toast(removed: usize) -> String {
    format!("Removed {removed} missing or modified track(s) from queue")
}

/// Month-to-date cover data usage label.
pub fn month_usage_mb(usage_mb: f64) -> String {
    format!("Month-to-date usage: {usage_mb:.1} MB")
}

/// DSD summary badge (e.g. "DSD64", "DSD128").
pub fn dsdbadge(bit_depth: u32) -> String {
    format!("DSD{}", bit_depth * 64)
}

// ── Keybinding action descriptions ──

use crate::keybindings::Action;

pub fn action_description(action: &Action) -> &'static str {
    match action {
        Action::PlayPause => "Play / Pause",
        Action::NextTrack => "Next track",
        Action::PreviousTrack => "Previous track",
        Action::Stop => "Stop",
        Action::ToggleMode => "Toggle Album / Folder mode",
        Action::FocusSearch => "Focus search bar",
        Action::OpenSettings => "Open settings",
        Action::OpenShortcuts => "Show keyboard shortcuts",
        Action::Quit => "Quit",
        Action::QueueMoveUp => "Move item up in queue",
        Action::QueueMoveDown => "Move item down in queue",
        Action::QueueRemoveSelected => "Remove from queue",
        Action::ActivateSelection => "Activate selected item",
        Action::DeleteSelected => "Delete selected",
        Action::Escape => "Cancel / Deselect",
        Action::FolderCollapse => "Collapse folder",
        Action::AlbumPlay => "Play album",
    }
}

/// Help dialog shortcut table.
pub fn shortcut_list() -> &'static [(&'static str, &'static str)] {
    &[
        ("Ctrl+1", "Album Mode"),
        ("Ctrl+2", "Folder Mode"),
        ("Ctrl+F", "Search albums"),
        ("Ctrl+,", "Settings"),
        ("Ctrl+?", "Keyboard Shortcuts"),
        ("Ctrl+Q", "Quit"),
        ("", ""),
        ("Double-click album", "Play album"),
        ("Double-click queue item", "Play track at position"),
        ("Right-click queue item", "Context menu (Play Now / Remove)"),
        ("Delete", "Remove selected queue item"),
        ("Shift+Up", "Move queue item up"),
        ("Shift+Down", "Move queue item down"),
    ]
}

/// Desktop entry .desktop file content for application menu integration.
pub fn desktop_entry(exe_path: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=mpd-client\n\
         Exec={exe_path}\n\
         Icon=mpd-client\n\
         Terminal=false\n\
         Categories=Audio;Music;Player;\n\
         MimeType=audio/flac;audio/mpeg;audio/ogg;audio/wav;audio/x-flac;\n"
    )
}

/// CLI usage text.
pub fn usage_text() -> &'static str {
    "\
Usage: mpd-client [OPTIONS]

Session overrides (not persisted):
  --mpd-host <HOST>        MPD server hostname (default: 127.0.0.1)
  --mpd-port <PORT>        MPD server port (default: 6600)
  --profile <NAME>         Connection profile name (reserved, not yet implemented)
  --mode <album|folder>    Startup UI mode (default: album)

Media actions (dispatched after connection):
  --start-playing          Start playback
  --toggle-playback        Toggle play/pause
  --next                   Skip to next track
  --prev                   Skip to previous track

Info:
  --help, -h               Show this help and exit
  --version, -V            Print version and exit\
"
}

// ── CUE / DSD tooltip helpers ──

pub fn cue_tooltip(track_count: usize) -> String {
    format!(
        "Cue sheet \u{2014} {} track{}",
        track_count,
        if track_count == 1 { "" } else { "s" }
    )
}

pub fn dsd_tooltip(file_count: usize) -> String {
    format!(
        "DSD folder \u{2014} {} file{}",
        file_count,
        if file_count == 1 { "" } else { "s" }
    )
}

// ── Settings feedback messages ──

pub const EXPORT_ERROR_PREFIX: &str = "Export error: ";
pub const EXPORT_FAILED_PREFIX: &str = "Export failed: ";
pub const EXPORT_SUCCESS: &str = "Layout exported successfully";
pub const IMPORT_SUCCESS: &str = "Layout profile imported successfully";
pub const IMPORT_FAILED_PREFIX: &str = "Import failed: ";
pub const IMPORT_READ_ERROR_PREFIX: &str = "Cannot read file: ";
