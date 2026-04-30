//! Application entry point — initializes env_logger, creates state, starts GTK main loop. Thread: UI (startup then GTK main loop).

pub mod config;
pub mod constants;
pub mod coverart;
pub mod errors;
pub mod mpd;
pub mod search;
pub mod state;
pub mod ui;

use log::info;
use mpd::state_machine::{MpdCommand, MpdEvent, MpdEventLoop};
use state::create_initial_state;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use ui::App;

/// Set by SIGINT/SIGTERM signal handlers to request a graceful GTK main loop exit.
pub(crate) static SHUTDOWN_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() {
    env_logger::init();
    info!("Starting MPD client");

    let config = config::Config::load();
    let state = create_initial_state();

    // Shared host/port for live reconnect (Settings writes, background thread reads)
    let conn_params: Arc<Mutex<(String, u16)>> = Arc::new(Mutex::new((
        config.mpd_host.clone(),
        config.mpd_port,
    )));

    // Create bounded MPD event channel (backpressure: drop events when UI is busy)
    let (event_tx, event_rx) = std::sync::mpsc::sync_channel::<MpdEvent>(1024);
    let (event_loop, cmd_tx) = MpdEventLoop::spawn(
        config.mpd_host.clone(),
        config.mpd_port,
        event_tx,
        conn_params.clone(),
    );

    info!("MPD event loop started");

    // Signal handlers are no longer registered directly (glib::source::unix_signal_add
    // was removed in glib 0.22). The SHUTDOWN_REQUESTED flag is set by Ctrl+Q
    // (registered in App::run). On window close, the GTK main loop exits normally
    // and the shutdown sequence below runs.

    // Block until the GTK application exits
    let close_tx = cmd_tx.clone();
    let app = App::new(state, event_rx, cmd_tx, conn_params);
    app.run();

    info!("Shutting down MPD connection");

    // Send the MPD close command for graceful shutdown before stopping the thread.
    // This ensures MPD sees a clean disconnect rather than an abrupt TCP close.
    if close_tx.send(MpdCommand::Close).is_ok() {
        thread::sleep(Duration::from_millis(100));
    } else {
        log::warn!("MPD command channel already closed during shutdown");
    }

    event_loop.signal_stop();

    // Attempt join with 3-second timeout
    let handle = event_loop.into_handle();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if handle.is_finished() {
            break;
        }
        if Instant::now() >= deadline {
            eprintln!("MPD event loop did not exit within 3s, aborting shutdown");
            break;
        }
        thread::park_timeout(Duration::from_millis(100));
    }
    info!("Exiting");
}
