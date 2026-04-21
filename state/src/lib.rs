use mpd_adapter::{Album, Track, QueueItem};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
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
    pub scroll_position: (f64, f64), // (x, y)
    pub selected_album_id: Option<String>,
    pub hover_album_id: Option<String>,
    pub group_expansion: HashMap<String, bool>, // group header -> expanded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderBrowsingState {
    pub expanded_paths: Vec<PathBuf>,
    pub selected_track_id: Option<String>,
    pub scroll_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutState {
    pub shell_split: (f64, f64), // left/right proportion (0.7, 0.3)
    pub right_rail_width: f64, // pixels
    pub album_mode_proportions: (f64, f64, f64), // now playing, current album, queue
    pub folder_mode_proportions: (f64, f64), // now playing, queue
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
        },
        folder_browsing: FolderBrowsingState {
            expanded_paths: Vec::new(),
            selected_track_id: None,
            scroll_position: 0.0,
        },
        layout: LayoutState {
            shell_split: (0.7, 0.3),
            right_rail_width: 320.0,
            album_mode_proportions: (0.4, 0.2, 0.4),
            folder_mode_proportions: (0.55, 0.45),
        },
        mode: Mode::Album,
    }))
}

pub struct EventBus {
    pub state_updates: broadcast::Sender<AppState>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { state_updates: tx }
    }
}

pub struct Store {
    state: SharedState,
    event_bus: EventBus,
}

impl Store {
    pub fn new(state: SharedState, event_bus: EventBus) -> Self {
        Self { state, event_bus }
    }

    pub async fn update_playback_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut PlaybackStatus),
    {
        let mut state = self.state.write().await;
        updater(&mut state.playback);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn update_queue<F>(&self, updater: F)
    where
        F: FnOnce(&mut QueueState),
    {
        let mut state = self.state.write().await;
        updater(&mut state.queue);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn update_connection_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut ConnectionState),
    {
        let mut state = self.state.write().await;
        updater(&mut state.connection);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn update_current_context<F>(&self, updater: F)
    where
        F: FnOnce(&mut CurrentContext),
    {
        let mut state = self.state.write().await;
        updater(&mut state.current);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn update_album_browsing<F>(&self, updater: F)
    where
        F: FnOnce(&mut AlbumBrowsingState),
    {
        let mut state = self.state.write().await;
        updater(&mut state.album_browsing);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn update_folder_browsing<F>(&self, updater: F)
    where
        F: FnOnce(&mut FolderBrowsingState),
    {
        let mut state = self.state.write().await;
        updater(&mut state.folder_browsing);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn update_layout<F>(&self, updater: F)
    where
        F: FnOnce(&mut LayoutState),
    {
        let mut state = self.state.write().await;
        updater(&mut state.layout);
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub async fn switch_mode(&self, mode: Mode) {
        let mut state = self.state.write().await;
        state.mode = mode;
        let _ = self.event_bus.state_updates.send(state.clone());
    }

    pub fn get_state(&self) -> SharedState {
        self.state.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppState> {
        self.event_bus.state_updates.subscribe()
    }
}