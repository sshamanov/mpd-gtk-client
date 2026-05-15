//! MPD state machine — connection lifecycle, command dispatch, event emission. Thread: dedicated background thread.

#![allow(clippy::expect_used)]

use crate::mpd::cover::{self, CoverFetchResult, CoverThreadSender};
use crate::mpd::{ConnectionTarget, DirEntry, MpdAdapter, MpdStream};
use crate::search::{SearchCommand, SearchCommandSender};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Commands sent from the UI thread to the MPD background thread.
#[derive(Debug)]
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
    SearchFiles(String),
    ListDirectory(String),
    PlayFile(String),
    ListQueue,
    PlayPosition(i32),
    DeleteId(i32),
    MoveId(i32, i32),
    Add(String),
    /// Add album at a specific queue position.
    AddAt(String, i32),
    InsertNext(String),
    PlayAlbum(String),
    /// Play a list of URIs directly (clear + add each + play 0).
    /// Used by folder tree for CUE/DSD normalized albums.
    PlayUris(Vec<String>),
    /// Add a list of URIs directly to the queue (no clear, no play).
    AddUris(Vec<String>),
    /// Insert a list of URIs after the current track (for file-based "play next").
    InsertNextUris(Vec<String>),
    /// List directory contents, clear queue, add all files, and play.
    PlayDirectory(String),
    /// List directory contents and add all files to queue.
    AddDirectory(String),
    /// List directory contents and insert all files after current track.
    InsertNextDirectory(String),
    Clear,
    Reconnect,
    FetchCovers(Vec<(String, String)>),
    /// Fetch all tracks for a given album name.
    ListAlbumTracks(String),
    /// Send the MPD `close` command and exit the connected loop gracefully.
    Close,
    /// Trigger MPD's `update` (rescan the music library).
    Update,
    /// Execute multiple commands as a single atomic MPD command list.
    /// Sub-commands that map to single MPD protocol lines are collected and sent
    /// via `command_list_begin`/`command_list_end`.
    Batch(Vec<MpdCommand>),
}

/// Events emitted by the MPD background thread to the UI thread.
#[derive(Debug, Clone)]
pub enum MpdEvent {
    Connected,
    Connecting,
    Disconnected,
    StateChanged(PlaybackUpdate),
    Albums(Vec<crate::mpd::AlbumMeta>),
    AlbumsGrouped(crate::mpd::AlbumGroup),
    SearchResults { results: Vec<(String, String)>, generation: u64 },
    /// Search index not yet built — signal for UI to show indexing state.
    SearchIndexing,
    FileSearchResults(Vec<(String, String)>),
    DirectoryListing(String, Vec<crate::mpd::DirEntry>),
    Queue(Vec<crate::mpd::QueueEntry>),
    LibraryChanged,
    CoverPaths(std::collections::HashMap<String, Option<String>>),
    /// Cover art data fetched by ActualRead, carrying raw JPEG bytes for direct texture decode.
    CoverRefreshed { album_id: String, data: Vec<u8> },
    /// Tracks of the currently playing album: Vec<(title, file, duration)>.
    AlbumTracks(Vec<(String, String, f64)>),
    /// User-facing notification with severity level.
    Toast { message: String, level: ToastLevel },
    Error(String),
}

/// Severity level for Toast events — determines display behaviour in-app and via desktop notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    /// Informational — auto-dismiss after 3s in-app, default timeout on desktop.
    Info,
    /// Warning — auto-dismiss after 5s in-app, default timeout on desktop.
    Warn,
    /// Error — persists until user dismisses in-app, persistent on desktop.
    Error,
}

impl ToastLevel {
    /// Default toast timeout in seconds.
    /// - Error: 0 (persistent, must be manually dismissed)
    /// - Warn: 5 seconds
    /// - Info: 3 seconds
    pub const fn default_timeout_seconds(self) -> u32 {
        match self {
            ToastLevel::Error => 0,
            ToastLevel::Warn => 5,
            ToastLevel::Info => 3,
        }
    }
}

/// Parsed audio format with sample rate, bit depth, and codec.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub bit_depth: u16,
    pub codec: String,
    pub is_dsd: bool,
}

impl AudioFormat {
    pub fn display_text(&self) -> String {
        if self.is_dsd {
            format!("DSD{}", self.bit_depth as u32 * 64)
        } else if self.bit_depth > 0 && self.sample_rate > 0 {
            let rate = (self.sample_rate as f64) / 1000.0;
            let rate_str = format!("{:.1}", rate)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string();
            format!("{}/{} · {}", self.bit_depth, rate_str, self.codec)
        } else if !self.codec.is_empty() {
            self.codec.clone()
        } else {
            String::new()
        }
    }
}

/// Unified audio format parser — checks three MPD fields in priority order.
///
/// Priority: `songs_audio` (file format, immutable) → `songs_format` (file sample spec)
/// → `status_audio` (DAC output, may be resampled). File format wins over output format.
pub fn parse_mpd_audio_format(
    songs_audio: Option<&str>,
    songs_format: Option<&str>,
    status_audio: Option<&str>,
) -> Option<AudioFormat> {
    // 1. Try currentsong Audio field (file format, immutable) — e.g., "dsd64", "44100:24:2"
    if let Some(audio) = songs_audio {
        if let Some(fmt) = parse_single_audio_source(audio, false) {
            return Some(fmt);
        }
    }
    // 2. Try currentsong Format field
    if let Some(format) = songs_format {
        if let Some(fmt) = parse_single_audio_source(format, false) {
            return Some(fmt);
        }
    }
    // 3. Fall back to status audio (DAC output, may be resampled)
    if let Some(audio) = status_audio {
        if let Some(fmt) = parse_single_audio_source(audio, true) {
            return Some(fmt);
        }
    }
    None
}

