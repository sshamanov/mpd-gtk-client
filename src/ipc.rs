//! Second-instance detection — lock file and Unix socket IPC.
//!
//! - **First instance:** atomically creates a lock file with PID, starts a Unix socket listener.
//! - **Second instance:** lock creation fails (O_EXCL), forwards CLI action to the
//!   running instance via the Unix socket, then exits.
//! - **Stale lock:** PID is dead or process name doesn't match → overwrite and proceed.
//!
//! Thread: socket listener runs on a dedicated background thread with non-blocking accept.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::mpd::state_machine::MpdCommand;

/// Directory for IPC artifacts: `~/.cache/mpd-client/`
fn ipc_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("mpd-client")
}

/// Lock file path: `~/.cache/mpd-client/lock`
fn lock_file_path() -> PathBuf {
    ipc_dir().join("lock")
}

/// Unix socket path: `~/.cache/mpd-client/socket`
fn socket_path() -> PathBuf {
    ipc_dir().join("socket")
}

/// Outcome of an attempt to acquire the singleton lock.
pub enum LockOutcome {
    /// This instance holds the lock and should proceed as the primary instance.
    Acquired,
    /// Another instance with a live PID + matching process name is already running.
    AnotherInstanceRunning,
}

/// Atomically acquire the singleton lock.
///
/// Uses `O_CREAT | O_EXCL` to prevent the TOCTOU race between checking and creating.
/// If the lock exists but the owning PID is dead or name doesn't match, the stale
/// lock is replaced and we retry once.
pub fn try_acquire_lock() -> Result<LockOutcome, String> {
    let path = lock_file_path();
    let dir = ipc_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create IPC dir: {e}"))?;

    // Atomic create — fails with AlreadyExists if lock is held
    let res = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path);

    match res {
        Ok(mut file) => {
            use std::io::Write;
            write!(file, "{}\n", std::process::id())
                .map_err(|e| format!("Cannot write lock file: {e}"))?;
            Ok(LockOutcome::Acquired)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Lock exists — check if PID is alive and is actually mpd-client
            let stale = lock_is_stale();
            if stale {
                log::warn!("Stale lock file detected (PID {} dead or not mpd-client), overwriting", read_lock_pid().unwrap_or(0));
                // Stale — remove and retry once
                let _ = std::fs::remove_file(&path);
                let mut file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(|e| format!("Cannot acquire lock after stale cleanup: {e}"))?;
                write!(file, "{}\n", std::process::id())
                    .map_err(|e| format!("Cannot write lock file: {e}"))?;
                Ok(LockOutcome::Acquired)
            } else {
                Ok(LockOutcome::AnotherInstanceRunning)
            }
        }
        Err(e) => Err(format!("Cannot create lock file: {e}")),
    }
}

/// Returns `true` if the existing lock file is stale (PID dead, name mismatch, or /proc unavailable).
fn lock_is_stale() -> bool {
    let pid = match read_lock_pid() {
        Some(pid) => pid,
        None => return true, // corrupt lock file
    };
    if pid == std::process::id() {
        return false; // our own lock (racing with ourselves)
    }
    !pid_is_alive_mpd_client(pid)
}

/// Read the PID from the lock file, if it exists and is valid.
fn read_lock_pid() -> Option<u32> {
    let content = std::fs::read_to_string(lock_file_path()).ok()?;
    content.trim().parse().ok()
}

/// Check whether the given PID is alive AND running `mpd-client` specifically.
///
/// Reads `/proc/<pid>/status` to verify both existence and process name.
/// If `/proc` is unavailable (containers), falls back to treating the lock as stale.
fn pid_is_alive_mpd_client(pid: u32) -> bool {
    let status_path_str = format!("/proc/{pid}/status");
    let status_path = std::path::Path::new(&status_path_str);
    let content = match std::fs::read_to_string(status_path) {
        Ok(c) => c,
        Err(_) => return false, // PID dead or /proc unavailable
    };

    // Extract the `Name:` line — first line of /proc/<pid>/status
    // Format: "Name:\tmpd-client\n"
    for line in content.lines() {
        if let Some(name) = line.strip_prefix("Name:\t") {
            return name == "mpd-client";
        }
    }
    false
}

/// Turn a CLI action `MpdCommand` into an IPC action string.
pub fn action_to_string(cmd: &MpdCommand) -> Option<&'static str> {
    match cmd {
        MpdCommand::Play => Some("start-playing"),
        MpdCommand::Pause => Some("toggle-playback"),
        MpdCommand::Next => Some("next"),
        MpdCommand::Previous => Some("prev"),
        _ => None,
    }
}

/// Parse an IPC action string into an `MpdCommand`.
fn string_to_action(s: &str) -> Option<MpdCommand> {
    match s.trim() {
        "start-playing" => Some(MpdCommand::Play),
        "toggle-playback" => Some(MpdCommand::Pause),
        "next" => Some(MpdCommand::Next),
        "prev" => Some(MpdCommand::Previous),
        _ => None,
    }
}

/// Start the Unix socket listener on the calling thread.
///
/// Binds to `~/.cache/mpd-client/socket`, accepts one connection at a time,
/// reads one action line, dispatches via `cmd_tx`, and responds `"OK\n"` or
/// `"ERR\n"`. Polls at 100ms with non-blocking accept for clean shutdown.
///
/// The listener stops when `stop` is set to `true`.
pub fn start_listener(
    cmd_tx: crate::mpd::state_machine::CommandSender,
    stop: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path)?;

    let mut stream_buf = [0u8; 128];

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Set read timeout so we don't block on a slow/malicious client
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                match stream.read(&mut stream_buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        let line = String::from_utf8_lossy(&stream_buf[..n]).trim().to_string();
                        if let Some(cmd) = string_to_action(&line) {
                            if cmd_tx.send(cmd).is_ok() {
                                let _ = stream.write_all(b"OK\n");
                            } else {
                                let _ = stream.write_all(b"ERR\n");
                            }
                        } else {
                            let _ = stream.write_all(b"ERR\n");
                        }
                    }
                    Err(_) => {
                        let _ = stream.write_all(b"ERR\n");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                // Log and continue — don't break the listener on transient errors (EINTR, EMFILE, etc.)
                log::warn!("IPC listener accept error (continuing): {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

/// Remove the lock file.
pub fn remove_lock() {
    let _ = std::fs::remove_file(lock_file_path());
}

/// Remove the Unix socket file.
pub fn remove_socket() {
    let _ = std::fs::remove_file(socket_path());
}

/// Forward a CLI action string to the running instance via Unix socket.
///
/// The action string should match CLI flag names without `--` prefix.
/// Returns `true` if the action was sent and acknowledged successfully.
pub fn forward_action(action: &str) -> bool {
    let path = socket_path();
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("IPC: failed to connect to running instance: {e}");
            return false;
        }
    };

    // Set read timeout so the second instance doesn't hang on the response read
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));

    if stream.write_all(format!("{action}\n").as_bytes()).is_err() {
        return false;
    }

    let mut buf = [0u8; 8];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => &buf[..n] == b"OK\n",
        _ => false,
    }
}
