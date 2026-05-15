//! Application entry point — CLI parsing, initializes env_logger, creates state, starts GTK main loop. Thread: UI (startup then GTK main loop).

pub mod config;

pub mod coverart;
pub mod errors;
pub mod ipc;
pub mod keybindings;
pub mod logging;
pub mod profiling;
pub mod mpd;
#[cfg(feature = "mpris")]
pub mod mpris;
pub mod notifications;
pub mod metadata;
pub mod presenters;
pub mod search;
pub mod state;
pub mod ui;

use config::CliOverrides;
use log::info;
use mpd::state_machine::{MpdCommand, MpdEvent, MpdEventLoop};
use mpd::ConnectionTarget;
use state::create_initial_state;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use ui::App;

/// Print usage information to stdout.
fn print_usage() {
    println!(
        "\
Usage: mpd-client [OPTIONS]

Session overrides (not persisted):
  --mpd-host <HOST>        MPD server hostname (default: 127.0.0.1)
  --mpd-port <PORT>        MPD server port (default: 6600)
  --profile <NAME>         Connection profile name (reserved, not yet implemented)
  --mode <album|folder>    Startup UI mode (default: album)

Media actions (dispatched after connection):
  --start-playing          Start playback
  --toggle-playback        Toggle play/pause
  --next                   Skip to next track
  --prev                   Skip to previous track

Info:
  --help, -h               Show this help and exit
  --version, -V            Print version and exit\
"
    );
}

/// Parse CLI arguments into overrides and an optional action command.
///
/// Returns `Err(String)` for unrecognized flags or missing values (caller prints to stderr
/// and exits non-zero). Returns `Ok((overrides, action))` on success.
///
/// Accepts an iterator for testability. The public wrapper `parse_cli_args` passes
/// `std::env::args().skip(1)`.
fn parse_args<I>(args: I) -> Result<(CliOverrides, Option<MpdCommand>), String>
where
    I: IntoIterator<Item = String>,
{
    let mut overrides = CliOverrides::default();
    let mut action: Option<MpdCommand> = None;

    let mut args = args.into_iter().peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("mpd-client v{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--mpd-host" => {
                let val = args.next().ok_or_else(|| "--mpd-host requires a value".to_string())?;
                overrides.mpd_host = Some(val);
            }
            "--mpd-port" => {
                let val = args.next().ok_or_else(|| "--mpd-port requires a value".to_string())?;
                let port: u16 = val.parse().map_err(|_| format!("Invalid port value: {val}"))?;
                overrides.mpd_port = Some(port);
            }
            "--profile" => {
                let val = args.next().ok_or_else(|| "--profile requires a value".to_string())?;
                overrides.profile = Some(val);
            }
            "--mode" => {
                let val = args.next().ok_or_else(|| "--mode requires a value".to_string())?;
                match val.as_str() {
                    "album" | "folder" => overrides.mode = Some(val),
                    _ => return Err(format!("Invalid mode: {val}. Expected 'album' or 'folder'")),
                }
            }
            "--start-playing" => {
                if action.is_some() {
                    eprintln!("warning: --start-playing overrides previous action flag");
                }
                action = Some(MpdCommand::Play);
            }
            "--toggle-playback" => {
                if action.is_some() {
                    eprintln!("warning: --toggle-playback overrides previous action flag");
                }
                action = Some(MpdCommand::Pause);
            }
            "--next" => {
                if action.is_some() {
                    eprintln!("warning: --next overrides previous action flag");
                }
                action = Some(MpdCommand::Next);
            }
            "--prev" => {
                if action.is_some() {
                    eprintln!("warning: --prev overrides previous action flag");
                }
                action = Some(MpdCommand::Previous);
            }
            unknown => {
                return Err(format!(
                    "Unknown flag: {unknown}\nTry 'mpd-client --help' for usage."
                ));
            }
        }
    }

    Ok((overrides, action))
}

/// Parse CLI arguments from `std::env::args()`, skipping argv[0].
///
/// Calls `parse_args` internally. Exits via `std::process::exit` for `--help`/`--version`.
fn parse_cli_args() -> Result<(CliOverrides, Option<MpdCommand>), String> {
    parse_args(std::env::args().skip(1))
}

