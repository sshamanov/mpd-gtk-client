//! MPD state machine — connection lifecycle, command dispatch, event emission. Thread: dedicated background thread.

#![allow(clippy::expect_used)]

use crate::mpd::MpdAdapter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Commands sent from the UI thread to the MPD background thread.
pub enum MpdCommand {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Seek(i64),
    Status,
    CurrentSong,
    ListAlbums,
    ListAlbumsGrouped(String),
    Search(String),
    ListDirectory(String),
    PlayFile(String),
    ListQueue,
    PlayPosition(i32),
    DeleteId(i32),
    MoveId(i32, i32),
    Add(String),
    InsertNext(String),
    PlayAlbum(String),
    Clear,
}

/// Events emitted by the MPD background thread to the UI thread.
#[derive(Debug, Clone)]
pub enum MpdEvent {
    Connected,
    Connecting,
    Disconnected,
    StateChanged(PlaybackUpdate),
    Albums(Vec<(String, String)>),
    AlbumsGrouped(crate::mpd::AlbumGroup),
    SearchResults(Vec<(String, String)>),
    DirectoryListing(String, Vec<crate::mpd::DirEntry>),
    Queue(Vec<crate::mpd::QueueEntry>),
    LibraryChanged,
    Error(String),
}

/// Playback state update from the MPD status response.
#[derive(Debug, Clone, Default)]
pub struct PlaybackUpdate {
    pub state: String,
    pub song: Option<u32>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub volume: i16,
    pub elapsed: Option<f64>,
    pub duration: Option<f64>,
    pub playlist_version: Option<String>,
}

/// Internal state machine for the MPD connection lifecycle.
#[derive(Debug)]
enum MpdState {
    Disconnected { backoff: ExponentialBackoff, first_attempt: bool },
    Connecting {
        #[allow(dead_code)]
        start_time: Instant,
    },
    #[allow(dead_code)]
    Connected { protocol_version: String },
    #[allow(dead_code)]
    Error { error: String, backoff: ExponentialBackoff, will_retry_at: Instant },
}

/// Exponential backoff for reconnection attempts.
#[derive(Debug)]
pub struct ExponentialBackoff {
    current: Duration,
    max: Duration,
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self::new()
    }
}

impl ExponentialBackoff {
    pub fn new() -> Self {
        Self {
            current: Duration::from_secs(1),
            max: Duration::from_secs(60),
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = std::cmp::min(self.current * 2, self.max);
        delay
    }

    pub fn reset(&mut self) {
        self.current = Duration::from_secs(1);
    }
}

/// The MPD event loop runs on a background thread, owning the adapter and state machine.
pub struct MpdEventLoop {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MpdEventLoop {
    /// Spawn the background thread and return a handle + command sender.
    pub fn spawn(
        host: String,
        port: u16,
        event_tx: mpsc::SyncSender<MpdEvent>,
    ) -> (Self, mpsc::Sender<MpdCommand>) {
        let (cmd_tx, cmd_rx) = mpsc::channel::<MpdCommand>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let handle = thread::Builder::new()
            .name("mpd-event-loop".into())
            .spawn(move || {
                let mut state = MpdState::Disconnected {
                    backoff: ExponentialBackoff::new(),
                    first_attempt: true,
                };

                loop {
                    if stop_clone.load(Ordering::Acquire) {
                        return;
                    }

                    state = match state {
                        MpdState::Disconnected { ref mut backoff, ref mut first_attempt } => {
                            if !*first_attempt {
                                let _ = event_tx.try_send(MpdEvent::Disconnected);
                            }
                            *first_attempt = false;

                            let delay = backoff.next_delay();
                            let wake = Instant::now() + delay;

                            // Wait for backoff delay or stop signal
                            loop {
                                if stop_clone.load(Ordering::Acquire) {
                                    return;
                                }
                                if Instant::now() >= wake {
                                    break;
                                }
                                thread::sleep(Duration::from_millis(50));
                            }

                            MpdState::Connecting { start_time: Instant::now() }
                        }
                        MpdState::Connecting { start_time: _ } => {
                            let _ = event_tx.try_send(MpdEvent::Connecting);
                            match MpdAdapter::connect(&host, port) {
                                Ok(adapter) => {
                                    let _ = event_tx.try_send(MpdEvent::Connected);
                                    connected_loop(adapter, &cmd_rx, &event_tx, &stop_clone);
                                    // When connected_loop exits, connection was lost.
                                    // Preserve backoff across the reconnect cycle.
                                    let backoff = ExponentialBackoff::new();
                                    MpdState::Disconnected { backoff, first_attempt: false }
                                }
                                Err(e) => {
                                    let _ = event_tx.try_send(MpdEvent::Error(e.to_string()));
                                    let b = ExponentialBackoff::new();
                                    MpdState::Error {
                                        error: e.to_string(),
                                        backoff: b,
                                        will_retry_at: Instant::now(),
                                    }
                                }
                            }
                        }
                        MpdState::Connected { protocol_version: _ } => {
                            // This variant documents the connected state's data.
                            // The actual connected phase is handled by connected_loop().
                            // If we reach here, the state transitions back to Disconnected.
                            let backoff = ExponentialBackoff::new();
                            MpdState::Disconnected { backoff, first_attempt: false }
                        }
                        MpdState::Error {
                            error: _,
                            ref mut backoff,
                            ref mut will_retry_at,
                        } => {
                            let delay = backoff.next_delay();
                            *will_retry_at = Instant::now() + delay;
                            thread::sleep(delay);
                            MpdState::Connecting { start_time: Instant::now() }
                        }
                    };
                }
            })
            .expect("Failed to spawn MPD event loop thread");

        (
            Self {
                stop,
                handle: Some(handle),
            },
            cmd_tx,
        )
    }

