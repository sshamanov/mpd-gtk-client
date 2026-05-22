//! Application state — shared playback/queue state and mode-local browsing state. Thread: UI (single-threaded mutations).

use crate::mpd::state_machine::MpdEvent;
use crate::mpd::state_machine::ToastLevel;
#[cfg(test)]
use crate::mpd::state_machine::PlaybackUpdate;
use crate::mpd::{Album, QueueItem, Track};
use std::collections::{HashMap, VecDeque};
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
pub struct AppState {
    pub playback: PlaybackStatus,
    pub current: CurrentContext,
    pub queue: QueueState,
    pub connection: ConnectionState,
    pub album_browsing: AlbumBrowsingState,
    pub folder_browsing: FolderBrowsingState,
    pub mode: Mode,
    #[serde(skip)]
    pub toast_queue: VecDeque<(String, ToastLevel)>,
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
        mode: Mode::Album,
        toast_queue: VecDeque::new(),
    }))
}

#[derive(Clone)]
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

    pub fn switch_mode(&self, mode: Mode) {
        if let Err(e) = self.state.write().map(|mut s| s.mode = mode) {
            log::error!("RwLock poisoned in switch_mode: {e}");
        }
    }

    pub fn save_album_scroll_position(&self, value: f64) {
        if let Err(e) = self.state.write().map(|mut s| s.album_browsing.scroll_position.1 = value) {
            log::error!("RwLock poisoned in save_album_scroll_position: {e}");
        }
    }

    pub fn save_folder_browsing_state(&self, expanded_paths: Vec<std::path::PathBuf>, scroll_position: f64) {
        if let Err(e) = self.state.write().map(|mut s| {
            s.folder_browsing.expanded_paths = expanded_paths;
            s.folder_browsing.scroll_position = scroll_position;
        }) {
            log::error!("RwLock poisoned in save_folder_browsing_state: {e}");
        }
    }

    pub fn set_custom_album_order(&self, order: Vec<String>) {
        if let Err(e) = self.state.write().map(|mut s| s.album_browsing.custom_album_order = order) {
            log::error!("RwLock poisoned in set_custom_album_order: {e}");
        }
    }

    pub fn get_state(&self) -> SharedState {
        self.state.clone()
    }
}

/// Apply an MPD event to shared state — acquires write lock and calls reduce.
///
/// This is the only entry point for event-driven state mutations outside of Store methods.
pub fn apply_event(state: &SharedState, event: &MpdEvent) {
    match state.write() {
        Ok(mut s) => reduce(&mut s, event),
        Err(e) => log::error!("RwLock poisoned in apply_event: {e}"),
    }
}

