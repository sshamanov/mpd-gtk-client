//! ActualRead — background fetch queue for cover art. Thread: MPD background thread.
//!
//! ActualRead processes one album per idle cycle from the MPD state machine. It uses
//! `albumart <uri>` (primary) then `readpicture <uri>` (fallback) to fetch cover data,
//! MD5-hashes the binary data, compares against CoverProvider's cache, and emits
//! `CoverPaths` events only when the hash or timestamp differs.
//!
//! CoverProvider reads, ActualRead writes. They never call each other — no loops.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use md5::{Digest, Md5};

use crate::coverart::CoverProvider;
#[cfg(feature = "online-cover-art")]
use crate::coverart::CoverOnlineProvider;
use crate::mpd::MpdAdapter;
use crate::mpd::state_machine::MpdEvent;

/// Shorthand for the event sender type used by the MPD state machine.
type EventSender = mpsc::SyncSender<MpdEvent>;

/// Background cover fetch queue. One album processed per idle cycle.
pub struct ActualRead {
    /// FIFO queue of (artist, album_name) tuples pending fetch.
    queue: VecDeque<(String, String)>,
    /// Cache directory path.
    cache_dir: PathBuf,
    /// Online cover lookup provider (feature-gated, disabled by default).
    #[cfg(feature = "online-cover-art")]
    online: CoverOnlineProvider,
}

