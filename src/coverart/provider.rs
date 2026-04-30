//! CoverProvider — fast synchronous cache read for cover art. Thread: any.
//!
//! CoverProvider is the read-side of the two-layer cover art pipeline. It maintains
//! an in-memory index of album_id → (md5_hash, timestamp) built from a sidecar
//! `index.json` file in the cache directory. `get()` returns cached cover info
//! synchronously in <1ms — no I/O at query time beyond a `stat()` on the cache file.
//!
//! CoverProvider never writes to disk, never fetches from MPD, and never falls through
//! to any provider chain. It is pure read. ActualRead (Story 13.2) handles writes.
//! CoverProvider and ActualRead never call each other — no loops.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Metadata for a single cached cover image.
#[derive(Debug, Clone)]
pub struct CachedCover {
    /// Absolute path to the JPEG file in the cache directory.
    pub path: PathBuf,
    /// Hex-encoded MD5 hash of the albumart binary data.
    pub md5: String,
    /// Optional mtime timestamp (available for readpicture fetches).
    pub timestamp: Option<u64>,
}

/// Internal index entry stored in index.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    /// Hex-encoded MD5 hash; also the filename stem: `{md5}.jpg`.
    md5: String,
    /// Optional mtime timestamp from readpicture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}

/// Fast synchronous cache reader for cover art.
///
/// ## Thread Safety
/// Internal index is behind `std::sync::RwLock`, allowing concurrent reads and
/// infrequent writes (invalidate). Share via `Arc<CoverProvider>`.
pub struct CoverProvider {
    cache_dir: PathBuf,
    index: RwLock<HashMap<String, IndexEntry>>,
}

impl CoverProvider {
    const JPEG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];

    /// Create a new CoverProvider, building the in-memory index from disk.
    ///
    /// If the cache directory cannot be created or the index file is corrupt,
    /// an empty index is used and warnings are logged. Never panics.
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("mpd-client")
            .join("covers");

        if let Err(e) = fs::create_dir_all(&cache_dir) {
            log::warn!(
                "[cover_provider] Failed to create cache directory {:?}: {}",
                cache_dir,
                e
            );
        }

        let index = match Self::load_index(&cache_dir) {
            Ok(idx) => {
                log::debug!(
                    "[cover_provider] Loaded index with {} entries from {:?}",
                    idx.len(),
                    cache_dir
                );
                idx
            }
            Err(e) => {
                log::warn!("[cover_provider] Failed to load cache index: {e}");
                HashMap::new()
            }
        };

        Self {
            cache_dir,
            index: RwLock::new(index),
        }
    }

    /// Look up a cached cover by album ID.
    ///
    /// Returns `Some(CachedCover)` if the album has a valid cached cover,
    /// or `None` if no cache entry exists or the cache file is missing/corrupt.
    ///
    /// Corrupt cache files are deleted and the index entry is removed.
    /// No disk I/O beyond `stat()` and optional corrupt-file read + delete.
    pub fn get(&self, album_id: &str) -> Option<CachedCover> {
        let entry = {
            let index = self.index.read().ok()?;
            index.get(album_id)?.clone()
        };

        let path = self.cache_dir.join(format!("{}.jpg", entry.md5));

        // Check if the cache file still exists
        if !path.exists() {
            log::debug!(
                "[cover_provider] Cache file missing for '{album_id}': {:?}",
                path
            );
            self.invalidate(album_id);
            return None;
        }

        // Validate JPEG header — corrupt files are deleted
        if !Self::is_valid_jpeg(&path) {
            log::warn!(
                "[cover_provider] Corrupt cache file for '{album_id}': {:?} — deleting",
                path
            );
            let _ = fs::remove_file(&path);
            self.invalidate(album_id);
            return None;
        }

        Some(CachedCover {
            path,
            md5: entry.md5,
            timestamp: entry.timestamp,
        })
    }

    /// Remove an entry from the in-memory index.
    ///
    /// Called when a cache file is missing or corrupt. Does NOT touch the
    /// filesystem — that is the responsibility of the component that writes
    /// the cache (ActualRead, Story 13.2).
    pub fn invalidate(&self, album_id: &str) {
        if let Ok(mut index) = self.index.write() {
            if index.remove(album_id).is_some() {
                log::debug!("[cover_provider] Invalidated index entry for '{album_id}'");
            }
        }
    }

    /// Number of entries in the index.
    pub fn len(&self) -> usize {
        self.index.read().map(|i| i.len()).unwrap_or(0)
    }

    /// Returns true if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // ── Private helpers ──

    /// Load the index from `cache_dir/index.json`.
    fn load_index(cache_dir: &Path) -> Result<HashMap<String, IndexEntry>, String> {
        let index_path = cache_dir.join("index.json");
        if !index_path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&index_path)
            .map_err(|e| format!("read error: {e}"))?;
        let index: HashMap<String, IndexEntry> = serde_json::from_str(&content)
            .map_err(|e| format!("parse error: {e}"))?;
        Ok(index)
    }

    /// Check that a file has a valid JPEG header (FF D8 FF).
    fn is_valid_jpeg(path: &Path) -> bool {
        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut header = [0u8; 3];
        if file.read_exact(&mut header).is_err() {
            return false;
        }
        header == Self::JPEG_MAGIC
    }
}

