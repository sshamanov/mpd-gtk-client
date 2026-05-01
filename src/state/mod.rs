//! Application state — shared playback/queue state and mode-local browsing state. Thread: UI (single-threaded mutations).

use crate::mpd::{Album, QueueItem, Track};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackStatus {
    pub state: PlaybackState,
    pub current_position: u64, // milliseconds
    pub volume: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentContext {
    pub track: Option<Track>,
    pub album: Option<Album>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueState {
    pub items: Vec<QueueItem>,
    pub current_position: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionState {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumBrowsingState {
    pub scroll_position: (f64, f64),
    pub selected_album_id: Option<String>,
    pub hover_album_id: Option<String>,
    pub group_expansion: HashMap<String, bool>,
    pub custom_album_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderBrowsingState {
    pub expanded_paths: Vec<PathBuf>,
    pub selected_track_id: Option<String>,
    pub scroll_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutState {
    pub shell_split: (f64, f64),
    pub right_rail_width: f64,
    pub album_mode_proportions: (f64, f64, f64),
    pub folder_mode_proportions: (f64, f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub playback: PlaybackStatus,
    pub current: CurrentContext,
    pub queue: QueueState,
    pub connection: ConnectionState,
    pub album_browsing: AlbumBrowsingState,
    pub folder_browsing: FolderBrowsingState,
    pub layout: LayoutState,
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    Album,
    Folder,
}

pub type SharedState = Arc<RwLock<AppState>>;

pub fn create_initial_state() -> SharedState {
    Arc::new(RwLock::new(AppState {
        playback: PlaybackStatus {
            state: PlaybackState::Stopped,
            current_position: 0,
            volume: 100,
        },
        current: CurrentContext {
            track: None,
            album: None,
        },
        queue: QueueState {
            items: Vec::new(),
            current_position: None,
        },
        connection: ConnectionState::Disconnected,
        album_browsing: AlbumBrowsingState {
            scroll_position: (0.0, 0.0),
            selected_album_id: None,
            hover_album_id: None,
            group_expansion: HashMap::new(),
            custom_album_order: Vec::new(),
        },
        folder_browsing: FolderBrowsingState {
            expanded_paths: Vec::new(),
            selected_track_id: None,
            scroll_position: 0.0,
        },
        layout: LayoutState {
            shell_split: (crate::constants::SHELL_SPLIT_RATIO, 1.0 - crate::constants::SHELL_SPLIT_RATIO),
            right_rail_width: crate::constants::RAIL_WIDTH_MIN,
            album_mode_proportions: (0.4, 0.2, 0.4),
            folder_mode_proportions: (0.55, 0.45),
        },
        mode: Mode::Album,
    }))
}

pub struct Store {
    state: SharedState,
}

impl Store {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    pub fn update_playback_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut PlaybackStatus),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.playback)) {
            log::error!("RwLock poisoned in update_playback_state: {e}");
        }
    }

    pub fn update_queue<F>(&self, updater: F)
    where
        F: FnOnce(&mut QueueState),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.queue)) {
            log::error!("RwLock poisoned in update_queue: {e}");
        }
    }

    pub fn update_connection_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut ConnectionState),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.connection)) {
            log::error!("RwLock poisoned in update_connection_state: {e}");
        }
    }

    pub fn update_current_context<F>(&self, updater: F)
    where
        F: FnOnce(&mut CurrentContext),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.current)) {
            log::error!("RwLock poisoned in update_current_context: {e}");
        }
    }

    pub fn update_album_browsing<F>(&self, updater: F)
    where
        F: FnOnce(&mut AlbumBrowsingState),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.album_browsing)) {
            log::error!("RwLock poisoned in update_album_browsing: {e}");
        }
    }

    pub fn update_folder_browsing<F>(&self, updater: F)
    where
        F: FnOnce(&mut FolderBrowsingState),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.folder_browsing)) {
            log::error!("RwLock poisoned in update_folder_browsing: {e}");
        }
    }

    pub fn update_layout<F>(&self, updater: F)
    where
        F: FnOnce(&mut LayoutState),
    {
        if let Err(e) = self.state.write().map(|mut s| updater(&mut s.layout)) {
            log::error!("RwLock poisoned in update_layout: {e}");
        }
    }

    pub fn switch_mode(&self, mode: Mode) {
        if let Err(e) = self.state.write().map(|mut s| s.mode = mode) {
            log::error!("RwLock poisoned in switch_mode: {e}");
        }
    }

    pub fn get_state(&self) -> SharedState {
        self.state.clone()
    }
}
