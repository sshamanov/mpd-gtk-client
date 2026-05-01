//! Rotating file logger — custom `log::Log` implementation.
//!
//! Replaces `env_logger`. Writes to both stderr (with env_logger-style formatting)
//! and a rotating file at `~/.local/share/mpd-client/log/mpd-client.log`.
//! Rotation: max 5MB per file, keep 3 rotated archives.
//!
//! Thread: Sync (all state behind Mutex).

use log::{LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;
const MAX_ROTATED_FILES: u32 = 3;

fn log_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("mpd-client")
        .join("log")
}

struct FileLogger {
    inner: Mutex<FileLoggerInner>,
}

struct FileLoggerInner {
    file: Option<File>,
    path: PathBuf,
    stderr: io::Stderr,
    filter: LevelFilter,
}

impl FileLogger {
    fn new(filter: LevelFilter) -> Option<Self> {
        let dir = log_dir();
        if fs::create_dir_all(&dir).is_err() {
            return None;
        }
        let path = dir.join("mpd-client.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Some(Self {
            inner: Mutex::new(FileLoggerInner {
                file,
                path,
                stderr: io::stderr(),
                filter,
            }),
        })
    }

    fn format(record: &Record) -> String {
        let level = record.level();
        let target = record.target();
        let args = record.args();
        chrono_prefix().map_or_else(
            || format!("[{level:<5}] {target} — {args}"),
            |ts| format!("{ts} [{level:<5}] {target} — {args}"),
        )
    }
}

fn chrono_prefix() -> Option<String> {
    // Use std::time for a simple UTC timestamp (no chrono dependency)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    let secs = now.as_secs();
    // Simple breakdown
    let (y, m, d, hh, mm, ss) = seconds_to_ymd_hms(secs);
    Some(format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z"))
}

fn seconds_to_ymd_hms(total_secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = total_secs / 86400;
    let time_secs = total_secs % 86400;
    let hh = time_secs / 3600;
    let mm = (time_secs % 3600) / 60;
    let ss = time_secs % 60;

    // Days to year/month/day (civil date)
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0u64;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md {
            m = (i + 1) as u64;
            break;
        }
        remaining -= md;
    }
    if m == 0 {
        m = 12;
    }
    (y as u64, m, (remaining + 1) as u64, hh, mm, ss)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.inner.lock().unwrap().filter
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = Self::format(record);
        let line_with_nl = format!("{line}\n");

        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;

        // Write to stderr
        let _ = write!(inner.stderr, "{line_with_nl}");

        // Write to file with rotation
        if inner.file.is_some() {
            let should_rotate = inner.file.as_ref().and_then(|f| f.metadata().ok())
                .map(|m| m.len() >= MAX_FILE_SIZE)
                .unwrap_or(false);

            if should_rotate {
                // Close file and rotate
                inner.file = None;
                rotate_files(&inner.path);
                inner.file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&inner.path)
                    .ok();
            }

            if let Some(ref mut file) = inner.file {
                let _ = write!(file, "{line_with_nl}");
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(ref mut file) = guard.file {
                let _ = file.flush();
            }
        }
    }
}

fn rotate_files(path: &PathBuf) {
    for i in (1..=MAX_ROTATED_FILES).rev() {
        let src = path.with_file_name(format!("mpd-client.{i}.log"));
        if i == MAX_ROTATED_FILES {
            let _ = fs::remove_file(&src);
        } else {
            let dst = path.with_file_name(format!("mpd-client.{}.log", i + 1));
            let _ = fs::rename(&src, &dst);
        }
    }
    let rotated = path.with_file_name("mpd-client.1.log");
    let _ = fs::rename(path, &rotated);
}

/// Initialize logging with rotating file output.
///
/// Replaces `env_logger::init()`. Reads `RUST_LOG` env var for level filtering
/// (defaults to "info"). Logs to both stderr and a rotating file.
pub fn init() {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);

    if let Some(logger) = FileLogger::new(filter) {
        if let Err(e) = log::set_boxed_logger(Box::new(logger)) {
            // Can't happen normally since we replace env_logger entirely
            eprintln!("Warning: failed to set logger: {e}");
        }
        log::set_max_level(filter);
    } else {
        // Fallback to env_logger
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info"),
        )
        .init();
    }
}
