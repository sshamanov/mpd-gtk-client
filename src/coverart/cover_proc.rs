//! Cover Proc worker — dedicated thread for JPEG decode, MD5 hash, cache write.
//! Thread: persistent background thread (1, spawned once per MPD connection).
//!
//! Receives CoverFetchResult from the MPD Cover thread via channel, decodes JPEG
//! to RGBA via the `image` crate, resizes to 200×200 (Lanczos3), computes MD5 hash,
//! compares against CoverProvider cache, writes new covers to disk, and emits
//! CoverRefreshed events with raw RGBA bytes for zero-copy GPU upload via
//! `gdk4::MemoryTexture`.
//!
//! Thread: persistent background thread (1 thread).
//!
//! Architecture: architecture.md §2233-2238 (6-thread model)

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use image::imageops::FilterType;
use md5::{Digest, Md5};

use crate::coverart::CoverProvider;
use crate::mpd::cover::CoverFetchResult;
use crate::mpd::state_machine::MpdEvent;

type EventSender = mpsc::SyncSender<MpdEvent>;

/// Spawn the Cover Proc worker thread.
///
/// Receives `CoverFetchResult` from the MPD Cover thread via `job_rx`, decodes JPEG
/// to RGBA via the `image` crate, computes MD5 hash, compares against CoverProvider
/// cache, writes new covers to disk, and emits `CoverRefreshed` events with raw RGBA
/// bytes for zero-copy GPU upload.
///
/// The thread runs until the channel is disconnected or the stop flag is set.
pub fn spawn(
    job_rx: mpsc::Receiver<CoverFetchResult>,
    event_tx: EventSender,
    cover_provider: Arc<RwLock<CoverProvider>>,
    cache_dir: std::path::PathBuf,
    stop: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("cover-proc".into())
        .spawn(move || {
            log::info!("[cover-proc] Thread started");

            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                match job_rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(result) => {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        match result {
                            CoverFetchResult::Success { key, data, mtime } => {
                                process_success(
                                    &key, &data, mtime, &cover_provider,
                                    &cache_dir, &event_tx,
                                );
                            }
                            CoverFetchResult::Empty { key } => {
                                log::debug!("[cover-proc] '{key}': no cover data available");
                            }
                            CoverFetchResult::Error { key, error } => {
                                log::warn!("[cover-proc] '{key}': cover fetch error: {error}");
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            log::info!("[cover-proc] Thread terminated");
        })
        .expect("Failed to spawn cover-proc thread");
}

/// Process a successful cover fetch: MD5 hash, cache compare, decode, resize, emit.
fn process_success(
    key: &str,
    data: &[u8],
    mtime: Option<u64>,
    provider: &Arc<RwLock<CoverProvider>>,
    cache_dir: &Path,
    event_tx: &EventSender,
) {
    let md5 = format!("{:x}", Md5::digest(data));

    // Check CoverProvider cache — if hash matches and timestamp not newer, skip
    if let Ok(ref prov) = provider.read() {
        if let Some(cached) = prov.get(key) {
            if cached.md5 == md5 {
                if mtime.is_none() {
                    log::debug!("[cover-proc] '{key}': hash unchanged, emitting cached path");
                    emit_cover_path(key, &md5, cache_dir, event_tx);
                    return;
                }
                if let Some(cached_ts) = cached.timestamp {
                    if cached_ts >= mtime.unwrap_or(0) {
                        log::debug!(
                            "[cover-proc] '{key}': timestamp not newer (cached: {cached_ts}, mtime: {})",
                            mtime.unwrap_or(0)
                        );
                        return;
                    }
                }
            }
        }
    }

    // New or updated cover — write JPEG to disk cache
    write_cache(key, data, &md5, mtime, cache_dir);

    // Update CoverProvider in-memory index
    if let Ok(ref prov) = provider.read() {
        prov.update_entry(key, &md5, mtime);
    }

    // Emit CoverPaths (path-based delivery for cached covers)
    emit_cover_path(key, &md5, cache_dir, event_tx);

    // Decode JPEG → resize to 200×200 RGBA via image crate
    match decode_and_resize(data) {
        Ok(rgba) => {
            log::info!(
                "[cover-proc] '{key}': decoded {} JPEG bytes → {} RGBA bytes (200×200)",
                data.len(),
                rgba.len()
            );
            let _ = event_tx.try_send(MpdEvent::CoverRefreshed {
                album_id: key.to_string(),
                data: rgba,
            });
        }
        Err(e) => {
            log::warn!("[cover-proc] '{key}': JPEG decode failed ({e}), skipping CoverRefreshed (placeholder fallback)");
            // CoverPaths already emitted above; UI loads cached JPEG from disk.
            // Emitting raw JPEG as CoverRefreshed would corrupt MemoryTexture (expects RGBA).
        }
    }
}

/// Decode JPEG bytes via the `image` crate and resize to 200×200 RGBA (Lanczos3).
fn decode_and_resize(data: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(data).map_err(|e| format!("{e}"))?;
    let rgba = img.to_rgba8();
    let resized = image::imageops::resize(&rgba, 200, 200, FilterType::Lanczos3);
    Ok(resized.into_raw())
}

/// Write cover JPEG data to disk cache (md5-named file) and update index.json.
fn write_cache(
    album_key: &str,
    data: &[u8],
    md5: &str,
    timestamp: Option<u64>,
    cache_dir: &Path,
) {
    let jpeg_path = cache_dir.join(format!("{md5}.jpg"));
    if let Err(e) = fs::write(&jpeg_path, data) {
        log::warn!("[cover-proc] Failed to write cache for '{album_key}': {e}");
    } else {
        log::info!(
            "[cover-proc] Cached cover for '{album_key}' at {:?} ({} bytes)",
            jpeg_path,
            data.len()
        );
    }

    update_index_json(cache_dir, album_key, md5, timestamp);
}

/// Update the index.json sidecar file with a new or updated entry.
///
/// Reads the current index, upserts the entry, writes atomically via temp file + rename.
fn update_index_json(
    cache_dir: &Path,
    album_id: &str,
    md5: &str,
    timestamp: Option<u64>,
) {
    let index_path = cache_dir.join("index.json");
    let tmp_path = cache_dir.join("index.json.tmp");

    let mut index: HashMap<String, serde_json::Value> =
        match fs::read_to_string(&index_path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };

    let entry = match timestamp {
        Some(ts) => serde_json::json!({ "md5": md5, "timestamp": ts }),
        None => serde_json::json!({ "md5": md5 }),
    };
    index.insert(album_id.to_string(), entry);

    match serde_json::to_string(&index) {
        Ok(json) => {
            if let Err(e) = fs::write(&tmp_path, &json) {
                log::warn!("[cover-proc] Failed to write index tmp for '{album_id}': {e}");
                return;
            }
            if let Err(e) = fs::rename(&tmp_path, &index_path) {
                log::warn!("[cover-proc] Failed to rename index for '{album_id}': {e}");
            }
        }
        Err(e) => {
            log::warn!("[cover-proc] Failed to serialize index for '{album_id}': {e}");
        }
    }
}

/// Emit a CoverPaths event for the given album, if the cache file exists.
fn emit_cover_path(
    album_key: &str,
    md5: &str,
    cache_dir: &Path,
    event_tx: &EventSender,
) {
    let jpeg_path = cache_dir.join(format!("{md5}.jpg"));
    let mut covers = HashMap::new();
    if jpeg_path.exists() {
        covers.insert(album_key.to_string(), Some(jpeg_path.to_string_lossy().to_string()));
    } else {
        covers.insert(album_key.to_string(), None);
    }
    let _ = event_tx.try_send(MpdEvent::CoverPaths(covers));
}