fn main() {
    // Parse CLI args before any other initialization
    let (overrides, action) = match parse_cli_args() {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    logging::init();
    info!("Starting MPD client");

    // Install desktop entry file for application menu integration
    if let Err(e) = config::Config::install_desktop_file() {
        log::warn!("Failed to install desktop entry: {e}");
    }

    // Load config — use with_profile if --profile was passed
    let mut config = if let Some(ref profile) = overrides.profile {
        config::Config::with_profile(profile)
    } else {
        config::Config::load()
    };

    // Apply CLI overrides to config (host, port)
    overrides.apply_to_config(&mut config);

    info!("Config: {}:{}", config.mpd_host, config.mpd_port);

    // --- Second-instance detection (atomic O_EXCL lock, before window creation) ---
    match ipc::try_acquire_lock() {
        Ok(ipc::LockOutcome::Acquired) => {
            info!("IPC lock acquired, starting as primary instance");
        }
        Ok(ipc::LockOutcome::AnotherInstanceRunning) => {
            if let Some(ref cmd) = action {
                if let Some(action_str) = ipc::action_to_string(cmd) {
                    if ipc::forward_action(action_str) {
                        info!("Action '{action_str}' forwarded to running instance, exiting");
                    } else {
                        log::error!("Failed to forward action to running instance");
                    }
                } else {
                    info!("Second instance detected with non-forwardable action, exiting");
                }
            } else {
                info!("Second instance detected, exiting");
            }
            std::process::exit(0);
        }
        Err(e) => {
            log::error!("Failed to acquire IPC lock: {e}");
            std::process::exit(1);
        }
    }

    let state = create_initial_state();

    // Apply mode override (--mode) to initial state
    if let Some(ref mode_str) = overrides.mode {
        if let Ok(mut s) = state.write() {
            s.mode = match mode_str.as_str() {
                "folder" => crate::state::Mode::Folder,
                _ => crate::state::Mode::Album,
            };
        }
    }

    // Shared connection target for live reconnect (Settings writes, background thread reads)
    let conn_params: Arc<Mutex<ConnectionTarget>> = Arc::new(Mutex::new(
        config.connection_target(),
    ));

    // Event channel carries all MPD events including rapid cover updates during scrolling;
    // 1024 slots absorbs the burst from a full grid population (up to ~500 covers with
    // 2 events each). Backpressure: try_send drops events when UI is busy.
    let (event_tx, event_rx) = std::sync::mpsc::sync_channel::<MpdEvent>(1024);
    let (event_loop, cmd_tx, metadata_cache, search_cmd_tx) = MpdEventLoop::spawn(
        event_tx,
        conn_params.clone(),
    );

    info!("MPD event loop started");

    // --- Start IPC socket listener for second-instance forwarding ---
    let ipc_stop = Arc::new(AtomicBool::new(false));
    let ipc_cmd_tx = cmd_tx.clone();
    let ipc_stop_clone = ipc_stop.clone();
    thread::Builder::new()
        .name("ipc-listener".into())
        .spawn(move || {
            if let Err(e) = ipc::start_listener(ipc_cmd_tx, ipc_stop_clone) {
                log::error!("IPC listener failed: {e}");
            }
        })
        .expect("Failed to spawn IPC listener thread");

    // MPRIS update channel (live only when feature is enabled, but the App always holds the sender)
    #[cfg(feature = "mpris")]
    let (mpris_update_tx, mpris_update_rx) = std::sync::mpsc::channel::<mpd::state_machine::PlaybackUpdate>();
    #[cfg(not(feature = "mpris"))]
    let (mpris_update_tx, _) = std::sync::mpsc::channel::<mpd::state_machine::PlaybackUpdate>();

    // Initialize D-Bus services (feature-gated, opt-in via config)
    #[cfg(feature = "mpris")]
    {
        mpris::init(cmd_tx.clone(), state.clone(), config.mpris.enabled, mpris_update_rx);
    }
    #[cfg(not(feature = "mpris"))]
    if config.mpris.enabled {
        log::warn!("MPRIS: enabled in config but not compiled (rebuild with --features mpris)");
    }

    // NotificationRouter — receives cloned MPD events from the GTK thread for
    // desktop notification dispatch via D-Bus (org.freedesktop.Notifications).
    // Skip spawning when mode is Toast (no desktop notifications needed).
    // Toast channel (256): events are rare — connection state changes and user-triggered
    // toasts, at most a few per minute. 256 slots never fills under normal operation;
    // backpressure is not a concern for this channel.
    let (toast_tx, toast_rx) = std::sync::mpsc::sync_channel::<MpdEvent>(256);
    let notif_mode = config.notifications.mode;
    let notif_stop = Arc::new(AtomicBool::new(false));
    let notif_handle: Option<std::thread::JoinHandle<()>> = if notif_mode != config::NotificationMode::Toast {
        Some(notifications::router::spawn(toast_rx, notif_mode, notif_stop.clone()))
    } else {
        drop(toast_rx); // Channel dropped, toast_tx sends become no-ops
        None
    };

    // Signal handlers (SIGINT/SIGTERM) were removed — glib::source::unix_signal_add
    // was dropped in glib 0.22. Ctrl+Q calls app.quit() directly. On window close,
    // the GTK main loop exits normally and the shutdown sequence below runs.

    // Dispatch deferred action commands after event loop is running.
    // Commands sit in the mpsc channel until connected_loop begins processing them,
    // making this robust against connection delays or retries.
    if let Some(cmd) = action {
        info!("Dispatching deferred action command");
        if let Err(e) = cmd_tx.send(cmd) {
            log::error!("Failed to send deferred action command: {e}");
        }
    }

    // Block until the GTK application exits
    let close_tx = cmd_tx.clone();
    let app = App::new(state, event_rx, cmd_tx, conn_params, mpris_update_tx, metadata_cache, search_cmd_tx, toast_tx);
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
    // Clean up IPC artifacts
    ipc_stop.store(true, Ordering::Relaxed);
    notif_stop.store(true, Ordering::Relaxed);
    if let Some(handle) = notif_handle {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if handle.is_finished() {
                break;
            }
            if Instant::now() >= deadline {
                log::warn!("Notification-router thread did not exit within 1s, proceeding with shutdown");
                break;
            }
            thread::park_timeout(Duration::from_millis(100));
        }
    }
    ipc::remove_socket();
    ipc::remove_lock();
    info!("Exiting");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_empty() {
        let (overrides, action) = parse_args(std::iter::empty::<String>()).unwrap();
        assert!(overrides.mpd_host.is_none());
        assert!(overrides.mpd_port.is_none());
        assert!(overrides.profile.is_none());
        assert!(overrides.mode.is_none());
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_args_mpd_host() {
        let (overrides, action) = parse_args(["--mpd-host", "192.168.1.100"].map(String::from)).unwrap();
        assert_eq!(overrides.mpd_host.as_deref(), Some("192.168.1.100"));
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_args_mpd_port() {
        let (overrides, _) = parse_args(["--mpd-port", "6601"].map(String::from)).unwrap();
        assert_eq!(overrides.mpd_port, Some(6601));
    }

    #[test]
    fn test_parse_args_mpd_port_invalid() {
        let err = parse_args(["--mpd-port", "not-a-number"].map(String::from)).unwrap_err();
        assert!(err.contains("Invalid port"));
    }

    #[test]
    fn test_parse_args_mpd_port_out_of_range() {
        let err = parse_args(["--mpd-port", "99999"].map(String::from)).unwrap_err();
        assert!(err.contains("Invalid port"));
    }

    #[test]
    fn test_parse_args_profile() {
        let (overrides, _) = parse_args(["--profile", "home"].map(String::from)).unwrap();
        assert_eq!(overrides.profile.as_deref(), Some("home"));
    }

    #[test]
    fn test_parse_args_mode_album() {
        let (overrides, _) = parse_args(["--mode", "album"].map(String::from)).unwrap();
        assert_eq!(overrides.mode.as_deref(), Some("album"));
    }

    #[test]
    fn test_parse_args_mode_folder() {
        let (overrides, _) = parse_args(["--mode", "folder"].map(String::from)).unwrap();
        assert_eq!(overrides.mode.as_deref(), Some("folder"));
    }

    #[test]
    fn test_parse_args_mode_invalid() {
        let err = parse_args(["--mode", "invalid"].map(String::from)).unwrap_err();
        assert!(err.contains("Invalid mode"));
    }

    #[test]
    fn test_parse_args_start_playing() {
        let (_, action) = parse_args(["--start-playing"].map(String::from)).unwrap();
        assert!(matches!(action, Some(MpdCommand::Play)));
    }

    #[test]
    fn test_parse_args_toggle_playback() {
        let (_, action) = parse_args(["--toggle-playback"].map(String::from)).unwrap();
        assert!(matches!(action, Some(MpdCommand::Pause)));
    }

    #[test]
    fn test_parse_args_next() {
        let (_, action) = parse_args(["--next"].map(String::from)).unwrap();
        assert!(matches!(action, Some(MpdCommand::Next)));
    }

    #[test]
    fn test_parse_args_prev() {
        let (_, action) = parse_args(["--prev"].map(String::from)).unwrap();
        assert!(matches!(action, Some(MpdCommand::Previous)));
    }

    #[test]
    fn test_parse_args_unknown_flag() {
        let err = parse_args(["--bogus"].map(String::from)).unwrap_err();
        assert!(err.contains("Unknown flag"));
    }

    #[test]
    fn test_parse_args_missing_value() {
        let err = parse_args(["--mpd-host"].map(String::from)).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn test_parse_args_missing_mode_value() {
        let err = parse_args(["--mode"].map(String::from)).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn test_parse_args_missing_profile_value() {
        let err = parse_args(["--profile"].map(String::from)).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn test_parse_args_combined_config_overrides() {
        let (overrides, _) = parse_args(
            ["--mpd-host", "10.0.0.1", "--mpd-port", "6600", "--profile", "office"]
                .map(String::from),
        )
        .unwrap();
        assert_eq!(overrides.mpd_host.as_deref(), Some("10.0.0.1"));
        assert_eq!(overrides.mpd_port, Some(6600));
        assert_eq!(overrides.profile.as_deref(), Some("office"));
    }

    #[test]
    fn test_parse_args_combined_action_and_config() {
        let (overrides, action) = parse_args(
            ["--mode", "folder", "--start-playing", "--mpd-host", "localhost"]
                .map(String::from),
        )
        .unwrap();
        assert_eq!(overrides.mode.as_deref(), Some("folder"));
        assert_eq!(overrides.mpd_host.as_deref(), Some("localhost"));
        assert!(matches!(action, Some(MpdCommand::Play)));
    }

    #[test]
    fn test_parse_args_multiple_actions_last_wins() {
        // Multiple action flags — only the last one is kept (since action is overridden)
        let (_, action) = parse_args(
            ["--next", "--start-playing", "--prev"].map(String::from),
        )
        .unwrap();
        assert!(matches!(action, Some(MpdCommand::Previous)));
    }

    #[test]
    fn test_apply_to_config_overrides_host_and_port() {
        let mut cfg = config::Config::default();
        assert_eq!(cfg.mpd_host, "127.0.0.1");
        assert_eq!(cfg.mpd_port, 6600);

        let overrides = CliOverrides {
            mpd_host: Some("10.0.0.1".into()),
            mpd_port: Some(7700),
            profile: None,
            mode: None,
        };
        overrides.apply_to_config(&mut cfg);
        assert_eq!(cfg.mpd_host, "10.0.0.1");
        assert_eq!(cfg.mpd_port, 7700);
    }

    #[test]
    fn test_apply_to_config_partial_override() {
        let mut cfg = config::Config::default();
        let overrides = CliOverrides {
            mpd_host: Some("other".into()),
            mpd_port: None,
            profile: None,
            mode: None,
        };
        overrides.apply_to_config(&mut cfg);
        assert_eq!(cfg.mpd_host, "other");
        // Port should remain default
        assert_eq!(cfg.mpd_port, 6600);
    }

    #[test]
    fn test_apply_to_config_empty_override_does_nothing() {
        let mut cfg = config::Config::default();
        let overrides = CliOverrides::default();
        overrides.apply_to_config(&mut cfg);
        assert_eq!(cfg.mpd_host, "127.0.0.1");
        assert_eq!(cfg.mpd_port, 6600);
    }
}
