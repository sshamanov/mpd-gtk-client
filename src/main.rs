//! Application entry point — initializes env_logger, creates state, starts GTK main loop. Thread: UI (startup then GTK main loop).

mod app;
mod config;
pub mod constants;
pub mod coverart;
pub mod errors;
pub mod mpd;
pub mod presenters;
pub mod search;
pub mod state;
pub mod ui;
pub mod utils;

use log::info;
use mpd::state_machine::{MpdEvent, MpdEventLoop};
use state::create_initial_state;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use ui::App;

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

    // Register SIGINT (2) / SIGTERM (15) handlers
    // Use idle_add to defer exit to the GLib main context, allowing clean shutdown
    let sig_stop = event_loop.get_stop_flag();
    glib::source::unix_signal_add(2, move || {
        info!("Received SIGINT, shutting down");
        sig_stop.store(true, Ordering::Release);
        glib::idle_add(|| std::process::exit(0));
        glib::ControlFlow::Break
    });
    let sig_stop2 = event_loop.get_stop_flag();
    glib::source::unix_signal_add(15, move || {
        info!("Received SIGTERM, shutting down");
        sig_stop2.store(true, Ordering::Release);
        glib::idle_add(|| std::process::exit(0));
        glib::ControlFlow::Break
    });

    // Block until the GTK application exits
    let app = App::new(state, event_rx, cmd_tx, conn_params);
    app.run();

    info!("Shutting down MPD connection");
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
