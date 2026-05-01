//! MPRIS D-Bus integration — `org.mpris.MediaPlayer2` interface.
//! Feature-gated behind `#[cfg(feature = "mpris")]`.
//!
//! Uses `zbus` blocking API (`default-features = false, features = ["blocking-api", "async-io"]`).
//! The `async-io` runtime is a lightweight epoll-based runtime (no tokio dependency).
//!
//! Thread safety: interface methods receive D-Bus calls on zbus's internal IO thread.
//! `SharedState` (`Arc<RwLock<AppState>>`) is `Send + Sync` — safe to read from any thread.
//! `mpsc::Sender<MpdCommand>` is `Send` — safe to send from any thread.

#![cfg(feature = "mpris")]

use crate::mpd::state_machine::{MpdCommand, PlaybackUpdate};
use crate::state::{AppState, PlaybackState, SharedState};
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use zbus::blocking::Connection;
use zbus::fdo;
use zbus::zvariant::{ObjectPath, Value};

/// Encode a filesystem path to a percent-encoded `file://` URI.
fn file_uri(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    let encoded: String = raw
        .chars()
        .map(|c| match c {
            ' ' => "%20".into(),
            '#' => "%23".into(),
            '%' => "%25".into(),
            '?' => "%3F".into(),
            '[' => "%5B".into(),
            ']' => "%5D".into(),
            '\\' => "%5C".into(),
            '^' => "%5E".into(),
            '`' => "%60".into(),
            '{' => "%7B".into(),
            '|' => "%7C".into(),
            '}' => "%7D".into(),
            '"' => "%22".into(),
            '<' => "%3C".into(),
            '>' => "%3E".into(),
            c if c >= '\x7f' => {
                let mut bytes = [0u8; 4];
                let s = c.encode_utf8(&mut bytes);
                s.bytes().map(|b| format!("%{:02X}", b)).collect::<String>()
            }
            c => c.to_string(),
        })
        .collect();
    format!("file://{}", encoded)
}

// -- Root interface: org.mpris.MediaPlayer2 --

