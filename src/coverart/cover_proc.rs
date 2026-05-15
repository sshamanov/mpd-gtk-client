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
    running: Arc<AtomicBool>,
) {
    // Atomically acquire the running guard — only one Cover Proc worker at a time.
    if running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        log::warn!("[cover-proc] Another Cover Proc worker is still running, skipping spawn");
        return;
    }
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

            running.store(false, Ordering::Release);
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
    match provider.read() {
        Ok(ref prov) => {
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
        Err(e) => log::error!("[cover-proc] CoverProvider RwLock poisoned (read): {e}"),
    }

    // New or updated cover — write JPEG to disk cache
    write_cache(key, data, &md5, mtime, cache_dir);

    // Update CoverProvider in-memory index
    match provider.read() {
        Ok(ref prov) => prov.update_entry(key, &md5, mtime),
        Err(e) => log::error!("[cover-proc] CoverProvider RwLock poisoned (update_entry): {e}"),
    }

    // Emit CoverPaths (path-based delivery for cached covers)
    emit_cover_path(key, &md5, cache_dir, event_tx);

    // LRU eviction if cache exceeds size limit
    if let Ok(ref prov) = provider.read() {
        prov.evict_lru(CoverProvider::DEFAULT_MAX_SIZE_BYTES);
    }

    // Decode JPEG → resize to 200×200 RGBA via image crate
    match decode_and_resize(data) {
        Ok(rgba) => {
            log::info!(
                "[cover-proc] '{key}': decoded {} JPEG bytes → {} RGBA bytes (200×200)",
                data.len(),
                rgba.len()
            );
            if let Err(e) = event_tx.try_send(MpdEvent::CoverRefreshed {
                album_id: key.to_string(),
                data: rgba,
            }) {
                log::warn!(
                    "[cover-proc] Failed to send CoverRefreshed for '{key}': {e:?}"
                );
            }
        }
        Err(e) => {
            log::warn!("[cover-proc] '{key}': JPEG decode failed ({e}), skipping CoverRefreshed (placeholder fallback)");
            // CoverPaths already emitted above; UI loads cached JPEG from disk.
            // Emitting raw JPEG as CoverRefreshed would corrupt MemoryTexture (expects RGBA).
        }
    }
}

/// Maximum image dimension accepted for decode (width or height).
/// 4096×4096 → 64 MB RGBA intermediate buffer, well within safe limits for real album art.
/// Rejects MPD protocol maximum 16384×16384 (1 GB buffer) to prevent OOM.
const MAX_DIM: u32 = 4096;

/// Decode JPEG bytes via the `image` crate and resize to 200×200 RGBA (Lanczos3).
///
/// Checks image dimensions via header parse before full decode — rejects images
/// exceeding `MAX_DIM` to avoid allocating an intermediate RGBA buffer large enough
/// to OOM the process.
fn decode_and_resize(data: &[u8]) -> Result<Vec<u8>, String> {
    let reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("{e}"))?;
    let (w, h) = reader.into_dimensions().map_err(|e| format!("{e}"))?;
    if w > MAX_DIM || h > MAX_DIM {
        return Err(format!(
            "image dimensions {w}x{h} exceed MAX_DIM ({MAX_DIM}), rejecting to prevent OOM"
        ));
    }
    let img = image::load_from_memory(data)
        .map_err(|e| format!("{e} (data size: {} bytes)", data.len()))?;
    let rgba = img.to_rgba8();
    let resized = image::imageops::resize(&rgba, 200, 200, FilterType::Lanczos3);
    Ok(resized.into_raw())
}

/// Write cover data to disk cache normalized as JPEG (md5-named file) and update index.json.
///
/// If the MPD returns non-JPEG data (PNG, WebP), it is decoded and re-encoded to JPEG
/// before writing. The MD5 hash is always computed from the original MPD binary data.
fn write_cache(
    album_key: &str,
    data: &[u8],
    md5: &str,
    timestamp: Option<u64>,
    cache_dir: &Path,
) {
    let jpeg_data = ensure_jpeg(data);
    let jpeg_path = cache_dir.join(format!("{md5}.jpg"));
    if let Err(e) = fs::write(&jpeg_path, &jpeg_data) {
        log::warn!("[cover-proc] Failed to write cache for '{album_key}': {e}");
        log::warn!("[cover-proc] Skipping index.json update for '{album_key}' (JPEG write failed)");
        return;
    }
    log::info!(
        "[cover-proc] Cached cover for '{album_key}' at {:?} ({} bytes)",
        jpeg_path,
        jpeg_data.len()
    );

    update_index_json(cache_dir, album_key, md5, timestamp);
}

/// If data is already JPEG, return it unchanged. Otherwise decode and re-encode as JPEG.
fn ensure_jpeg(data: &[u8]) -> Vec<u8> {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return data.to_vec();
    }
    match image::load_from_memory(data) {
        Ok(img) => {
            let mut buf = Vec::new();
            match img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg) {
                Ok(_) => {
                    log::info!(
                        "[cover-proc] Converted non-JPEG ({} bytes) to JPEG ({} bytes)",
                        data.len(),
                        buf.len()
                    );
                    buf
                }
                Err(e) => {
                    log::warn!(
                        "[cover-proc] JPEG re-encode failed: {e}, writing original data"
                    );
                    data.to_vec()
                }
            }
        }
        Err(e) => {
            log::warn!(
                "[cover-proc] Non-JPEG decode failed ({} bytes): {e}, writing original data",
                data.len()
            );
            data.to_vec()
        }
    }
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
    if let Err(e) = event_tx.try_send(MpdEvent::CoverPaths(covers)) {
        static FAIL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = FAIL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n % 10 == 1 {
            log::warn!(
                "[cover-proc] Failed to send CoverPaths ({} total drops): {e:?}",
                n
            );
        }
    }
}
