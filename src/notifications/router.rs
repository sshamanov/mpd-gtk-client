//! NotificationRouter — lightweight thread that receives `MpdEvent` events
//! and dispatches desktop notifications via D-Bus `org.freedesktop.Notifications`.
//!
//! Thread: persistent background thread (1, spawned once at app start).
//!
//! Architecture: architecture.md §2246-2251 (6-thread model, NotificationRouter).
//! No GTK imports, no MPD protocol knowledge.
//! Falls back silently to no-op if D-Bus session bus is unavailable or not compiled in.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crate::config::NotificationMode;
use crate::mpd::state_machine::MpdEvent;
#[cfg(feature = "mpris")]
use crate::mpd::state_machine::ToastLevel;

#[cfg(feature = "mpris")]
mod dbus {
    use std::collections::HashMap;
    use std::time::Instant;
    use zbus::blocking::Connection;
    use zbus::zvariant::Value;

    pub struct State {
        pub conn: Connection,
        pub notif_id: u32,
        pub last_disconnected: Option<Instant>,
    }

    pub fn open() -> Option<State> {
        match Connection::session() {
            Ok(conn) => {
                log::debug!("[notification-router] D-Bus session bus connected");
                Some(State {
                    conn,
                    notif_id: 0,
                    last_disconnected: None,
                })
            }
            Err(e) => {
                log::debug!("[notification-router] No D-Bus session bus: {e}");
                None
            }
        }
    }

    pub fn send(
        state: &mut State,
        summary: &str,
        body: &str,
        persistent: bool,
    ) {
        let expires: i32 = if persistent { 0 } else { 5000 };

        let args: &[Value] = &[
            Value::new("mpd-client"),
            Value::new(state.notif_id),
            Value::new(""),
            Value::new(summary),
            Value::new(body),
            Value::new(Vec::<String>::new()),
            Value::new(HashMap::<&str, Value>::new()),
            Value::new(expires),
        ];

        match state.conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &args,
        ) {
            Ok(msg) => {
                state.notif_id = msg.body().deserialize::<u32>().unwrap_or(state.notif_id);
            }
            Err(e) => {
                log::debug!("[notification-router] D-Bus call failed: {e}");
            }
        }
    }
}

/// Spawn the NotificationRouter thread.
///
/// Receives `MpdEvent` from the GTK thread via `rx`, filters for `Toast`,
/// `Connected`, and `Disconnected` events, checks the notification mode,
/// and fires desktop notifications via D-Bus when mode is `Desktop` or `Both`.
///
/// Returns the `JoinHandle` so the caller can join the thread on shutdown.
pub fn spawn(
    rx: mpsc::Receiver<MpdEvent>,
    mode: NotificationMode,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("notification-router".into())
        .spawn(move || {
            log::info!("[notification-router] Thread started (mode: {mode:?})");

            #[cfg(feature = "mpris")]
            let mut dbus = if mode == NotificationMode::Toast {
                None
            } else {
                dbus::open()
            };
            #[cfg(not(feature = "mpris"))]
            let mut dbus: Option<()> = None;

            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(event) => handle_event(event, mode, &mut dbus),
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            log::info!("[notification-router] Thread terminated");
        })
        .expect("Failed to spawn notification-router thread")
}

#[cfg(feature = "mpris")]
fn handle_event(
    event: MpdEvent,
    _mode: NotificationMode,
    dbus: &mut Option<dbus::State>,
) {
    match event {
        MpdEvent::Toast { message, level } => {
            if let Some(d) = dbus {
                let (summary, persistent) = match level {
                    ToastLevel::Error => ("MPD Error", true),
                    ToastLevel::Warn => ("MPD Warning", false),
                    ToastLevel::Info => ("mpd-client", false),
                };
                dbus::send(d, summary, &message, persistent);
            }
        }
        MpdEvent::Disconnected => {
            if let Some(d) = dbus {
                let now = std::time::Instant::now();
                if d.last_disconnected.map_or(true, |t| {
                    t.elapsed() >= Duration::from_secs(30)
                }) {
                    dbus::send(d, "MPD Disconnected", "Connection lost — retrying...", true);
                    d.last_disconnected = Some(now);
                }
            }
        }
        MpdEvent::Connected => {
            if let Some(d) = dbus {
                if d.last_disconnected.is_some() {
                    dbus::send(d, "MPD Reconnected", "Connection restored.", false);
                    d.last_disconnected = None;
                }
            }
        }
        _ => {} // Ignore other event types
    }
}

#[cfg(not(feature = "mpris"))]
fn handle_event(
    _event: MpdEvent,
    _mode: NotificationMode,
    _dbus: &mut Option<()>,
) {
    // no-op: D-Bus not compiled in
}