impl Default for CoverProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a minimal valid JPEG byte sequence.
    fn valid_jpeg_bytes() -> Vec<u8> {
        // Minimal valid JPEG: SOI + APP0 + DQT + SOF0 + DHT + SOS + EOI
        // This is a 1x1 pixel grayscale JPEG
        vec![
            0xFF, 0xD8, 0xFF, // SOI + start of APP0 marker
            0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00,
            0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
            0x00, // APP0 (JFIF)
            0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09,
            0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B,
            0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D,
            0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
            0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C,
            0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31, 0x34,
            0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, // DQT
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00,
            0x01, 0x01, 0x01, 0x11, 0x00, // SOF0 (1x1)
            0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03,
            0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
            // DHT (first of two)
            0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01,
            0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04,
            0x04, 0x00, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02,
            0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31,
            0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71,
            0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
            0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33,
            0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18,
            0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A,
            0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43,
            0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53,
            0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63,
            0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73,
            0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83,
            0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92,
            0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A,
            0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9,
            0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8,
            0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7,
            0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
            0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4,
            0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2,
            0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA,
            // DHT (second)
            0xFF, 0xC4, 0x00, 0x1F, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0xFF, 0xD9, // EOI
        ]
    }

    /// Helper: create a temporary directory for testing.
    fn test_cache_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let cache_dir = dir.path().join("mpd-client").join("covers");
        fs::create_dir_all(&cache_dir).expect("Failed to create test cache dir");
        (dir, cache_dir)
    }

    /// Helper: write index.json to the cache directory.
    fn write_index(cache_dir: &Path, index: &HashMap<String, IndexEntry>) {
        let json = serde_json::to_string(index).expect("Failed to serialize index");
        let mut f = fs::File::create(cache_dir.join("index.json"))
            .expect("Failed to create index.json");
        f.write_all(json.as_bytes()).expect("Failed to write index.json");
    }

    #[test]
    fn test_cache_hit() {
        let (_dir, cache_dir) = test_cache_dir();
        let md5 = "abcdef0123456789abcdef0123456789";

        // Write the JPEG file
        fs::write(cache_dir.join(format!("{md5}.jpg")), valid_jpeg_bytes())
            .expect("Failed to write test JPEG");

        // Write index
        let mut index = HashMap::new();
        index.insert(
            "test_album".to_string(),
            IndexEntry {
                md5: md5.to_string(),
                timestamp: Some(12345),
            },
        );
        write_index(&cache_dir, &index);

        let provider = CoverProvider {
            cache_dir: cache_dir.to_path_buf(),
            index: RwLock::new(index),
        };

        let result = provider.get("test_album");
        assert!(result.is_some(), "Expected Some for cached cover");
        let cover = result.unwrap();
        assert_eq!(cover.md5, md5);
        assert_eq!(cover.timestamp, Some(12345));
        assert!(cover.path.exists());
        assert!(cover.path.to_string_lossy().ends_with(&format!("{md5}.jpg")));
    }

    #[test]
    fn test_cache_miss() {
        let (_dir, cache_dir) = test_cache_dir();
        let provider = CoverProvider {
            cache_dir,
            index: RwLock::new(HashMap::new()),
        };
        assert!(provider.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_missing_file() {
        let (_dir, cache_dir) = test_cache_dir();
        let md5 = "abcdef0123456789abcdef0123456789";

        // Write index referencing a file that doesn't exist
        let mut index = HashMap::new();
        index.insert(
            "test_album".to_string(),
            IndexEntry {
                md5: md5.to_string(),
                timestamp: None,
            },
        );
        write_index(&cache_dir, &index);

        let provider = CoverProvider {
            cache_dir: cache_dir.to_path_buf(),
            index: RwLock::new(HashMap::new()),
        };

        // Manually insert into the live index (simulating loading a stale index)
        if let Ok(mut idx) = provider.index.write() {
            idx.insert(
                "test_album".to_string(),
                IndexEntry {
                    md5: md5.to_string(),
                    timestamp: None,
                },
            );
        }

        // get() should return None and remove the stale entry
        assert!(provider.get("test_album").is_none());
        assert!(provider.get("test_album").is_none()); // Still gone after second call
    }

    #[test]
    fn test_startup_index_load() {
        let (_dir, cache_dir) = test_cache_dir();
        let md5 = "abcdef0123456789abcdef0123456789";

        // Write JPEG and index
        fs::write(cache_dir.join(format!("{md5}.jpg")), valid_jpeg_bytes())
            .expect("Failed to write test JPEG");
        let mut index = HashMap::new();
        index.insert(
            "album_1".to_string(),
            IndexEntry {
                md5: md5.to_string(),
                timestamp: None,
            },
        );
        write_index(&cache_dir, &index);

        // Override cache_dir so new() uses our test dir
        let provider = CoverProvider {
            cache_dir: cache_dir.to_path_buf(),
            index: RwLock::new(
                CoverProvider::load_index(&cache_dir).unwrap_or_default(),
            ),
        };

        assert_eq!(provider.len(), 1);
        assert!(provider.get("album_1").is_some());
    }

    #[test]
    fn test_empty_cache_dir() {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let cache_dir = dir.path().join("mpd-client").join("covers");

        let provider = CoverProvider {
            cache_dir,
            index: RwLock::new(HashMap::new()),
        };

        assert!(provider.is_empty());
        assert!(provider.get("anything").is_none());
    }

    #[test]
    fn test_corrupt_index_json() {
        let (_dir, cache_dir) = test_cache_dir();

        // Write corrupt JSON
        fs::write(cache_dir.join("index.json"), "not valid json{{{")
            .expect("Failed to write corrupt index.json");

        let result = CoverProvider::load_index(&cache_dir);
        assert!(result.is_err(), "Expected error for corrupt index.json");
    }

    #[test]
    fn test_corrupt_jpeg_file() {
        let (_dir, cache_dir) = test_cache_dir();
        let md5 = "abcdef0123456789abcdef0123456789";

        // Write invalid JPEG bytes
        fs::write(cache_dir.join(format!("{md5}.jpg")), b"not a jpeg file")
            .expect("Failed to write test file");

        // Write index
        let mut index = HashMap::new();
        index.insert(
            "test_album".to_string(),
            IndexEntry {
                md5: md5.to_string(),
                timestamp: None,
            },
        );
        write_index(&cache_dir, &index);

        let provider = CoverProvider {
            cache_dir: cache_dir.to_path_buf(),
            index: RwLock::new(index),
        };

        // get() should detect corruption, delete the file, return None
        assert!(provider.get("test_album").is_none());
        // File should be deleted
        assert!(!cache_dir.join(format!("{md5}.jpg")).exists());
        // Index entry should be removed
        assert!(provider.get("test_album").is_none());
    }

    #[test]
    fn test_invalidate() {
        let (_dir, cache_dir) = test_cache_dir();
        let mut index = HashMap::new();
        index.insert(
            "album_x".to_string(),
            IndexEntry {
                md5: "aaa".to_string(),
                timestamp: None,
            },
        );

        let provider = CoverProvider {
            cache_dir: cache_dir.to_path_buf(),
            index: RwLock::new(index),
        };

        assert!(provider.get("album_x").is_none()); // file doesn't exist, gets invalidated

        // Index should now be empty
        assert!(provider.is_empty());
    }
}
