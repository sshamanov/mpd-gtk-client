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
    Reconnect,
    FetchCovers(Vec<(String, String)>),
    /// Fetch all tracks for a given album name.
    ListAlbumTracks(String),
    /// Send the MPD `close` command and exit the connected loop gracefully.
    Close,
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
    CoverPaths(std::collections::HashMap<String, Option<String>>),
    /// Cover art data fetched by ActualRead, carrying raw JPEG bytes for direct texture decode.
    CoverRefreshed { album_id: String, data: Vec<u8> },
    /// Tracks of the currently playing album: Vec<(title, file, duration)>.
    AlbumTracks(Vec<(String, String, f64)>),
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
    pub format: Option<String>,
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
        _host: String,
        _port: u16,
        event_tx: mpsc::SyncSender<MpdEvent>,
        conn_params: Arc<std::sync::Mutex<(String, u16)>>,
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
                            let (ref c_host, c_port) = *conn_params.lock().expect("conn_params lock");
                            match MpdAdapter::connect(c_host, c_port) {
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
    let mut local_queue: Vec<crate::mpd::QueueEntry> = Vec::new();
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("mpd-client")
        .join("covers");
    let cover_provider = std::sync::Arc::new(std::sync::RwLock::new(crate::coverart::CoverProvider::new()));
    let mut actual_read = crate::coverart::ActualRead::new(cache_dir);
    let mut cached_flat_albums: Vec<(String, String)> = Vec::new();

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
                        // Use cache for Albums/Artist views — no MPD round-trip needed
                        if !cached_flat_albums.is_empty() && (group == "Albums" || group == "Artist") {
                            log::info!("[MPD] ListAlbumsGrouped({group}): using cache ({})", cached_flat_albums.len());
                            let groups = if group == "Albums" {
                                vec![("All Albums".into(), cached_flat_albums.clone())]
                            } else {
                                group_albums_by_artist(&cached_flat_albums)
                            };
                            let _ = event_tx.try_send(MpdEvent::AlbumsGrouped(groups));
                        } else if let Ok(groups) = adapter.list_albums_grouped(&group) {
                            log::info!("[MPD] ListAlbumsGrouped({group}): fetched from MPD");
                            // Cache the flat list for future local regrouping
                            if group == "Albums" {
                                cached_flat_albums = groups.iter()
                                    .flat_map(|(_, a)| a.clone()).collect();
                            }
                            let _ = event_tx.try_send(MpdEvent::AlbumsGrouped(groups));
                        }
                    }
                    MpdCommand::FetchCovers(albums) => {
                        // Enqueue covers for background fetch; processed one per idle cycle below
                        actual_read.enqueue(albums);
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
                    MpdCommand::ListAlbumTracks(album) => {
                        if let Ok(tracks) = adapter.find_album_tracks(&album) {
                            let _ = event_tx.try_send(MpdEvent::AlbumTracks(tracks));
                        }
                    }
                    MpdCommand::PlayFile(path) => {
                        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
                        let cmds = vec![
                            "clear".to_string(),
                            format!("add \"{escaped}\""),
                            "play 0".to_string(),
                        ];
                        if let Err(e) = adapter.send_batch(&cmds) { log::error!("PlayFile failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let pv = update.playlist_version.clone();
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                            sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, pv.as_deref());
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::ListQueue => {
                        sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, None);
                    }
                    MpdCommand::PlayPosition(pos) => {
                        if let Err(e) = adapter.send_command(&format!("play {}", pos)) {
                            log::error!("PlayPosition failed: {e}");
                        }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let pv = update.playlist_version.clone();
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                            sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, pv.as_deref());
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::DeleteId(id) => {
                        if let Err(e) = adapter.send_command(&format!("deleteid {}", id)) {
                            log::error!("DeleteId failed: {e}");
                        }
                        sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, None);
                    }
                    MpdCommand::MoveId(id, to_pos) => {
                        if let Err(e) = adapter.send_command(&format!("moveid {} {}", id, to_pos)) {
                            log::error!("MoveId failed: {e}");
                        }
                        sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, None);
                    }
                    MpdCommand::Add(album) => {
                        match adapter.find_album_uris(&album) {
                            Ok(uris) => {
                                if uris.is_empty() { break; }
                                let cmds: Vec<String> = uris.iter()
                                    .map(|uri| format!("addid \"{}\"", uri.replace('\\', "\\\\").replace('"', "\\\"")))
                                    .collect();
                                if let Err(e) = adapter.send_batch(&cmds) { log::error!("Add album failed: {e}"); }
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let pv = update.playlist_version.clone();
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                    sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, pv.as_deref());
                                }
                                last_status = Instant::now();
                            }
                            Err(e) => log::error!("Add album failed: {e}"),
                        }
                    }
                    MpdCommand::InsertNext(album) => {
                        match adapter.find_album_uris(&album) {
                            Ok(uris) => {
                                if uris.is_empty() { break; }
                                let current_pos = adapter.status()
                                    .ok()
                                    .and_then(|s| s.get("song").cloned())
                                    .and_then(|s| s.parse::<i32>().ok())
                                    .unwrap_or(-1);
                                if current_pos < 0 { break; }
                                // Batch all addid calls with position parameter to avoid per-item
                                // round-trips and eliminate the race between addid and moveid.
                                let mut cmds = Vec::with_capacity(uris.len());
                                for (i, uri) in uris.iter().enumerate() {
                                    let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
                                    let target = current_pos + 1 + i as i32;
                                    cmds.push(format!("addid \"{escaped}\" {target}"));
                                }
                                if let Err(e) = adapter.send_batch(&cmds) {
                                    log::error!("InsertNext batch failed: {e}");
                                }
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let pv = update.playlist_version.clone();
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                    sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, pv.as_deref());
                                }
                                last_status = Instant::now();
                            }
                            Err(e) => log::error!("InsertNext find failed: {e}"),
                        }
                    }
                    MpdCommand::PlayAlbum(album) => {
                        match adapter.find_album_uris(&album) {
                            Ok(uris) => {
                                let mut cmds = vec!["clear".to_string()];
                                for uri in &uris {
                                    let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
                                    cmds.push(format!("addid \"{escaped}\""));
                                }
                                cmds.push("play 0".to_string());
                                if let Err(e) = adapter.send_batch(&cmds) { log::error!("PlayAlbum failed: {e}"); }
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let pv = update.playlist_version.clone();
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                    sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, pv.as_deref());
                                }
                                last_status = Instant::now();
                            }
                            Err(e) => log::error!("PlayAlbum failed: {e}"),
                        }
                    }
                    MpdCommand::Clear => {
                        if let Err(e) = adapter.send_command("clear") { log::error!("Clear failed: {e}"); }
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let pv = update.playlist_version.clone();
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                            sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, pv.as_deref());
                        }
                        last_status = Instant::now();
                    }
                    MpdCommand::Reconnect => {
                        log::info!("[MPD] received Reconnect command, restarting connection");
                        return;
                    }
                    MpdCommand::Close => {
                        log::info!("[MPD] received Close command, sending close to MPD");
                        let _ = adapter.send_command("close");
                        // Prevent the outer state machine from attempting reconnection
                        stop.store(true, Ordering::Release);
                        return;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Process one pending cover fetch per idle cycle to avoid blocking
                if actual_read.has_pending() {
                    let caps = adapter.capabilities.clone();
                    actual_read.process_one(&mut adapter, &caps, &cover_provider.read().unwrap(), event_tx);
                }
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
                let current_version = update.playlist_version.clone();
                let playlist_changed = current_version != last_playlist_version;
                if playlist_changed && current_version.is_some() {
                    last_playlist_version = current_version.clone();
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
                    sync_queue(&mut adapter, event_tx, &mut local_queue, &mut last_playlist_version, current_version.as_deref());
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

/// Sync the local queue copy from MPD when the playlist version has changed.
/// Skips the round-trip when the version matches (`last_version == current_version`).
/// When `current_version` is `None`, fetches status to determine the version.
fn sync_queue(
    adapter: &mut MpdAdapter,
    event_tx: &mpsc::SyncSender<MpdEvent>,
    local_queue: &mut Vec<crate::mpd::QueueEntry>,
    last_version: &mut Option<String>,
    current_version: Option<&str>,
) {
    let version = current_version
        .map(|v| v.to_string())
        .or_else(|| {
            adapter.status().ok()
                .and_then(|s| s.get("playlist").cloned())
        });

    let Some(ref ver) = version else { return };

    // Skip if version hasn't changed since last sync
    if last_version.as_deref() == Some(ver.as_str()) {
        return;
    }

    match adapter.list_queue() {
        Ok(queue) => {
            *local_queue = queue;
            *last_version = Some(ver.clone());
            let _ = event_tx.try_send(MpdEvent::Queue(local_queue.clone()));
        }
        Err(e) => log::error!("queue sync failed: {e}"),
    }
}

/// Group a flat (artist, album) list by artist name, sorted alphabetically.
fn group_albums_by_artist(albums: &[(String, String)]) -> crate::mpd::AlbumGroup {
    let mut map: std::collections::BTreeMap<String, Vec<(String, String)>> = std::collections::BTreeMap::new();
    for (artist, album) in albums {
        let key = if artist.is_empty() { "Unknown Artist" } else { artist.as_str() };
        map.entry(key.to_string()).or_default().push((artist.clone(), album.clone()));
    }
    map.into_iter().collect()
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
    let format = format_badge_text(song);
    PlaybackUpdate {
        artist: song.get("Artist").cloned(),
        title: song.get("Title").cloned(),
        album: song.get("Album").cloned(),
        format,
        ..Default::default()
    }
}

/// Build a compact format badge from MPD currentsong audio metadata.
fn format_badge_text(song: &std::collections::HashMap<String, String>) -> Option<String> {
    // Check for DSD audio first (Audio field: "dsd64", "dsd128", etc.)
    if let Some(audio) = song.get("Audio") {
        let lower = audio.to_lowercase();
        if lower.starts_with("dsd") {
            return Some(audio.to_uppercase());
        }
    }
    // PCM: use Format field (e.g., "44100:24:2" → "24/44.1")
    // DSD via format: "2822400:1:2" → "DSD64"
    if let Some(format) = song.get("Format") {
        let parts: Vec<&str> = format.split(':').collect();
        if parts.len() >= 2 {
            if let Ok(rate) = parts[0].parse::<u32>() {
                // DSD rates: 2822400 = DSD64, 5644800 = DSD128, etc.
                let dsd_base: u32 = 44100 * 64; // 2822400
                if rate >= dsd_base && rate % dsd_base == 0 {
                    let mult = rate / dsd_base;
                    return Some(format!("DSD{}", mult * 64));
                }
            }
            if parts.len() == 3 {
                if let Ok(bits) = parts[1].parse::<u32>() {
                    let rate_str = if let Ok(r) = parts[0].parse::<f64>() {
                        format!("{:.1}", r / 1000.0).trim_end_matches('0').trim_end_matches('.').to_string()
                    } else { parts[0].to_string() };
                    return Some(format!("{}/{}", bits, rate_str));
                }
            }
        }
    }
    None
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
            if update.format.is_none() { update.format = song_update.format; }
        }
    }
    Some(update)
}
