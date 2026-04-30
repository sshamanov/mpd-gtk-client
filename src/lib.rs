pub mod mpd;
pub mod state;
pub mod ui;
pub mod coverart;
pub mod search;
pub mod config;
pub mod errors;
pub mod constants;

use std::sync::atomic::AtomicBool;

/// Set by SIGINT/SIGTERM signal handlers to request a graceful GTK main loop exit.
/// The frame clock event-processing callback in the UI layer checks this flag and calls `app.quit()`.
pub(crate) static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