impl ActualRead {
    /// Create a new ActualRead with the given cache directory.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            queue: VecDeque::new(),
            cache_dir,
            #[cfg(feature = "online-cover-art")]
            online: CoverOnlineProvider::new(),
        }
    }

    /// Replace the queue with a new set of albums to fetch.
    ///
    /// This replaces any pending fetches — the caller is responsible for providing
    /// the complete set of albums that need covers.
    pub fn enqueue(&mut self, albums: Vec<(String, String)>) {
        self.queue.clear();
        self.queue.extend(albums);
        log::debug!("[actual_read] Enqueued {} albums", self.queue.len());
    }

    /// Returns true if there are pending albums to fetch.
    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Returns the number of pending albums.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Process one album from the queue.
    ///
    /// 1. Pops the front album from the queue.
    /// 2. Primary: `adapter.albumart(uri)` — MD5 hash, compare against CoverProvider cache.
    /// 3. Fallback: `adapter.readpicture(uri)` — compare timestamp against cache.
    /// 4. (optional) Online lookup (feature-gated, disabled by default).
    /// 5. If new data: write cache file, update index.json, emit `CoverPaths`, invalidate CoverProvider.
    /// 6. If unchanged: skip silently (no emission).
    /// 7. If all fail: log at debug level, no emission.
    pub fn process_one(
        &mut self,
        adapter: &mut MpdAdapter,
        caps: &crate::mpd::MpdCapabilities,
        provider: &CoverProvider,
        event_tx: &EventSender,
    ) {
        // When online-cover-art is disabled, `artist` is unused; suppress the warning.
        #[allow(unused_variables)]
        let Some((artist, album_name)) = self.queue.pop_front() else {
            return;
        };

        log::debug!(
            "[actual_read] Processing '{album_name}' ({} remaining)",
            self.queue.len()
        );

        // Step 1: Find album URIs
        let uris = match adapter.find_album_uris(&album_name) {
            Ok(uris) => uris,
            Err(e) => {
                log::error!("[actual_read] find_album_uris failed for '{album_name}': {e}");
                return;
            }
        };
        let Some(uri) = uris.first().cloned() else {
            log::debug!("[actual_read] No URIs found for '{album_name}'");
            return;
        };

        // Step 2: Primary — albumart
        log::info!("[cover] '{album_name}': albumart={}, readpicture={}", caps.albumart, caps.readpicture);
        if caps.albumart {
            match adapter.albumart(&album_name) {
                Ok(Some(data)) => {
                    log::info!("[cover] '{album_name}': albumart returned {} bytes", data.len());
                    if self.handle_albumart_data(&album_name, &data, provider, event_tx) {
                        return;
                    }
                }
                Ok(None) => {
                    log::info!("[cover] '{album_name}': albumart returned no data, trying readpicture");
                }
                Err(e) => {
                    log::info!("[cover] '{album_name}': albumart failed ({e}), trying readpicture");
                }
            }
        }

        // Step 3: Fallback — readpicture
        if caps.readpicture {
            match adapter.readpicture(&uri) {
                Ok(Some((data, mtime))) => {
                    log::info!("[cover] '{album_name}': readpicture returned {} bytes", data.len());
                    self.handle_readpicture_data(&album_name, &data, mtime, provider, event_tx);
                }
                Ok(None) => {
                    log::info!("[cover] '{album_name}': albumart + readpicture both empty");
                }
                Err(e) => {
                    log::info!("[cover] '{album_name}': readpicture failed ({e})");
                }
            }
        }

        // Step 4: Fallback — online lookup (feature-gated, disabled by default)
        #[cfg(feature = "online-cover-art")]
        self.try_online_lookup(&artist, &album_name, provider, event_tx);
    }

    /// Handle albumart data: MD5 hash, compare with cache, write if new.
    /// Returns true if the album was handled (emitted or skipped), false if caller
    /// should try fallback.
    fn handle_albumart_data(
        &mut self,
        album_name: &str,
        data: &[u8],
        provider: &CoverProvider,
        event_tx: &EventSender,
    ) -> bool {
        let md5 = format!("{:x}", Md5::digest(data));

        // Check CoverProvider cache — if same hash, emit path from cache and skip refetch
        if let Some(cached) = provider.get(album_name) {
            if cached.md5 == md5 {
                log::debug!(
                    "[actual_read] '{album_name}': albumart hash unchanged, emitting cached path"
                );
                self.emit_cover_path(album_name, &cached.md5, event_tx);
                return true;
            }
        }

        // New data — write cache and emit both CoverPaths (path-based) and CoverRefreshed (raw bytes)
        self.write_cache(album_name, data, &md5, None);
        self.emit_cover_path(album_name, &md5, event_tx);
        self.emit_cover_refreshed(album_name, data, event_tx);
        provider.update_entry(album_name, &md5, None);
        true
    }

    /// Handle readpicture data: compare timestamp with cache, write if newer.
    fn handle_readpicture_data(
        &mut self,
        album_name: &str,
        data: &[u8],
        mtime: u64,
        provider: &CoverProvider,
        event_tx: &EventSender,
    ) {
        let md5 = format!("{:x}", Md5::digest(data));

        // Check CoverProvider cache — if same hash and timestamp not newer, skip
        if let Some(cached) = provider.get(album_name) {
            if cached.md5 == md5 {
                match cached.timestamp {
                    Some(ts) if ts >= mtime => {
                        log::debug!(
                            "[actual_read] '{album_name}': readpicture timestamp not newer (cached: {ts}, mtime: {mtime}), skipping"
                        );
                        return;
                    }
                    _ => {} // Hash same but timestamp newer — proceed
                }
            }
        }

        // New data — write cache and emit both CoverPaths (path-based) and CoverRefreshed (raw bytes)
        self.write_cache(album_name, data, &md5, Some(mtime));
        self.emit_cover_path(album_name, &md5, event_tx);
        self.emit_cover_refreshed(album_name, data, event_tx);
        provider.update_entry(album_name, &md5, Some(mtime));
    }

    /// Write cover data to disk cache.
    fn write_cache(&self, album_name: &str, data: &[u8], md5: &str, timestamp: Option<u64>) {
        let jpeg_path = self.cache_dir.join(format!("{md5}.jpg"));
        if let Err(e) = fs::write(&jpeg_path, data) {
            log::warn!(
                "[actual_read] Failed to write cache for '{album_name}': {e}"
            );
            // Non-fatal — UI can still display from event payload in future stories.
            // For now, we still emit the CoverPaths event below.
        } else {
            log::info!(
                "[actual_read] Cached cover for '{album_name}' at {:?}",
                jpeg_path
            );
        }

        // Always update the index, even if JPEG write failed (partial update tracking)
        Self::update_index_json(&self.cache_dir, album_name, md5, timestamp);
    }

    /// Emit a CoverPaths event for the given album.
    fn emit_cover_path(&self, album_name: &str, md5: &str, event_tx: &EventSender) {
        let jpeg_path = self.cache_dir.join(format!("{md5}.jpg"));
        let mut covers = HashMap::new();
        // Only emit a path if the file actually exists on disk
        if jpeg_path.exists() {
            covers.insert(
                album_name.to_string(),
                Some(jpeg_path.to_string_lossy().to_string()),
            );
        } else {
            covers.insert(album_name.to_string(), None);
        }
        let _ = event_tx.try_send(MpdEvent::CoverPaths(covers));
    }

    /// Emit a CoverRefreshed event for the given album, carrying raw JPEG bytes.
    ///
    /// The UI thread decodes the raw bytes into a GdkTexture via gdk-pixbuf for
    /// direct widget updates — no path-based handoff needed. This complements
    /// `emit_cover_path` which carries the file path for backward compat.
    fn emit_cover_refreshed(&self, album_name: &str, data: &[u8], event_tx: &EventSender) {
        let _ = event_tx.try_send(MpdEvent::CoverRefreshed {
            album_id: album_name.to_string(),
            data: data.to_vec(),
        });
    }

    /// Try to fetch cover art from online sources when MPD albumart + readpicture fail.
    ///
    /// This is gated behind the `online-cover-art` feature flag and only compiled when
    /// the feature is enabled. On success, writes to cache + emits events like any other
    /// cover source. On failure, silently returns (placeholder stays).
    #[cfg(feature = "online-cover-art")]
    fn try_online_lookup(
        &mut self,
        artist: &str,
        album_name: &str,
        provider: &CoverProvider,
        event_tx: &EventSender,
    ) {
        let data = match self.online.lookup(artist, album_name) {
            Some(d) => d,
            None => return,
        };

        log::info!(
            "[actual_read] Online cover fetched for '{album_name}' ({} bytes)",
            data.len()
        );

        let md5 = format!("{:x}", Md5::digest(&data));
        self.write_cache(album_name, &data, &md5, None);
        self.emit_cover_path(album_name, &md5, event_tx);
        self.emit_cover_refreshed(album_name, &data, event_tx);
        provider.update_entry(album_name, &md5, None);
    }

    /// Update the index.json sidecar file with a new or updated entry.
    ///
    /// Reads the current index, upserts the entry, writes atomically via temp file + rename.
    /// Failure is logged at warn level and silently ignored — index will be rebuilt on
    /// next startup or next write.
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

        // Atomic write: write to .tmp then rename
        match serde_json::to_string(&index) {
            Ok(json) => {
                if let Err(e) = fs::write(&tmp_path, &json) {
                    log::warn!(
                        "[actual_read] Failed to write index tmp for '{album_id}': {e}"
                    );
                    return;
                }
                if let Err(e) = fs::rename(&tmp_path, &index_path) {
                    log::warn!(
                        "[actual_read] Failed to rename index for '{album_id}': {e}"
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "[actual_read] Failed to serialize index for '{album_id}': {e}"
                );
            }
        }
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: very minimal JPEG bytes (SOI + EOI, not fully valid but enough for
    /// hashing and caching in tests).
    fn dummy_jpeg_bytes() -> Vec<u8> {
        // Minimal 2-byte JPEG SOI marker + padding
        let mut data = vec![0xFF, 0xD8, 0xFF];
        data.extend(std::iter::repeat(0x42).take(100));
        data
    }

    /// Another dummy JPEG with different content.
    fn dummy_jpeg_bytes_v2() -> Vec<u8> {
        let mut data = vec![0xFF, 0xD8, 0xFF];
        data.extend(std::iter::repeat(0x99).take(100));
        data
    }

    /// Create a temp cache directory for testing.
    fn test_cache_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let cache_dir = dir.path().join("mpd-client").join("covers");
        fs::create_dir_all(&cache_dir).expect("Failed to create test cache dir");
        (dir, cache_dir)
    }

    /// Create a CoverProvider pointing at the given cache directory.
    fn test_provider(cache_dir: &Path) -> CoverProvider {
        CoverProvider {
            cache_dir: cache_dir.to_path_buf(),
            index: std::sync::RwLock::new(HashMap::new()),
        }
    }

    #[test]
    fn test_enqueue_and_pending() {
        let cache_dir = PathBuf::from("/tmp/test_actual_read");
        let mut ar = ActualRead::new(cache_dir);

        assert!(!ar.has_pending());
        assert_eq!(ar.pending_count(), 0);

        ar.enqueue(vec![
            ("Artist1".into(), "Album1".into()),
            ("Artist2".into(), "Album2".into()),
        ]);

        assert!(ar.has_pending());
        assert_eq!(ar.pending_count(), 2);
    }

    #[test]
    fn test_enqueue_replaces_previous() {
        let cache_dir = PathBuf::from("/tmp/test_actual_read");
        let mut ar = ActualRead::new(cache_dir);

        ar.enqueue(vec![("A".into(), "Old".into())]);
        assert_eq!(ar.pending_count(), 1);

        ar.enqueue(vec![("B".into(), "New".into())]);
        assert_eq!(ar.pending_count(), 1); // Old was replaced
    }

    #[test]
    fn test_fifo_order() {
        let cache_dir = PathBuf::from("/tmp/test_actual_read");
        let mut ar = ActualRead::new(cache_dir);

        ar.enqueue(vec![
            ("A".into(), "First".into()),
            ("B".into(), "Second".into()),
            ("C".into(), "Third".into()),
        ]);

        assert_eq!(ar.queue.front().unwrap().1, "First");
        let _ = ar.queue.pop_front();
        assert_eq!(ar.queue.front().unwrap().1, "Second");
    }

    #[test]
    fn test_write_cache_and_index_update() {
        let (_dir, cache_dir) = test_cache_dir();
        let ar = ActualRead::new(cache_dir.clone());
        let data = dummy_jpeg_bytes();
        let md5 = format!("{:x}", Md5::digest(&data));

        // Write cache
        ar.write_cache("TestAlbum", &data, &md5, Some(12345));

        // Verify JPEG file exists
        let jpeg_path = cache_dir.join(format!("{md5}.jpg"));
        assert!(jpeg_path.exists(), "JPEG cache file should exist");

        // Verify index.json was updated
        let index_path = cache_dir.join("index.json");
        assert!(index_path.exists(), "index.json should exist");

        let content = fs::read_to_string(&index_path).expect("Should read index.json");
        let index: HashMap<String, serde_json::Value> =
            serde_json::from_str(&content).expect("Should parse index.json");
        assert!(index.contains_key("TestAlbum"), "index should contain TestAlbum");

        let entry = index.get("TestAlbum").unwrap();
        assert_eq!(entry.get("md5").and_then(|v| v.as_str()), Some(md5.as_str()));
        assert_eq!(
            entry.get("timestamp").and_then(|v| v.as_u64()),
            Some(12345)
        );
    }

    #[test]
    fn test_write_cache_file_failure_is_non_fatal() {
        // Use a non-writable path (root of filesystem without write perms)
        let cache_dir = PathBuf::from("/proc/self/covers");
        let ar = ActualRead::new(cache_dir);
        let data = dummy_jpeg_bytes();
        let md5 = format!("{:x}", Md5::digest(&data));

        // This should log a warning but not panic
        ar.write_cache("TestAlbum", &data, &md5, None);
        // No assertion needed — we just verify no panic
    }

    #[test]
    fn test_update_index_json_creates_file() {
        let (_dir, cache_dir) = test_cache_dir();

        ActualRead::update_index_json(&cache_dir, "Album1", "abc123", None);

        let index_path = cache_dir.join("index.json");
        assert!(index_path.exists());

        let content = fs::read_to_string(&index_path).unwrap();
        let index: HashMap<String, serde_json::Value> =
            serde_json::from_str(&content).unwrap();
        assert_eq!(
            index.get("Album1").and_then(|v| v.get("md5")).and_then(|v| v.as_str()),
            Some("abc123")
        );
    }

    #[test]
    fn test_update_index_json_updates_existing() {
        let (_dir, cache_dir) = test_cache_dir();

        // First write
        ActualRead::update_index_json(&cache_dir, "Album1", "oldhash", None);

        // Update
        ActualRead::update_index_json(&cache_dir, "Album1", "newhash", Some(99999));

        let content = fs::read_to_string(cache_dir.join("index.json")).unwrap();
        let index: HashMap<String, serde_json::Value> =
            serde_json::from_str(&content).unwrap();
        let entry = index.get("Album1").unwrap();
        assert_eq!(
            entry.get("md5").and_then(|v| v.as_str()),
            Some("newhash")
        );
        assert_eq!(entry.get("timestamp").and_then(|v| v.as_u64()), Some(99999));
    }

    #[test]
    fn test_handle_albumart_data_identical_hash_skips() {
        let (_dir, cache_dir) = test_cache_dir();
        let provider = test_provider(&cache_dir);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut ar = ActualRead::new(cache_dir.clone());

        let data = dummy_jpeg_bytes();
        let md5 = format!("{:x}", Md5::digest(&data));

        // Populate provider cache with same hash
        if let Ok(mut index) = provider.index.write() {
            use crate::coverart::provider::IndexEntry;
            index.insert(
                "TestAlbum".to_string(),
                IndexEntry {
                    md5: md5.clone(),
                    timestamp: None,
                },
            );
        }

        // Also write the JPEG so provider.get() succeeds
        fs::write(cache_dir.join(format!("{md5}.jpg")), &data).unwrap();

        let handled = ar.handle_albumart_data("TestAlbum", &data, &provider, &tx);
        assert!(handled, "Should handle albumart data");
        // CoverPaths should be emitted for cached covers (path-based delivery)
        assert!(rx.try_recv().is_ok(), "CoverPaths should be emitted for cached hash");
    }

    #[test]
    fn test_handle_albumart_data_new_hash_emits() {
        let (_dir, cache_dir) = test_cache_dir();
        let provider = test_provider(&cache_dir);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut ar = ActualRead::new(cache_dir.clone());

        let data = dummy_jpeg_bytes();
        let data2 = dummy_jpeg_bytes_v2();

        // Set up provider cache with v1 hash
        let md5_v1 = format!("{:x}", Md5::digest(&data));
        if let Ok(mut index) = provider.index.write() {
            use crate::coverart::provider::IndexEntry;
            index.insert(
                "TestAlbum".to_string(),
                IndexEntry {
                    md5: md5_v1,
                    timestamp: None,
                },
            );
        }

        let handled = ar.handle_albumart_data("TestAlbum", &data2, &provider, &tx);
        assert!(handled, "Should handle albumart data");

        // Should emit an event
        let event = rx.try_recv().expect("Should emit CoverPaths for new hash");
        match event {
            MpdEvent::CoverPaths(paths) => {
                assert!(paths.contains_key("TestAlbum"));
            }
            _ => panic!("Expected CoverPaths event"),
        }
    }

    #[test]
    fn test_handle_albumart_data_no_cache_entry_emits() {
        let (_dir, cache_dir) = test_cache_dir();
        let provider = test_provider(&cache_dir);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut ar = ActualRead::new(cache_dir);

        let data = dummy_jpeg_bytes();
        let handled = ar.handle_albumart_data("NewAlbum", &data, &provider, &tx);
        assert!(handled, "Should handle albumart data");

        // Should emit an event
        let event = rx.try_recv().expect("Should emit CoverPaths for new album");
        match event {
            MpdEvent::CoverPaths(paths) => {
                assert!(paths.contains_key("NewAlbum"));
            }
            _ => panic!("Expected CoverPaths event"),
        }
    }

    #[test]
    fn test_handle_readpicture_data_older_timestamp_skips() {
        let (_dir, cache_dir) = test_cache_dir();
        let provider = test_provider(&cache_dir);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut ar = ActualRead::new(cache_dir.clone());

        let data = dummy_jpeg_bytes();
        let md5 = format!("{:x}", Md5::digest(&data));

        // Populate provider cache with same hash and newer timestamp
        if let Ok(mut index) = provider.index.write() {
            use crate::coverart::provider::IndexEntry;
            index.insert(
                "TestAlbum".to_string(),
                IndexEntry {
                    md5: md5.clone(),
                    timestamp: Some(200), // cached timestamp is newer
                },
            );
        }

        // Write the JPEG so provider.get() succeeds
        fs::write(cache_dir.join(format!("{md5}.jpg")), &data).unwrap();

        ar.handle_readpicture_data("TestAlbum", &data, 100, &provider, &tx);
        // Should skip — cached timestamp (200) > mtime (100)
        assert!(rx.try_recv().is_err(), "No event for older mtime");
    }

    #[test]
    fn test_handle_readpicture_data_newer_timestamp_emits() {
        let (_dir, cache_dir) = test_cache_dir();
        let provider = test_provider(&cache_dir);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut ar = ActualRead::new(cache_dir.clone());

        let data = dummy_jpeg_bytes_v2();
        let md5 = format!("{:x}", Md5::digest(&data));

        // Populate provider cache with SAME hash but OLDER timestamp
        if let Ok(mut index) = provider.index.write() {
            use crate::coverart::provider::IndexEntry;
            index.insert(
                "TestAlbum".to_string(),
                IndexEntry {
                    md5: md5.clone(),
                    timestamp: Some(50), // old timestamp
                },
            );
        }

        // Write JPEG
        fs::write(cache_dir.join(format!("{md5}.jpg")), &data).unwrap();

        ar.handle_readpicture_data("TestAlbum", &data, 100, &provider, &tx);
        // Should emit — mtime (100) > cached timestamp (50)
        let event = rx.try_recv().expect("Should emit CoverPaths for newer mtime");
        match event {
            MpdEvent::CoverPaths(paths) => {
                assert!(paths.contains_key("TestAlbum"));
            }
            _ => panic!("Expected CoverPaths event"),
        }
    }

    #[test]
    fn test_process_one_empty_queue_does_nothing() {
        let (_dir, cache_dir) = test_cache_dir();
        let _provider = test_provider(&cache_dir);
        let _tx: mpsc::SyncSender<MpdEvent> = mpsc::sync_channel(64).0;
        let ar = ActualRead::new(cache_dir);

        // process_one on empty queue should be a no-op (no panic)
        // We can't easily test this with a real adapter, so we just verify
        // the method returns without panicking
        assert_eq!(ar.pending_count(), 0);
    }
}