#[allow(dead_code)]
struct MprisRoot {
    cmd_tx: mpsc::Sender<MpdCommand>,
    state: SharedState,
}
impl MprisRoot {
    fn read_state<R>(&self, f: impl FnOnce(&AppState) -> R) -> Option<R> {
        self.state.read().ok().map(|s| f(&s))
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[zbus(property)]
    fn identity(&self) -> &str {
        "mpd-client"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "mpd-client"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<&str> {
        vec!["file"]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<&str> {
        vec![]
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    fn quit(&self) -> fdo::Result<()> {
        Ok(())
    }
}

// -- Player interface: org.mpris.MediaPlayer2.Player --
// Command dispatch helpers (no MpdCommand refs in the zbus interface impl)

struct MprisPlayer {
    cmd_tx: mpsc::Sender<MpdCommand>,
    state: SharedState,
}

impl MprisPlayer {
    fn read_state<R>(&self, f: impl FnOnce(&AppState) -> R) -> Option<R> {
        self.state.read().ok().map(|s| f(&s))
    }

    fn cmd_play(&self) -> fdo::Result<()> {
        self.cmd_tx.send(MpdCommand::Play).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn cmd_pause(&self) -> fdo::Result<()> {
        self.cmd_tx.send(MpdCommand::Pause).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn cmd_stop(&self) -> fdo::Result<()> {
        self.cmd_tx.send(MpdCommand::Stop).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn cmd_next(&self) -> fdo::Result<()> {
        self.cmd_tx.send(MpdCommand::Next).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn cmd_previous(&self) -> fdo::Result<()> {
        self.cmd_tx.send(MpdCommand::Previous).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn cmd_seek(&self, secs: i64) -> fdo::Result<()> {
        self.cmd_tx.send(MpdCommand::Seek(secs)).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn is_playing(&self) -> bool {
        self.read_state(|s| s.playback.state == PlaybackState::Playing)
            .unwrap_or(false)
    }

    fn current_position_us(&self) -> i64 {
        self.read_state(|s| s.playback.current_position as i64 * 1000)
            .unwrap_or(0)
    }

    fn current_track_is(&self, track_id: &str) -> bool {
        self.read_state(|s| s.current.track.is_some())
            .unwrap_or(false)
            && track_id == "/org/mpris/MediaPlayer2/Track/0"
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    // -- Properties --

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.read_state(|s| match s.playback.state {
            PlaybackState::Playing => "Playing",
            PlaybackState::Paused => "Paused",
            PlaybackState::Stopped => "Stopped",
        })
        .unwrap_or("Stopped")
        .to_string()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        "None".to_string()
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, Value<'static>> {
        let mut map: HashMap<String, Value<'static>> = HashMap::new();

        self.read_state(|s| {
            map.insert(
                "mpris:trackid".into(),
                Value::new("/org/mpris/MediaPlayer2/Track/0".to_string()),
            );

            if let Some(ref track) = s.current.track {
                map.insert("xesam:title".into(), Value::new(track.title.clone()));
                map.insert("xesam:url".into(), Value::new(track.path.to_string_lossy().to_string()));

                if let Some(dur) = track.duration {
                    map.insert("mpris:length".into(), Value::new(dur.as_micros() as i64));
                }
            }

            if let Some(ref album) = s.current.album {
                let artist = album.artist.clone();
                let artists: Vec<String> = if artist.is_empty() {
                    vec![]
                } else {
                    vec![artist]
                };
                map.insert("xesam:artist".into(), Value::new(artists));
                map.insert("xesam:album".into(), Value::new(album.title.clone()));

                if let Some(ref cover) = album.cover_path {
                    map.insert("mpris:artUrl".into(), Value::new(file_uri(cover)));
                }
            }
        });

        map
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.read_state(|s| f64::from(s.playback.volume) / 100.0)
            .unwrap_or(0.0)
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.current_position_us()
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    // -- Methods --

    fn next(&self) -> fdo::Result<()> {
        self.cmd_next()
    }

    fn previous(&self) -> fdo::Result<()> {
        self.cmd_previous()
    }

    fn pause(&self) -> fdo::Result<()> {
        self.cmd_pause()
    }

    fn play_pause(&self) -> fdo::Result<()> {
        let cmd = if self.is_playing() {
            self.cmd_pause()
        } else {
            self.cmd_play()
        };
        cmd
    }

    fn stop(&self) -> fdo::Result<()> {
        self.cmd_stop()
    }

    fn play(&self) -> fdo::Result<()> {
        self.cmd_play()
    }

    fn seek(&self, offset_us: i64) -> fdo::Result<()> {
        let target_secs = ((self.current_position_us() + offset_us) / 1_000_000).max(0);
        self.cmd_seek(target_secs)
    }

    fn set_position(
        &self,
        track_id: zbus::zvariant::ObjectPath<'_>,
        position_us: i64,
    ) -> fdo::Result<()> {
        if !self.current_track_is(track_id.as_str()) {
            return Err(fdo::Error::NotSupported(
                "Track ID does not match the currently playing track".into(),
            ));
        }
        self.cmd_seek((position_us / 1_000_000).max(0))
    }

    fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        log::warn!("MPRIS OpenUri not implemented: {_uri}");
        Ok(())
    }
}

// -- Initialization --

/// Initialize MPRIS D-Bus interfaces and start background monitoring.
///
/// Creates a `zbus::blocking::Connection`, registers `org.mpris.MediaPlayer2`
/// and `org.mpris.MediaPlayer2.Player` interfaces, requests the bus name,
/// and spawns threads for PropertiesChanged emission and D-Bus health monitoring.
///
/// The monitoring thread reconnects automatically if the D-Bus session bus
/// restarts (up to 3 retries with backoff).
pub fn init(
    cmd_tx: mpsc::Sender<MpdCommand>,
    state: SharedState,
    enabled: bool,
    update_rx: mpsc::Receiver<PlaybackUpdate>,
) {
    if !enabled {
        return;
    }

    let conn: Connection = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("MPRIS: failed to connect to D-Bus session bus: {e}");
            return;
        }
    };

    if register_interfaces(&conn, &cmd_tx, &state).is_none() {
        return;
    }

    match conn.request_name("org.mpris.MediaPlayer2.mpdclient") {
        Ok(()) => log::info!("MPRIS: registered as org.mpris.MediaPlayer2.mpdclient"),
        Err(e) => {
            log::warn!("MPRIS: failed to request bus name: {e}");
            return;
        }
    }

    // Spawn PropertiesChanged emission thread
    let emit_conn = conn.clone();
    thread::Builder::new()
        .name("mpris-updates".into())
        .spawn(move || emit_loop(emit_conn, update_rx))
        .expect("MPRIS: failed to spawn update listener thread");

    // Spawn D-Bus connection health monitor
    let mon_cmd_tx = cmd_tx.clone();
    let mon_state = state.clone();
    thread::Builder::new()
        .name("mpris-monitor".into())
        .spawn(move || monitor_loop(conn, mon_cmd_tx, mon_state))
        .expect("MPRIS: failed to spawn monitor thread");
}

/// Register MPRIS interfaces on a connection. Returns `Some(())` on success.
fn register_interfaces(
    conn: &Connection,
    cmd_tx: &mpsc::Sender<MpdCommand>,
    state: &SharedState,
) -> Option<()> {
    let root = MprisRoot {
        cmd_tx: cmd_tx.clone(),
        state: state.clone(),
    };
    conn.object_server()
        .at("/org/mpris/MediaPlayer2", root)
        .map_err(|e| log::warn!("MPRIS: failed to register root interface: {e}"))
        .ok()?;

    let player = MprisPlayer {
        cmd_tx: cmd_tx.clone(),
        state: state.clone(),
    };
    conn.object_server()
        .at("/org/mpris/MediaPlayer2", player)
        .map_err(|e| log::warn!("MPRIS: failed to register player interface: {e}"))
        .ok()?;

    Some(())
}

/// Check whether the D-Bus session bus connection is alive by pinging the bus daemon.
fn conn_is_alive(conn: &Connection) -> bool {
    let args = ();
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "Hello",
        &args,
    )
    .is_ok()
}

/// Monitor D-Bus connection health. Reconnects up to 3 times on session bus restart.
fn monitor_loop(mut conn: Connection, cmd_tx: mpsc::Sender<MpdCommand>, state: SharedState) {
    const CHECK_INTERVAL: Duration = Duration::from_secs(5);
    const MAX_RETRIES: u32 = 3;

    loop {
        thread::sleep(CHECK_INTERVAL);

        if conn_is_alive(&conn) {
            continue;
        }

        log::warn!("MPRIS: D-Bus session bus disconnected, attempting reconnection");

        let mut success = false;
        let mut delay = Duration::from_secs(1);

        for attempt in 1..=MAX_RETRIES {
            thread::sleep(delay);

            match zbus::blocking::Connection::session() {
                Ok(new_conn) => {
                    if register_interfaces(&new_conn, &cmd_tx, &state).is_some() {
                        match new_conn.request_name("org.mpris.MediaPlayer2.mpdclient") {
                            Ok(()) => {
                                log::info!("MPRIS: reconnected (attempt {attempt})");
                                conn = new_conn;
                                success = true;
                                break;
                            }
                            Err(e) => {
                                log::warn!("MPRIS: name request failed on reconnect: {e}");
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("MPRIS: reconnect attempt {attempt} failed: {e}");
                }
            }

            delay = delay.saturating_mul(2);
        }

        if !success {
            log::warn!("MPRIS: failed to reconnect after {MAX_RETRIES} attempts");
            // MPRIS is permanently disabled for this session
            return;
        }
    }
}

/// Background loop: receives `PlaybackUpdate` messages and emits
/// `org.freedesktop.DBus.Properties.PropertiesChanged` for `org.mpris.MediaPlayer2.Player`.
fn emit_loop(conn: Connection, rx: mpsc::Receiver<PlaybackUpdate>) {
    let path: ObjectPath<'_> = match "/org/mpris/MediaPlayer2".try_into() {
        Ok(p) => p,
        Err(_) => return,
    };

    while let Ok(update) = rx.recv() {
        let mut changed: Vec<&str> = Vec::new();

        if update.state == "play" || update.state == "pause" || update.state == "stop" {
            changed.push("PlaybackStatus");
        }
        if update.song.is_some() || update.title.is_some() || update.album.is_some() {
            changed.push("Metadata");
        }
        if update.elapsed.is_some() {
            changed.push("Position");
        }

        if !changed.is_empty() {
            // Emit PropertiesChanged signal via zbus blocking API
            // Signal interface: org.freedesktop.DBus.Properties
            // Signal name: PropertiesChanged
            // Signature: (STRING interface, DICT<STRING,VARIANT> changed, ARRAY<STRING> invalidated)
            let changed_properties: HashMap<&str, Value<'_>> = changed
                .iter()
                .map(|&name| (name, Value::new("")))
                .collect();

            let args = (
                "org.mpris.MediaPlayer2.Player",
                changed_properties,
                Vec::<String>::new(),
            );

            match conn.emit_signal(
                None::<&str>,
                &path,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &args,
            ) {
                Ok(()) => log::trace!("MPRIS: PropertiesChanged for {changed:?}"),
                Err(e) => log::debug!("MPRIS: emit_signal failed: {e}"),
            }
        }
    }
}
