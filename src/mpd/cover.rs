//! MPD Cover thread — separate MPD connection for binary cover data.
//!
//! The MPD IO thread forwards `(artist, album, uri)` tuples here via channel.
//! This thread opens its own MPD TCP/Unix connection, sends `albumart`/`readpicture`,
//! and returns raw binary data via a result channel. The MPD IO thread's socket is
//! never blocked by binary cover transfers.
//!
//! Thread: dedicated background thread (0-1, created on demand).
//!
//! Architecture: architecture.md §2185-2284 (6-thread model)

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crate::mpd::{ConnectionTarget, MpdAdapter, MpdCapabilities};

/// A cover fetch job: `(artist, album_name, file_uri)`.
/// The URI comes pre-resolved from the MPD IO thread (`find_album_uris`).
pub type CoverJob = (String, String, String);

/// Result from a cover fetch by the MPD Cover thread.
pub enum CoverFetchResult {
    /// Raw binary data fetched successfully.
    Success {
        key: String,
        data: Vec<u8>,
        /// Non-None when data came from `readpicture` (carries the mtime timestamp).
        mtime: Option<u64>,
    },
    /// Album has no embedded art (albumart + readpicture both empty).
    Empty {
        key: String,
    },
    /// Both albumart and readpicture failed with errors.
    Error {
        key: String,
        error: String,
    },
}

/// Send a `CoverFetchResult` to the Cover Proc worker, retrying on full channel
/// with shutdown-flag awareness. Logs and drops if disconnected or shutting down.
fn send_result(
    result: CoverFetchResult,
    tx: &mpsc::SyncSender<CoverFetchResult>,
    shutting_down: &Arc<AtomicBool>,
) {
    let mut result = Some(result);
    while let Some(r) = result.take() {
        match tx.try_send(r) {
            Ok(_) => break,
            Err(mpsc::TrySendError::Full(r)) => {
                if shutting_down.load(std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("[mpd-cover] Shutting down, dropping cover result");
                    break;
                }
                result = Some(r);
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                log::error!("[mpd-cover] Cover Proc channel disconnected (worker likely dead), dropping result");
                break;
            }
        }
    }
}

/// Sender handle for enqueuing cover jobs to the MPD Cover thread.
#[derive(Clone)]
pub struct CoverThreadSender {
    tx: mpsc::SyncSender<CoverJob>,
}

impl CoverThreadSender {
    /// Enqueue a batch of cover fetch jobs. Uses `try_send` — if the channel
    /// is full, jobs are silently dropped (backpressure; they'll be re-requested).
    pub fn enqueue(&self, jobs: &[(String, String, String)]) {
        let mut dropped = 0u32;
        for job in jobs {
            match self.tx.try_send(job.clone()) {
                Ok(_) => {}
                Err(mpsc::TrySendError::Full(_)) => {
                    dropped += 1;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    log::warn!("[mpd-cover] Cover thread disconnected, {} jobs lost", jobs.len());
                    break;
                }
            }
        }
        if dropped > 0 {
            log::warn!("[mpd-cover] Channel full: dropped {dropped}/{total} jobs",
                total = jobs.len());
        }
    }
}

/// Spawn the MPD Cover thread.
///
/// Returns a `CoverThreadSender` for enqueuing jobs and receives results via
/// `result_tx`. The thread opens its own MPD connection and fetches binary
/// cover data independently of the MPD IO thread.
///
/// # Idle timeout
/// After 30 seconds with no jobs, the thread closes its connection and terminates.
/// A new thread is spawned (via another `spawn()` call) when the next batch arrives.
///
/// # Shutdown
/// When `shutting_down` is set, the thread exits at the next job boundary.
pub fn spawn(
    target: ConnectionTarget,
    caps: MpdCapabilities,
    result_tx: mpsc::SyncSender<CoverFetchResult>,
    shutting_down: Arc<AtomicBool>,
) -> CoverThreadSender {
    let (job_tx, job_rx) = mpsc::sync_channel::<CoverJob>(512);

    let _ = std::thread::Builder::new()
        .name("mpd-cover".into())
        .spawn(move || {
            let mut adapter: Option<MpdAdapter> = None;

            loop {
                match job_rx.recv_timeout(Duration::from_secs(30)) {
                    Ok((artist, album, uri)) => {
                        if shutting_down.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }

                        let key = crate::coverart::cover_key(&artist, &album);

                        // Lazy-connect MPD on first job
                        if adapter.is_none() {
                            match MpdAdapter::connect(&target) {
                                Ok(a) => adapter = Some(a),
                                Err(e) => {
                                    log::error!("[mpd-cover] Failed to connect: {e}");
                                    send_result(CoverFetchResult::Error {
                                        key,
                                        error: format!("MPD connect failed: {e}"),
                                    }, &result_tx, &shutting_down);
                                    continue;
                                }
                            }
                        }

                        let a = adapter.as_mut().unwrap();

                        // Primary: albumart
                        if caps.albumart {
                            match a.albumart_by_uri(&uri, &album) {
                                Ok(Some(data)) => {
                                    log::info!(
                                        "[mpd-cover] '{key}': albumart returned {} bytes",
                                        data.len()
                                    );
                                    send_result(CoverFetchResult::Success { key, data, mtime: None }, &result_tx, &shutting_down);
                                    continue;
                                }
                                Ok(None) => {
                                    log::info!("[mpd-cover] '{key}': albumart empty, fallback to readpicture");
                                }
                                Err(e) => {
                                    log::info!("[mpd-cover] '{key}': albumart failed ({e}), fallback to readpicture");
                                }
                            }
                        }

                        // Fallback: readpicture
                        if caps.readpicture {
                            match a.readpicture(&uri) {
                                Ok(Some((data, mtime))) => {
                                    log::info!(
                                        "[mpd-cover] '{key}': readpicture returned {} bytes",
                                        data.len()
                                    );
                                    send_result(CoverFetchResult::Success { key, data, mtime: Some(mtime) }, &result_tx, &shutting_down);
                                    continue;
                                }
                                Ok(None) => {
                                    log::info!("[mpd-cover] '{key}': albumart + readpicture both empty");
                                    send_result(CoverFetchResult::Empty { key }, &result_tx, &shutting_down);
                                }
                                Err(e) => {
                                    send_result(CoverFetchResult::Error {
                                        key,
                                        error: format!("{e}"),
                                    }, &result_tx, &shutting_down);
                                }
                            }
                        } else {
                            send_result(CoverFetchResult::Empty { key }, &result_tx, &shutting_down);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        log::info!("[mpd-cover] Idle timeout (30s), shutting down");
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            log::info!("[mpd-cover] Thread terminated");
        });

    CoverThreadSender { tx: job_tx }
}