fn parse_single_audio_source(raw: &str, _is_dac_output: bool) -> Option<AudioFormat> {
    let lower = raw.to_lowercase();
    // DSD: "dsd64", "dsd128:2", "DSD128", "dsd256", "dsd512"
    let dsd_prefixes = [("dsd64", 64u32, 1u16), ("dsd128", 128, 2), ("dsd256", 256, 4), ("dsd512", 512, 8)];
    for (prefix, _, multiplier) in &dsd_prefixes {
        if lower.starts_with(prefix) {
            let rate = 44100 * 64 * (*multiplier as u32 / 1);
            return Some(AudioFormat {
                sample_rate: rate,
                bit_depth: 1,
                codec: prefix.to_uppercase(),
                is_dsd: true,
            });
        }
    }
    // DSD via format rate: "2822400:1:2" or similar
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() >= 2 {
        if let Ok(rate) = parts[0].parse::<u32>() {
            let dsd_base: u32 = 44100 * 64;
            if rate >= dsd_base && rate % dsd_base == 0 {
                let mult = rate / dsd_base;
                let dsd_val = mult * 64;
                return Some(AudioFormat {
                    sample_rate: rate,
                    bit_depth: 1,
                    codec: format!("DSD{}", dsd_val),
                    is_dsd: true,
                });
            }
        }
        if parts.len() == 3 {
            if let Ok(bits) = parts[1].parse::<u16>() {
                let rate = parts[0].parse::<u32>().unwrap_or(0);
                return Some(AudioFormat {
                    sample_rate: rate,
                    bit_depth: bits,
                    codec: String::new(),
                    is_dsd: false,
                });
            }
        }
    }
    None
}

/// Playback state update from the MPD status response.
#[derive(Debug, Clone, Default)]
pub struct PlaybackUpdate {
    pub state: String,
    pub song: Option<u32>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub volume: i16,
    pub elapsed: Option<f64>,
    pub duration: Option<f64>,
    pub playlist_version: Option<String>,
    pub format: Option<String>,
    pub file: Option<String>,
    pub bitrate: Option<String>,
    pub audio_format: Option<AudioFormat>,
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

/// Thread-safe MPD command sender that writes `noidle\n` to a stream clone
/// before forwarding each command. Breaks the worker thread out of MPD `idle`
/// mode so it can process the command immediately.
///
/// The stream clone is stored after the connection is established inside
/// `connected_loop`. Before that, commands are queued in the mpsc channel
/// normally (worker isn't in idle mode yet).
#[derive(Clone)]
pub struct CommandSender {
    inner: mpsc::Sender<MpdCommand>,
    noidle_socket: Arc<Mutex<Option<MpdStream>>>,
}

impl CommandSender {
    fn new(inner: mpsc::Sender<MpdCommand>) -> Self {
        Self {
            inner,
            noidle_socket: Arc::new(Mutex::new(None)),
        }
    }