/// Single reducer — applies an MPD event to application state.
///
/// Pure data transformation: no GTK operations, no channel sends, no I/O.
/// Runs on the GTK thread. Must complete in <1ms.
pub fn reduce(state: &mut AppState, event: &MpdEvent) {
    match event {
        MpdEvent::Connected => {
            state.connection = ConnectionState::Connected;
        }
        MpdEvent::Connecting => {
            state.connection = ConnectionState::Connecting;
        }
        MpdEvent::Disconnected => {
            state.connection = ConnectionState::Disconnected;
        }
        MpdEvent::StateChanged(update) => {
            if update.state == "stop" {
                state.current.track = None;
                state.current.album = None;
            } else {
                let dur = update.duration
                    .map(std::time::Duration::from_secs_f64);
                state.current.track = Some(Track {
                    id: update.song.map(|s| s.to_string()).unwrap_or_default(),
                    title: update.title.clone().unwrap_or_default(),
                    album_id: update.album.clone().unwrap_or_default(),
                    path: PathBuf::new(),
                    duration: dur,
                    format: update.audio_format.as_ref().map(|f| f.display_text()),
                });
                state.current.album = Some(Album {
                    id: update.album.clone().unwrap_or_default(),
                    title: update.album.clone().unwrap_or_default(),
                    artist: update.artist.clone().unwrap_or_default(),
                    year: None,
                    genre: None,
                    cover_path: None,
                    tracks: vec![],
                });
            }
        }
        // Events with no state changes — widget-only (processed in event_loop.rs)
        MpdEvent::Queue(_)
        | MpdEvent::Albums(_)
        | MpdEvent::AlbumsGrouped(_)
        | MpdEvent::SearchResults { .. }
        | MpdEvent::SearchIndexing
        | MpdEvent::FileSearchResults(..)
        | MpdEvent::DirectoryListing(..)
        | MpdEvent::AlbumTracks(_)
        | MpdEvent::CoverPaths(_)
        | MpdEvent::CoverRefreshed { .. }
        | MpdEvent::LibraryChanged => {}
        // Error → toast for user-visible feedback
        MpdEvent::Error(msg) => {
            state.toast_queue.push_back((msg.clone(), ToastLevel::Error));
        }
        // Toast → enqueue for display (consumed by event_loop widget update)
        MpdEvent::Toast { message, level } => {
            state.toast_queue.push_back((message.clone(), *level));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        AppState {
            playback: PlaybackStatus { state: PlaybackState::Stopped, current_position: 0, volume: 100 },
            current: CurrentContext { track: None, album: None },
            queue: QueueState { items: vec![], current_position: None },
            connection: ConnectionState::Disconnected,
            album_browsing: AlbumBrowsingState {
                scroll_position: (0.0, 0.0),
                selected_album_id: None,
                hover_album_id: None,
                group_expansion: HashMap::new(),
                custom_album_order: Vec::new(),
            },
            folder_browsing: FolderBrowsingState {
                expanded_paths: vec![],
                selected_track_id: None,
                scroll_position: 0.0,
            },
            mode: Mode::Album,
            toast_queue: VecDeque::new(),
        }
    }

    #[test]
    fn test_reduce_connected() {
        let mut state = test_state();
        assert!(matches!(state.connection, ConnectionState::Disconnected));
        reduce(&mut state, &MpdEvent::Connected);
        assert!(matches!(state.connection, ConnectionState::Connected));
    }

    #[test]
    fn test_reduce_disconnected() {
        let mut state = test_state();
        state.connection = ConnectionState::Connected;
        reduce(&mut state, &MpdEvent::Disconnected);
        assert!(matches!(state.connection, ConnectionState::Disconnected));
    }

    #[test]
    fn test_reduce_connecting() {
        let mut state = test_state();
        assert!(matches!(state.connection, ConnectionState::Disconnected));
        reduce(&mut state, &MpdEvent::Connecting);
        assert!(matches!(state.connection, ConnectionState::Connecting));
    }

    #[test]
    fn test_reduce_state_changed_playing() {
        let mut state = test_state();
        let update = PlaybackUpdate {
            state: "play".into(),
            song: Some(42),
            title: Some("Test Song".into()),
            artist: Some("Test Artist".into()),
            album: Some("Test Album".into()),
            elapsed: Some(10.0),
            duration: Some(200.0),
            volume: 80,
            ..Default::default()
        };
        reduce(&mut state, &MpdEvent::StateChanged(update));
        assert!(state.current.track.is_some());
        assert_eq!(state.current.track.as_ref().unwrap().title, "Test Song");
        assert!(state.current.album.is_some());
        assert_eq!(state.current.album.as_ref().unwrap().title, "Test Album");
    }

    #[test]
    fn test_reduce_state_changed_stop() {
        let mut state = test_state();
        state.current.track = Some(Track {
            id: "1".into(), title: "X".into(), album_id: "A".into(),
            path: PathBuf::new(), duration: None, format: None,
        });
        let update = PlaybackUpdate { state: "stop".into(), ..Default::default() };
        reduce(&mut state, &MpdEvent::StateChanged(update));
        assert!(state.current.track.is_none());
        assert!(state.current.album.is_none());
    }
}