    pub fn signal_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// Expose the stop flag for external signal handlers.
    pub fn get_stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Extract the thread handle for timed join.
    pub fn into_handle(self) -> JoinHandle<()> {
        self.handle.expect("MpdEventLoop handle already taken")
    }

    pub fn join(self) -> thread::Result<()> {
        if let Some(h) = self.handle {
            h.join()
        } else {
            Ok(())
        }
    }
}

/// Run the connected phase: command processing and independent status polling.
fn connected_loop(
    mut adapter: MpdAdapter,
    cmd_rx: &mpsc::Receiver<MpdCommand>,
    event_tx: &mpsc::SyncSender<MpdEvent>,
    stop: &AtomicBool,
) {
    let mut last_status = Instant::now();
    let mut last_song_pos: Option<u32>;
    let mut consecutive_failures: u32;
    let mut last_playlist_version: Option<String> = None;

    // Initial status fetch
    if let Some(update) = fetch_full_update(&mut adapter) {
        last_song_pos = update.song;
        consecutive_failures = 0;
        let _ = event_tx.try_send(MpdEvent::StateChanged(update));
    } else {
        log::error!("[MPD] initial status fetch failed, connection may be dead");
        return;
    }

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }

        // Process commands with a short timeout so status polling runs independently
        match cmd_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(cmd) => {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                match cmd {
                    MpdCommand::Play => {
                        if let Err(e) = adapter.play() { log::error!("Play failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Pause => {
                        if let Err(e) = adapter.pause() { log::error!("Pause failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Stop => {
                        if let Err(e) = adapter.stop() { log::error!("Stop failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Next => {
                        if let Err(e) = adapter.next_track() { log::error!("Next failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Previous => {
                        if let Err(e) = adapter.previous() { log::error!("Previous failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Seek(pos) => {
                        if let Err(e) = adapter.seek(pos) { log::error!("Seek failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Status => {
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::CurrentSong => {
                        if let Ok(Some(song)) = adapter.current_song() {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(
                                parse_song_update(&song)));
                        }
                    }
                    MpdCommand::ListAlbums => {
                        if let Ok(albums) = adapter.list_albums() {
                            let _ = event_tx.try_send(MpdEvent::Albums(albums));
                        }
                    }
                    MpdCommand::ListAlbumsGrouped(group) => {
                        if let Ok(groups) = adapter.list_albums_grouped(&group) {
                            let _ = event_tx.try_send(MpdEvent::AlbumsGrouped(groups));
                        }
                    }
                    MpdCommand::Search(query) => {
                        if let Ok(results) = adapter.search_albums(&query) {
                            let _ = event_tx.try_send(MpdEvent::SearchResults(results));
                        }
                    }
                    MpdCommand::ListDirectory(path) => {
                        if let Ok(entries) = adapter.lsinfo(&path) {
                            let _ = event_tx.try_send(MpdEvent::DirectoryListing(path, entries));
                        }
                    }
                    MpdCommand::PlayFile(path) => {
                        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
                        if let Err(e) = adapter.send_command("clear") { log::error!("PlayFile clear failed: {e}"); }
                        if let Err(e) = adapter.send_command(&format!("add \"{}\"", escaped)) { log::error!("PlayFile add failed: {e}"); }
                        if let Err(e) = adapter.send_command("play 0") { log::error!("PlayFile play failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        if let Ok(queue) = adapter.list_queue() { let _ = event_tx.try_send(MpdEvent::Queue(queue)); }
                        last_status = Instant::now();
                    }
                    MpdCommand::ListQueue => {
                        if let Ok(queue) = adapter.list_queue() {
                            let _ = event_tx.try_send(MpdEvent::Queue(queue));
                        }
                    }
                    MpdCommand::PlayPosition(pos) => {
                        if let Err(e) = adapter.send_command(&format!("play {}", pos)) {
                            log::error!("PlayPosition failed: {e}");
                        }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        if let Ok(queue) = adapter.list_queue() { let _ = event_tx.try_send(MpdEvent::Queue(queue)); }
                        last_status = Instant::now();
                    }
                    MpdCommand::DeleteId(id) => {
                        if let Err(e) = adapter.send_command(&format!("deleteid {}", id)) {
                            log::error!("DeleteId failed: {e}");
                        }
                        if let Ok(queue) = adapter.list_queue() {
                            let _ = event_tx.try_send(MpdEvent::Queue(queue));
                        }
                    }
                    MpdCommand::MoveId(id, to_pos) => {
                        if let Err(e) = adapter.send_command(&format!("moveid {} {}", id, to_pos)) {
                            log::error!("MoveId failed: {e}");
                        }
                        if let Ok(queue) = adapter.list_queue() {
                            let _ = event_tx.try_send(MpdEvent::Queue(queue));
                        }
                    }
                    MpdCommand::Add(album) => {
                        match adapter.find_album_uris(&album) {
                            Ok(uris) => {
                                for uri in &uris { if let Err(e) = adapter.addid(uri) { log::error!("Add addid failed: {e}"); } }
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                }
                                if let Ok(queue) = adapter.list_queue() { let _ = event_tx.try_send(MpdEvent::Queue(queue)); }
                                last_status = Instant::now();
                            }
                            Err(e) => log::error!("Add album failed: {e}"),
                        }
                    }
                    MpdCommand::InsertNext(album) => {
                        match adapter.find_album_uris(&album) {
                            Ok(uris) => {
                                let current_pos = adapter.status()
                                    .ok()
                                    .and_then(|s| s.get("song").cloned())
                                    .and_then(|s| s.parse::<i32>().ok())
                                    .unwrap_or(-1);
                                for (i, uri) in uris.iter().enumerate() {
                                    match adapter.addid(uri) {
                                        Ok(id) => {
                                            let target = (current_pos + 1 + i as i32).max(0);
                                            if let Err(e) = adapter.send_command(&format!("moveid {} {}", id, target)) {
                                                log::error!("InsertNext moveid failed: {e}");
                                            }
                                        }
                                        Err(e) => log::error!("InsertNext addid failed: {e}"),
                                    }
                                }
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                }
                                if let Ok(queue) = adapter.list_queue() { let _ = event_tx.try_send(MpdEvent::Queue(queue)); }
                                last_status = Instant::now();
                            }
                            Err(e) => log::error!("InsertNext find failed: {e}"),
                        }
                    }
                    MpdCommand::PlayAlbum(album) => {
                        match adapter.find_album_uris(&album) {
                            Ok(uris) => {
                                if let Err(e) = adapter.send_command("clear") { log::error!("PlayAlbum clear failed: {e}"); }
                                for uri in &uris {
                                    if let Err(e) = adapter.addid(uri) { log::error!("PlayAlbum addid failed: {e}"); }
                                }
                                if let Err(e) = adapter.send_command("play 0") { log::error!("PlayAlbum play failed: {e}"); }
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                }
                                if let Ok(queue) = adapter.list_queue() { let _ = event_tx.try_send(MpdEvent::Queue(queue)); }
                                last_status = Instant::now();
                            }
                            Err(e) => log::error!("PlayAlbum failed: {e}"),
                        }
                    }
                    MpdCommand::Clear => {
                        if let Err(e) = adapter.send_command("clear") { log::error!("Clear failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        }
                        if let Ok(queue) = adapter.list_queue() { let _ = event_tx.try_send(MpdEvent::Queue(queue)); }
                        last_status = Instant::now();
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Fall through to status poll check below
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return;
            }
        }

        // Independent status polling every 500ms — not tied to command arrival
        if last_status.elapsed() >= Duration::from_millis(500) {
            last_status = Instant::now();
            if let Some(update) = fetch_full_update(&mut adapter) {
                consecutive_failures = 0;
                let new_song = update.song;
                let song_changed = new_song != last_song_pos;
                let playlist_changed = update.playlist_version != last_playlist_version;
                if playlist_changed && update.playlist_version.is_some() {
                    last_playlist_version = update.playlist_version.clone();
                    let _ = event_tx.try_send(MpdEvent::LibraryChanged);
                }
                match event_tx.try_send(MpdEvent::StateChanged(update)) {
                    Ok(()) => {
                        if song_changed {
                            last_song_pos = new_song;
                        }
                    }
                    Err(mpsc::TrySendError::Full(_)) => {
                        log::warn!("[MPD] channel FULL — StateChanged event DROPPED");
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        log::error!("[MPD] channel disconnected");
                        return;
                    }
                }
                if song_changed && last_song_pos == new_song {
                    if let Ok(queue) = adapter.list_queue() {
                        let _ = event_tx.try_send(MpdEvent::Queue(queue));
                    }
                }
            } else {
                consecutive_failures += 1;
                log::error!("[MPD] status poll failed ({consecutive_failures}/3)");
                if consecutive_failures >= 3 {
                    log::error!("[MPD] connection dead after 3 consecutive poll failures, triggering reconnect");
                    return;
                }
            }
        }
    }
}

fn parse_status_update(status: &std::collections::HashMap<String, String>) -> PlaybackUpdate {
    PlaybackUpdate {
        state: status.get("state").cloned().unwrap_or_default(),
        song: status.get("song").and_then(|v| v.parse().ok()),
        volume: status.get("volume")
            .and_then(|v| v.parse::<i16>().ok())
            .map(|v| if v == -1 { 0 } else { v })
            .unwrap_or(0),
        elapsed: status.get("elapsed").and_then(|v| v.parse().ok()),
        duration: status.get("duration").and_then(|v| v.parse().ok()),
        playlist_version: status.get("playlist").cloned(),
        ..Default::default()
    }
}

fn parse_song_update(song: &std::collections::HashMap<String, String>) -> PlaybackUpdate {
    PlaybackUpdate {
        artist: song.get("Artist").cloned(),
        title: song.get("Title").cloned(),
        album: song.get("Album").cloned(),
        ..Default::default()
    }
}

/// Fetch a complete PlaybackUpdate: status + currentsong metadata when a song is active.
fn fetch_full_update(adapter: &mut MpdAdapter) -> Option<PlaybackUpdate> {
    let status = adapter.status().ok()?;
    let mut update = parse_status_update(&status);
    if update.song.is_some() {
        if let Ok(Some(song)) = adapter.current_song() {
            let song_update = parse_song_update(&song);
            if update.artist.is_none() { update.artist = song_update.artist; }
            if update.title.is_none() { update.title = song_update.title; }
            if update.album.is_none() { update.album = song_update.album; }
        }
    }
    Some(update)
}