    /// Forward a command to the MPD background thread. Writes `noidle\n`
    /// to a temporary stream clone to break the worker out of `idle` mode
    /// before the command is processed.
    pub fn send(&self, cmd: MpdCommand) -> Result<(), mpsc::SendError<MpdCommand>> {
        if let Ok(guard) = self.noidle_socket.lock() {
            if let Some(ref stream) = *guard {
                if let Ok(clone) = stream.try_clone() {
                    drop(guard);
                    let mut write_clone = clone;
                    let _ = write_clone.write_all(b"noidle\n");
                }
            }
        }
        self.inner.send(cmd)
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
        event_tx: mpsc::SyncSender<MpdEvent>,
        conn_params: Arc<std::sync::Mutex<ConnectionTarget>>,
    ) -> (Self, CommandSender, std::sync::Arc<crate::metadata::MetadataCache>, SearchCommandSender) {
        let (cmd_tx, cmd_rx) = mpsc::channel::<MpdCommand>();
        let cmd_sender = CommandSender::new(cmd_tx);
        let noidle_socket = cmd_sender.noidle_socket.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let stop_search = stop.clone();
        let cover_proc_running = Arc::new(AtomicBool::new(false));
        let metadata_cache = std::sync::Arc::new(crate::metadata::MetadataCache::new());
        let mc_thread = metadata_cache.clone();

        // Search worker: commands are triggered by keystrokes (max ~6/s with 150ms
        // debounce); 64 slots absorbs bursts. try_send drops when full as intentional
        // backpressure, keeping the UI responsive under heavy typing.
        let (search_cmd_tx, search_cmd_rx) = mpsc::sync_channel::<SearchCommand>(64);
        let search_sender = SearchCommandSender::new(search_cmd_tx);
        let search_event_tx = event_tx.clone();
        crate::search::worker::spawn(search_cmd_rx, search_event_tx, stop_search);

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
                                let _ = event_tx.try_send(MpdEvent::Toast {
                                    message: "MPD connection lost — retrying...".into(),
                                    level: ToastLevel::Warn,
                                });
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
                            let target = conn_params.lock().expect("conn_params lock").clone();
                            match MpdAdapter::connect(&target) {
                                Ok(adapter) => {
                                    let _ = event_tx.try_send(MpdEvent::Connected);
                                    mc_thread.clear();
                                    let cover_target = target.clone();
                                    connected_loop(adapter, &cmd_rx, &event_tx, &stop_clone, noidle_socket.clone(), &mc_thread, cover_target, &cover_proc_running);
                                    // When connected_loop exits, connection was lost.
                                    // Preserve backoff across the reconnect cycle.
                                    let backoff = ExponentialBackoff::new();
                                    MpdState::Disconnected { backoff, first_attempt: false }
                                }
                                Err(e) => {
                                    let _ = event_tx.try_send(MpdEvent::Toast {
                                        message: format!("MPD connection failed: {e}\n\nCheck your MPD server and settings."),
                                        level: ToastLevel::Error,
                                    });
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
            cmd_sender,
            metadata_cache,
            search_sender,
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

/// Run the connected phase: command processing with MPD idle protocol.
/// Blocks on MPD `idle` when no commands are pending — instant subsystem notification
/// instead of 500ms polling. Falls back to polling for MPD < 0.19 or transient errors.
fn connected_loop(
    mut adapter: MpdAdapter,
    cmd_rx: &mpsc::Receiver<MpdCommand>,
    event_tx: &mpsc::SyncSender<MpdEvent>,
    stop: &Arc<AtomicBool>,
    noidle_socket: Arc<Mutex<Option<MpdStream>>>,
    metadata_cache: &crate::metadata::MetadataCache,
    cover_target: ConnectionTarget,
    cover_proc_running: &Arc<AtomicBool>,
) {
    let mut last_status = Instant::now();
    let mut last_song_pos: Option<u32>;
    let mut consecutive_failures: u32;
    let mut last_playlist_version: Option<String> = None;
    let mut local_queue: Vec<crate::mpd::QueueEntry> = Vec::new();
    let mut plchanges_count: u32 = 0;
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("mpd-client")
        .join("covers");
    let cover_provider = std::sync::Arc::new(std::sync::RwLock::new(crate::coverart::CoverProvider::new()));
    let mut actual_read = crate::coverart::ActualRead::new();
    let mut cached_flat_albums: Vec<crate::mpd::AlbumMeta> = Vec::new();
    let mut use_idle = true;
    let mut transient_failures: u32 = 0;

    // Cover thread (story 28-1): separate MPD connection for binary cover data
    let (cover_result_tx, cover_result_rx) = mpsc::sync_channel::<CoverFetchResult>(64);
    let mut cover_tx: Option<CoverThreadSender> = None;

    // Cover Proc worker (story 28-2): JPEG decode, MD5 hash, cache write, emit RGBA
    crate::coverart::cover_proc::spawn(
        cover_result_rx,
        event_tx.clone(),
        cover_provider.clone(),
        cache_dir.clone(),
        stop.clone(),
        cover_proc_running.clone(),
    );

    // Set the stream clone for the main thread's CommandSender
    if let Ok(clone) = adapter.stream_clone() {
        *noidle_socket.lock().expect("noidle_socket lock") = Some(clone);
        log::info!("[MPD] stream clone ready for idle break");
    } else {
        log::warn!("[MPD] stream_clone failed, disabling idle protocol");
        use_idle = false;
    }

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

        // ── Idle phase ──
        if use_idle {
            match adapter.idle() {
                Ok(subsystems) => {
                    transient_failures = 0;
                    consecutive_failures = 0;
                    // Subsystem-specific refresh
                    let mut needs_full_status = false;
                    for subsystem in &subsystems {
                        match subsystem.as_str() {
                            "player" => {
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let new_song = update.song;
                                    let song_changed = new_song != last_song_pos;
                                    let pv = update.playlist_version.clone();
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                    last_status = Instant::now();
                                    if song_changed {
                                        last_song_pos = new_song;
                                        if let Some(ref ver) = pv {
                                            sync_queue(&mut adapter, event_tx, &mut local_queue,
                                                &mut last_playlist_version, Some(ver), &mut plchanges_count);
                                        }
                                    }
                                }
                            }
                            "playlist" | "options" => {
                                if let Some(update) = fetch_full_update(&mut adapter) {
                                    let pv = update.playlist_version.clone();
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                    last_status = Instant::now();
                                    sync_queue(&mut adapter, event_tx, &mut local_queue,
                                        &mut last_playlist_version, pv.as_deref(), &mut plchanges_count);
                                }
                            }
                            "mixer" => {
                                // Only volume changed — lightweight status check
                                if let Ok(status) = adapter.status() {
                                    let mut update = parse_status_update(&status);
                                    if let Ok(Some(song)) = adapter.current_song() {
                                        let song_update = parse_song_update(&song);
                                        if update.artist.is_none() { update.artist = song_update.artist; }
                                        if update.title.is_none() { update.title = song_update.title; }
                                        if update.album.is_none() { update.album = song_update.album; }
                                        if update.year.is_none() { update.year = song_update.year; }
                                    }
                                    let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                                    last_status = Instant::now();
                                }
                            }
                            "database" | "update" | "stored_playlist" => {
                                let _ = event_tx.try_send(MpdEvent::LibraryChanged);
                                needs_full_status = true;
                            }
                            _ => {
                                log::debug!("[MPD] idle: unknown subsystem '{subsystem}', full refresh");
                                needs_full_status = true;
                            }
                        }
                    }
                    if needs_full_status {
                        if let Some(update) = fetch_full_update(&mut adapter) {
                            let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                            last_status = Instant::now();
                        }
                    }
                    // Drain pending commands (non-blocking)
                    loop {
                        if stop.load(Ordering::Acquire) { return; }
                        match cmd_rx.try_recv() {
                            Ok(cmd) => {
                                if process_command(cmd, &mut adapter, event_tx, &mut actual_read, &cover_provider,
                                    &mut last_status, &mut last_song_pos, &mut last_playlist_version,
                                    &mut local_queue, &mut cached_flat_albums, &mut consecutive_failures,
                                    stop, metadata_cache,
                                    &mut cover_tx, &cover_target, &cover_result_tx, &mut plchanges_count,
                                ) {
                                    return;
                                }
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => return,
                        }
                    }
                    // Cover results are drained by Cover Proc worker (story 28-2)
                }
                Err(ref e) if e.to_string().contains("unknown command") => {
                    log::info!("[MPD] idle not supported, falling back to 500ms polling");
                    use_idle = false;
                }
                Err(e) => {
                    // Transient error — poll briefly then retry idle
                    log::warn!("[MPD] idle transient error: {e}, polling fallback");
                    transient_failures += 1;
                    if transient_failures >= 10 {
                        log::info!("[MPD] transient window passed, retrying idle");
                        transient_failures = 0;
                    }
                }
            }
        }

        // ── Poll fallback cycle ──
        if !use_idle || transient_failures > 0 {
            match cmd_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(cmd) => {
                    if stop.load(Ordering::Acquire) { return; }
                    if process_command(cmd, &mut adapter, event_tx, &mut actual_read, &cover_provider,
                        &mut last_status, &mut last_song_pos, &mut last_playlist_version,
                        &mut local_queue, &mut cached_flat_albums, &mut consecutive_failures,
                        stop, metadata_cache,
                        &mut cover_tx, &cover_target, &cover_result_tx, &mut plchanges_count,
                    ) {
                        return;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Cover results are drained by Cover Proc worker (story 28-2)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }

            // Independent status polling every 500ms (only in permanent polling fallback)
            if !use_idle {
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
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                        if song_changed && last_song_pos == new_song {
                            sync_queue(&mut adapter, event_tx, &mut local_queue,
                                &mut last_playlist_version, current_version.as_deref(), &mut plchanges_count);
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
    }
}

/// Process a single MPD command — dispatch to the appropriate adapter method
/// and emit events. Extracted so both idle and poll paths can call it.
/// Returns `true` if the caller should exit the connected loop (Reconnect/Close).
#[allow(clippy::too_many_arguments)]
fn process_command(
    cmd: MpdCommand,
    adapter: &mut MpdAdapter,
    event_tx: &mpsc::SyncSender<MpdEvent>,
    actual_read: &mut crate::coverart::ActualRead,
    _cover_provider: &std::sync::Arc<std::sync::RwLock<crate::coverart::CoverProvider>>,
    last_status: &mut Instant,
    _last_song_pos: &mut Option<u32>,
    last_playlist_version: &mut Option<String>,
    local_queue: &mut Vec<crate::mpd::QueueEntry>,
    cached_flat_albums: &mut Vec<crate::mpd::AlbumMeta>,
    _consecutive_failures: &mut u32,
    stop: &Arc<AtomicBool>,
    metadata_cache: &crate::metadata::MetadataCache,
    cover_tx: &mut Option<CoverThreadSender>,
    cover_target: &ConnectionTarget,
    cover_result_tx: &mpsc::SyncSender<CoverFetchResult>,
    plchanges_count: &mut u32,
) -> bool {
    match cmd {
        MpdCommand::Play => {
            if let Err(e) = adapter.play() { log::error!("Play failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::Pause => {
            if let Err(e) = adapter.pause() { log::error!("Pause failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::Stop => {
            if let Err(e) = adapter.stop() { log::error!("Stop failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::Next => {
            if let Err(e) = adapter.next_track() { log::error!("Next failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::Previous => {
            if let Err(e) = adapter.previous() { log::error!("Previous failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::Seek(pos) => {
            if let Err(e) = adapter.seek(pos) { log::error!("Seek failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::Status => {
            if let Some(update) = fetch_full_update(adapter) {
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
            *last_status = Instant::now();
        }
        MpdCommand::CurrentSong => {
            if let Ok(Some(song)) = adapter.current_song() {
                let mut update = parse_song_update(&song);
                // Merge cached status fields so elapsed/duration/state are present
                if let Ok(status) = adapter.status() {
                    let status_update = parse_status_update(&status);
                    update.state = status_update.state;
                    update.elapsed = status_update.elapsed;
                    update.duration = status_update.duration;
                    update.volume = status_update.volume;
                    update.playlist_version = status_update.playlist_version;
                    update.song = status_update.song;
                    if update.audio_format.is_none() {
                        update.audio_format = status_update.audio_format;
                    }
                }
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
            }
        }
        MpdCommand::ListAlbums => {
            if let Ok(albums) = adapter.list_albums() {
                let _ = event_tx.try_send(MpdEvent::Albums(albums));
            }
        }
        MpdCommand::ListAlbumsGrouped(group) => {
            if cached_flat_albums.is_empty() {
                match adapter.list_albums_full() {
                    Ok(albums) => {
                        log::info!("[MPD] ListAlbumsGrouped: fetched {} albums with full metadata", albums.len());
                        metadata_cache.build(albums.clone());
                        *cached_flat_albums = albums;

                        // Second pass: batch fetch file paths for all albums
                        match adapter.fetch_album_file_paths() {
                            Ok(paths) => {
                                log::info!("[MPD] fetched file paths for {} albums", paths.len());
                                metadata_cache.load_file_paths(paths);
                            }
                            Err(e) => log::warn!("[MPD] file path batch fetch failed: {e}"),
                        }
                    }
                    Err(e) => {
                        log::error!("[MPD] ListAlbumsGrouped: failed to fetch albums: {e}");
                        let _ = event_tx.try_send(MpdEvent::AlbumsGrouped(Vec::new()));
                        return false;
                    }
                }
            } else {
                log::info!("[MPD] ListAlbumsGrouped({group}): using cache ({})", cached_flat_albums.len());
            }
            let groups = adapter.list_albums_grouped(&group, cached_flat_albums);
            let _ = event_tx.try_send(MpdEvent::AlbumsGrouped(groups));
        }
        MpdCommand::FetchCovers(albums) => {
            log::info!("[cover] enqueuing {} albums for cover fetch", albums.len());
            actual_read.enqueue(albums.clone());

            // Lazy-spawn cover thread on first use (story 28-1)
            if cover_tx.is_none() {
                let caps = adapter.capabilities.clone();
                *cover_tx = Some(cover::spawn(
                    cover_target.clone(),
                    caps,
                    cover_result_tx.clone(),
                    stop.clone(),
                ));
            }

            // Resolve URIs on the MPD IO connection, enqueue (artist, album, uri)
            // jobs to the cover thread for binary fetch on its own connection
            let mut jobs = Vec::with_capacity(albums.len());
            for (artist, album) in &albums {
                match adapter.find_album_uris(album) {
                    Ok(uris) => {
                        if let Some(uri) = uris.first() {
                            jobs.push((artist.clone(), album.clone(), uri.clone()));
                        }
                    }
                    Err(e) => {
                        log::info!("[cover] find_album_uris for '{artist}/{album}' failed: {e}");
                    }
                }
            }

            if let Some(ref tx) = *cover_tx {
                tx.enqueue(&jobs);
            }
        }
        MpdCommand::Search(query) => {
            if let Ok(results) = adapter.search_albums(&query) {
                let _ = event_tx.try_send(MpdEvent::SearchResults { results, generation: 0 });
            }
        }
        MpdCommand::SearchFiles(query) => {
            if let Ok(results) = adapter.search_files(&query) {
                let _ = event_tx.try_send(MpdEvent::FileSearchResults(results));
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
            let escaped = path.replace("\\", "\\\\").replace("\"", "\\\"");
            let cmds = vec![
                "clear".to_string(),
                format!("add \"{escaped}\""),
                "play 0".to_string(),
            ];
            if let Err(e) = adapter.send_batch(&cmds) { log::error!("PlayFile failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::ListQueue => {
            sync_queue(adapter, event_tx, local_queue, last_playlist_version, None, plchanges_count);
        }
        MpdCommand::PlayPosition(pos) => {
            if let Err(e) = adapter.send_command(&format!("play {}", pos)) {
                log::error!("PlayPosition failed: {e}");
            }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::DeleteId(id) => {
            if let Err(e) = adapter.send_command(&format!("deleteid {}", id)) {
                log::error!("DeleteId failed: {e}");
            }
            sync_queue(adapter, event_tx, local_queue, last_playlist_version, None, plchanges_count);
        }
        MpdCommand::MoveId(id, to_pos) => {
            if let Err(e) = adapter.send_command(&format!("moveid {} {}", id, to_pos)) {
                log::error!("MoveId failed: {e}");
            }
            sync_queue(adapter, event_tx, local_queue, last_playlist_version, None, plchanges_count);
        }
        MpdCommand::Add(album) => {
            match adapter.find_album_uris(&album) {
                Ok(uris) => {
                    if uris.is_empty() { return false; }
                    let cmds: Vec<String> = uris.iter()
                        .map(|uri| format!("addid \"{}\"", uri.replace("\\", "\\\\").replace("\"", "\\\"")))
                        .collect();
                    if let Err(e) = adapter.send_batch(&cmds) { log::error!("Add album failed: {e}"); }
                    if let Some(update) = fetch_full_update(adapter) {
                        let pv = update.playlist_version.clone();
                        let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
                    }
                    *last_status = Instant::now();
                }
                Err(e) => log::error!("Add album failed: {e}"),
            }
        }
        MpdCommand::AddAt(album, pos) => {
            match adapter.find_album_uris(&album) {
                Ok(uris) => {
                    if uris.is_empty() { return false; }
                    let mut position = pos;
                    let cmds: Vec<String> = uris.iter()
                        .map(|uri| {
                            let cmd = format!("addid \"{}\" {}", uri.replace("\\", "\\\\").replace("\"", "\\\""), position);
                            position += 1;
                            cmd
                        })
                        .collect();
                    if let Err(e) = adapter.send_batch(&cmds) { log::error!("AddAt album failed: {e}"); }
                    if let Some(update) = fetch_full_update(adapter) {
                        let pv = update.playlist_version.clone();
                        let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
                    }
                    *last_status = Instant::now();
                }
                Err(e) => log::error!("AddAt album failed: {e}"),
            }
        }
        MpdCommand::InsertNext(album) => {
            match adapter.find_album_uris(&album) {
                Ok(uris) => {
                    if uris.is_empty() { return false; }
                    let current_pos = adapter.status()
                        .ok()
                        .and_then(|s| s.get("song").cloned())
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(-1);
                    if current_pos < 0 {
                        log::warn!("InsertNext: no current track, skipping insert");
                        let _ = event_tx.try_send(MpdEvent::Error(
                            "Cannot insert after current track: nothing is playing".into()
                        ));
                        return false;
                    }
                    let mut cmds = Vec::with_capacity(uris.len());
                    for (i, uri) in uris.iter().enumerate() {
                        let escaped = uri.replace("\\", "\\\\").replace("\"", "\\\"");
                        let target = current_pos + 1 + i as i32;
                        cmds.push(format!("addid \"{escaped}\" {target}"));
                    }
                    if let Err(e) = adapter.send_batch(&cmds) {
                        log::error!("InsertNext batch failed: {e}");
                    }
                    if let Some(update) = fetch_full_update(adapter) {
                        let pv = update.playlist_version.clone();
                        let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
                    }
                    *last_status = Instant::now();
                }
                Err(e) => log::error!("InsertNext find failed: {e}"),
            }
        }
        MpdCommand::PlayAlbum(album) => {
            match adapter.find_album_uris(&album) {
                Ok(uris) => {
                    let mut cmds = vec!["clear".to_string()];
                    for uri in &uris {
                        let escaped = uri.replace("\\", "\\\\").replace("\"", "\\\"");
                        cmds.push(format!("addid \"{escaped}\""));
                    }
                    cmds.push("play 0".to_string());
                    if let Err(e) = adapter.send_batch(&cmds) { log::error!("PlayAlbum failed: {e}"); }
                    if let Some(update) = fetch_full_update(adapter) {
                        let pv = update.playlist_version.clone();
                        let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
                    }
                    *last_status = Instant::now();
                }
                Err(e) => log::error!("PlayAlbum failed: {e}"),
            }
        }
        MpdCommand::PlayUris(uris) => {
            if uris.is_empty() { return false; }
            let mut cmds: Vec<String> = vec!["clear".to_string()];
            for uri in uris {
                let escaped = uri.replace("\\", "\\\\").replace("\"", "\\\"");
                cmds.push(format!("add \"{escaped}\""));
            }
            cmds.push("play 0".to_string());
            if let Err(e) = adapter.send_batch(&cmds) { log::error!("PlayUris failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::AddUris(uris) => {
            if uris.is_empty() { return false; }
            let cmds: Vec<String> = uris.iter()
                .map(|uri| format!("add \"{}\"", uri.replace("\\", "\\\\").replace("\"", "\\\"")))
                .collect();
            if let Err(e) = adapter.send_batch(&cmds) { log::error!("AddUris failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::InsertNextUris(uris) => {
            if uris.is_empty() { return false; }
            let current_pos = adapter.status()
                .ok()
                .and_then(|s| s.get("song").cloned())
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(-1);
            if current_pos < 0 {
                log::warn!("InsertNextUris: no current track, falling back to AddUris");
                let cmds: Vec<String> = uris.iter()
                    .map(|uri| format!("add \"{}\"", uri.replace("\\", "\\\\").replace("\"", "\\\"")))
                    .collect();
                if let Err(e) = adapter.send_batch(&cmds) { log::error!("InsertNextUris fallback failed: {e}"); }
            } else {
                let mut cmds = Vec::with_capacity(uris.len());
                for (i, uri) in uris.iter().enumerate() {
                    let escaped = uri.replace("\\", "\\\\").replace("\"", "\\\"");
                    cmds.push(format!("addid \"{escaped}\" {}", current_pos + 1 + i as i32));
                }
                if let Err(e) = adapter.send_batch(&cmds) { log::error!("InsertNextUris failed: {e}"); }
            }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::PlayDirectory(dir) => {
            match adapter.lsinfo(&dir) {
                Ok(entries) => {
                    let uris: Vec<String> = entries.iter()
                        .filter_map(|e| match e {
                            DirEntry::File { path, .. } => Some(path.clone()),
                            _ => None,
                        })
                        .collect();
                    if !uris.is_empty() {
                        let mut cmds: Vec<String> = vec!["clear".to_string()];
                        for uri in &uris {
                            cmds.push(format!("add \"{}\"", uri.replace("\\", "\\\\").replace("\"", "\\\"")));
                        }
                        cmds.push("play 0".to_string());
                        if let Err(e) = adapter.send_batch(&cmds) { log::error!("PlayDirectory failed: {e}"); }
                    }
                }
                Err(e) => log::error!("PlayDirectory lsinfo failed: {e}"),
            }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::AddDirectory(dir) => {
            match adapter.lsinfo(&dir) {
                Ok(entries) => {
                    let uris: Vec<String> = entries.iter()
                        .filter_map(|e| match e {
                            DirEntry::File { path, .. } => Some(path.clone()),
                            _ => None,
                        })
                        .collect();
                    if !uris.is_empty() {
                        let cmds: Vec<String> = uris.iter()
                            .map(|uri| format!("add \"{}\"", uri.replace("\\", "\\\\").replace("\"", "\\\"")))
                            .collect();
                        if let Err(e) = adapter.send_batch(&cmds) { log::error!("AddDirectory failed: {e}"); }
                    }
                }
                Err(e) => log::error!("AddDirectory lsinfo failed: {e}"),
            }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::InsertNextDirectory(dir) => {
            match adapter.lsinfo(&dir) {
                Ok(entries) => {
                    let uris: Vec<String> = entries.iter()
                        .filter_map(|e| match e {
                            DirEntry::File { path, .. } => Some(path.clone()),
                            _ => None,
                        })
                        .collect();
                    if !uris.is_empty() {
                        let current_pos = adapter.status()
                            .ok()
                            .and_then(|s| s.get("song").cloned())
                            .and_then(|s| s.parse::<i32>().ok())
                            .unwrap_or(-1);
                        let mut cmds = Vec::with_capacity(uris.len());
                        if current_pos < 0 {
                            for uri in &uris {
                                cmds.push(format!("add \"{}\"", uri.replace("\\", "\\\\").replace("\"", "\\\"")));
                            }
                        } else {
                            for (i, uri) in uris.iter().enumerate() {
                                let escaped = uri.replace("\\", "\\\\").replace("\"", "\\\"");
                                cmds.push(format!("addid \"{escaped}\" {}", current_pos + 1 + i as i32));
                            }
                        }
                        if let Err(e) = adapter.send_batch(&cmds) { log::error!("InsertNextDirectory failed: {e}"); }
                    }
                }
                Err(e) => log::error!("InsertNextDirectory lsinfo failed: {e}"),
            }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::Clear => {
            if let Err(e) = adapter.send_command("clear") { log::error!("Clear failed: {e}"); }
            if let Some(update) = fetch_full_update(adapter) {
                let pv = update.playlist_version.clone();
                let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
            }
            *last_status = Instant::now();
        }
        MpdCommand::Reconnect => {
            log::info!("[MPD] received Reconnect command, restarting connection");
            return true;
        }
        MpdCommand::Update => {
            if let Err(e) = adapter.update_library() {
                log::error!("Update failed: {e}");
            } else {
                log::info!("[MPD] library update triggered, refreshing grid");
                let _ = event_tx.try_send(MpdEvent::LibraryChanged);
            }
        }
        MpdCommand::Batch(commands) => {
            if commands.is_empty() {
                return false;
            }
            let mut strings: Vec<String> = Vec::with_capacity(commands.len());
            for cmd in &commands {
                let s = command_to_mpd_strings(cmd);
                if s.is_empty() {
                    log::error!("[MPD] Batch: unsupported sub-command {:?}", cmd);
                    let _ = event_tx.try_send(MpdEvent::Toast {
                        message: format!("Batch error: unsupported sub-command"),
                        level: crate::mpd::state_machine::ToastLevel::Error,
                    });
                    return false;
                }
                strings.extend(s);
            }
            match adapter.send_batch(&strings) {
                Ok(_) => {
                    if let Some(update) = fetch_full_update(adapter) {
                        let pv = update.playlist_version.clone();
                        let _ = event_tx.try_send(MpdEvent::StateChanged(update));
                        sync_queue(adapter, event_tx, local_queue, last_playlist_version, pv.as_deref(), plchanges_count);
                    }
                    *last_status = Instant::now();
                }
                Err(e) => {
                    log::error!("[MPD] Batch failed: {e}");
                    let _ = event_tx.try_send(MpdEvent::Toast {
                        message: format!("Batch operation failed: {e}"),
                        level: crate::mpd::state_machine::ToastLevel::Error,
                    });
                }
            }
        }
        MpdCommand::Close => {
            log::info!("[MPD] received Close command, sending close to MPD");
            let _ = adapter.send_command("close");
            stop.store(true, Ordering::Release);
            return true;
        }
    }
    false
}


/// Convert an MpdCommand variant to its MPD protocol command string(s).
/// Returns an empty Vec for variants that don't map to a single command line
/// (e.g., PlayAlbum which requires find_album_uris lookup).
fn command_to_mpd_strings(cmd: &MpdCommand) -> Vec<String> {
    match cmd {
        MpdCommand::Play => vec!["play".into()],
        MpdCommand::Pause => vec!["pause".into()],
        MpdCommand::Stop => vec!["stop".into()],
        MpdCommand::Next => vec!["next".into()],
        MpdCommand::Previous => vec!["previous".into()],
        MpdCommand::Seek(pos) => vec![format!("seekcur {pos}")],
        MpdCommand::Clear => vec!["clear".into()],
        MpdCommand::Update => vec!["update".into()],
        MpdCommand::PlayPosition(pos) => vec![format!("play {pos}")],
        MpdCommand::DeleteId(id) => vec![format!("deleteid {id}")],
        MpdCommand::MoveId(id, to_pos) => vec![format!("moveid {id} {to_pos}")],
        // Complex commands that require URI lookup or directory listing —
        // caller should use the dedicated single-command variants instead.
        MpdCommand::PlayAlbum(_)
        | MpdCommand::PlayUris(_)
        | MpdCommand::Add(_)
        | MpdCommand::AddAt(..)
        | MpdCommand::AddUris(_)
        | MpdCommand::InsertNext(_)
        | MpdCommand::InsertNextUris(_)
        | MpdCommand::PlayDirectory(_)
        | MpdCommand::AddDirectory(_)
        | MpdCommand::InsertNextDirectory(_)
        | MpdCommand::Search(_)
        | MpdCommand::SearchFiles(_)
        | MpdCommand::ListDirectory(_)
        | MpdCommand::PlayFile(_)
        | MpdCommand::Status
        | MpdCommand::CurrentSong
        | MpdCommand::ListAlbums
        | MpdCommand::ListAlbumsGrouped(_)
        | MpdCommand::ListQueue
        | MpdCommand::ListAlbumTracks(_)
        | MpdCommand::FetchCovers(_)
        | MpdCommand::Reconnect
        | MpdCommand::Close
        | MpdCommand::Batch(_) => vec![],
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
    plchanges_count: &mut u32,
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

    // Determine whether to use incremental plchanges or full sync
    let use_incremental = last_version.is_some()
        && *plchanges_count < 50
        && !version_wrapped(last_version.as_deref(), Some(ver.as_str()));

    if use_incremental {
        match try_incremental_sync(adapter, local_queue, last_version.as_deref()) {
            Ok(()) => {
                *plchanges_count += 1;
                *last_version = Some(ver.clone());
                let _ = event_tx.try_send(MpdEvent::Queue(local_queue.clone()));
                return;
            }
            Err(e) => {
                log::warn!("[MPD] plchanges failed ({e}), falling back to full sync");
            }
        }
    }

    // Full sync
    match adapter.list_queue() {
        Ok(queue) => {
            *local_queue = queue;
            *last_version = Some(ver.clone());
            *plchanges_count = 0;
            let _ = event_tx.try_send(MpdEvent::Queue(local_queue.clone()));
        }
        Err(e) => log::error!("queue sync failed: {e}"),
    }
}

/// Detect playlist version counter wrap (32-bit counter, delta > 1M).
fn version_wrapped(old: Option<&str>, new: Option<&str>) -> bool {
    let (Some(old_str), Some(new_str)) = (old, new) else { return false };
    let (Ok(old_num), Ok(new_num)) = (old_str.parse::<u64>(), new_str.parse::<u64>()) else { return false };
    // A large backward jump means the 32-bit counter wrapped
    new_num < old_num && old_num - new_num > 1_000_000
}

/// Attempt incremental queue sync via `plchanges`.
/// Merges added/changed entries into `local_queue` and removes entries
/// at positions beyond the current playlist length (deletions).
fn try_incremental_sync(
    adapter: &mut MpdAdapter,
    local_queue: &mut Vec<crate::mpd::QueueEntry>,
    last_version: Option<&str>,
) -> Result<(), crate::mpd::Error> {
    let version = last_version.ok_or_else(|| crate::mpd::Error::Protocol("no previous version".into()))?;
    let changes = adapter.plchanges(version)?;
    let playlist_len = adapter.status()
        .ok()
        .and_then(|s| s.get("playlistlength").cloned())
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    // Merge changes into local queue by position.
    // Extend with empty placeholders if a position is beyond the current length
    // (entries were added by another client beyond our local tail).
    for entry in changes {
        let pos = entry.position as usize;
        if pos >= local_queue.len() {
            local_queue.resize(pos + 1, crate::mpd::QueueEntry {
                position: 0, id: 0, title: None, artist: None,
                album: None, duration: None, file: String::new(),
            });
        }
        local_queue[pos] = entry;
    }

    // Truncate to playlist length (handles deletions from end)
    if local_queue.len() > playlist_len {
        local_queue.truncate(playlist_len);
    }

    // Re-index positions
    for (i, entry) in local_queue.iter_mut().enumerate() {
        entry.position = i as i32;
    }

    Ok(())
}

/// Group a flat (artist, album) list by artist name, sorted alphabetically.
fn parse_status_update(status: &std::collections::HashMap<String, String>) -> PlaybackUpdate {
    // Extract format badge from status "audio" field (e.g., "44100:24:2" → "24/44.1")
    let format = status.get("audio").and_then(|audio| {
        let first = audio.split(':').next().unwrap_or(audio);
        if first.eq_ignore_ascii_case("dsd64") {
            Some("DSD64".into())
        } else if first.eq_ignore_ascii_case("dsd128") {
            Some("DSD128".into())
        } else if first.eq_ignore_ascii_case("dsd256") {
            Some("DSD256".into())
        } else if first.eq_ignore_ascii_case("dsd512") {
            Some("DSD512".into())
        } else {
            // PCM: parse the full "44100:24:2" string
            let parts: Vec<&str> = audio.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(bits) = parts[1].parse::<u32>() {
                    let rate_str = if let Ok(r) = parts[0].parse::<f64>() {
                        format!("{:.1}", r / 1000.0).trim_end_matches('0').trim_end_matches('.').to_string()
                    } else { parts[0].to_string() };
                    return Some(format!("{}/{}", bits, rate_str));
                }
            }
            None
        }
    });

    PlaybackUpdate {
        state: status.get("state").cloned().unwrap_or_default(),
        song: status.get("song").and_then(|v| v.parse().ok()),
        artist: None,
        title: None,
        album: None,
        year: None,
        volume: status.get("volume")
            .and_then(|v| v.parse::<i16>().ok())
            .map(|v| if v == -1 { 0 } else { v })
            .unwrap_or(0),
        elapsed: status.get("elapsed").and_then(|v| v.parse().ok()),
        duration: status.get("duration").and_then(|v| v.parse().ok()),
        playlist_version: status.get("playlist").cloned(),
        format,
        file: None,
        bitrate: status.get("bitrate").cloned(),
        audio_format: parse_mpd_audio_format(None, None, status.get("audio").map(|s| s.as_str())),
    }
}

fn parse_song_update(song: &std::collections::HashMap<String, String>) -> PlaybackUpdate {
    let format = format_badge_text(song);
    let year = song.get("Date").map(|d| {
        d.split('-').next().unwrap_or(d).to_string()
    });
    PlaybackUpdate {
        artist: song.get("Artist").cloned(),
        title: song.get("Title").cloned(),
        album: song.get("Album").cloned(),
        year,
        format,
        file: song.get("file").cloned(),
        audio_format: parse_mpd_audio_format(
            song.get("Audio").map(|s| s.as_str()),
            song.get("Format").map(|s| s.as_str()),
            None,
        ),
        ..Default::default()
    }
}

/// Build a compact format badge from MPD currentsong audio metadata.
fn format_badge_text(song: &std::collections::HashMap<String, String>) -> Option<String> {
    crate::presenters::format::format_badge(song)
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
            if update.year.is_none() { update.year = song_update.year; }
            if update.format.is_none() { update.format = song_update.format; }
            if update.file.is_none() { update.file = song_update.file; }
            if update.audio_format.is_none() { update.audio_format = song_update.audio_format; }
        }
    }
    Some(update)
}
