//! Desktop notification integration via `org.freedesktop.Notifications` D-Bus interface.
//!
//! Uses a background thread that polls the shared connection state and sends
//! D-Bus notifications on state transitions. Optionally reuses the zbus
//! connection from MPRIS (story 14-2) or creates its own.
//!
//! Opt-in only — `[notifications] libnotify = false` by default.

#![cfg(feature = "mpris")]

use crate::state::{ConnectionState, SharedState};
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::zvariant::Value;

/// How often to poll the connection state (seconds).
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// How long to wait before re-showing a "disconnected" notification after it was dismissed.
const DISCONNECTED_COOLDOWN: Duration = Duration::from_secs(30);

/// Start the notification service on a background thread.
///
/// Spawns a thread that monitors `SharedState.connection` for changes and
/// sends libnotify desktop notifications when the state transitions.
///
/// The service is a no-op if `enabled` is false or if no D-Bus session bus
/// is available. Errors are logged at debug level (never fatal).
pub fn spawn(state: SharedState, enabled: bool) -> Option<thread::JoinHandle<()>> {
    if !enabled {
        return None;
    }

    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Notifications: cannot connect to D-Bus session bus: {e}");
            return None;
        }
    };

    Some(thread::Builder::new()
        .name("notifications".into())
        .spawn(move || run(state, conn))
        .expect("Failed to spawn notification thread"))
}

fn run(state: SharedState, conn: Connection) {
    let mut prev_state = ConnectionState::Connected;
    // Track the notification ID returned by the D-Bus daemon so we can replace
    // "disconnected" with "reconnected" instead of stacking notifications.
    let mut notif_id: u32 = 0;
    let mut last_disconnected: Option<Instant> = None;

    loop {
        thread::sleep(POLL_INTERVAL);

        let current = match state.read() {
            Ok(s) => s.connection.clone(),
            Err(_) => continue,
        };

        // Detect state transitions
        match (&prev_state, &current) {
            (_, ConnectionState::Disconnected) => {
                if last_disconnected.map_or(true, |t| t.elapsed() >= DISCONNECTED_COOLDOWN) {
                    notif_id = send_notification(
                        &conn,
                        notif_id,
                        "MPD Disconnected",
                        "Connection lost — retrying...",
                    );
                    last_disconnected = Some(Instant::now());
                }
            }
            (ConnectionState::Disconnected, ConnectionState::Connected) => {
                notif_id = send_notification(
                    &conn,
                    notif_id,
                    "MPD Reconnected",
                    "Connection restored.",
                );
                last_disconnected = None;
            }
            (ConnectionState::Disconnected, ConnectionState::Error(_)) => {
                notif_id = send_notification(
                    &conn,
                    notif_id,
                    "MPD Connection Failed",
                    "Check your MPD server and settings.",
                );
            }
            _ => {}
        }

        prev_state = current;
    }
}

/// Send (or replace) a desktop notification via `org.freedesktop.Notifications`.
///
/// Returns the notification ID assigned by the notification daemon.
fn send_notification(conn: &Connection, replaces_id: u32, summary: &str, body: &str) -> u32 {
    // org.freedesktop.Notifications.Notify signature:
    //   app_name: String
    //   replaces_id: u32
    //   app_icon: String
    //   summary: String
    //   body: String
    //   actions: Vec<String>
    //   hints: Dict<String, Variant>
    //   expire_timeout: i32  (-1 = default, 0 = never, >0 = ms)
    let expires: i32 = if summary.contains("Disconnected") || summary.contains("Failed") {
        0 // persistent until dismissed
    } else {
        5000 // 5 seconds
    };

    let args: &[Value] = &[
        Value::new("mpd-client"),
        Value::new(replaces_id),
        Value::new(""),
        Value::new(summary),
        Value::new(body),
        Value::new(Vec::<String>::new()),
        Value::new(HashMap::<&str, Value>::new()),
        Value::new(expires),
    ];

    match conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &args,
    ) {
        Ok(msg) => {
            msg.body()
                .deserialize::<u32>()
                .unwrap_or(replaces_id)
        }
        Err(e) => {
            log::debug!("Notifications: D-Bus call failed: {e}");
            replaces_id
        }
    }
}
