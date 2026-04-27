//! Cover art subsystem — local file scanning, memory cache, display pipeline. Thread: background.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::mpd::MpdAdapter;

pub struct CoverFetcher {
    cache: HashMap<String, Option<PathBuf>>,
}

impl Default for CoverFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverFetcher {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    /// Fetch cover art for an album. Checks memory cache, then scans local files.
    /// Returns path to the cover image file, or None if no cover found.
    pub fn fetch_cover(&mut self, album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
        // Memory cache hit
        if let Some(cached) = self.cache.get(album_name) {
            return cached.clone();
        }

        let result = self.scan_local_cover(album_name, adapter);
        self.cache.insert(album_name.to_string(), result.clone());
        result
    }

    /// Clear the cache (e.g., on library change or reconnect).
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Scan the album's directory for cover image files.
    fn scan_local_cover(&self, album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
        let uris = adapter.find_album_uris(album_name).ok()?;
        let first_uri = uris.first()?;

        // Get the directory containing the album files
        let file_path = Path::new(first_uri);
        let dir = file_path.parent()?;

        // Priority order: embedded first (not supported yet), then common names
        let candidates = ["cover.jpg", "cover.png", "folder.jpg", "Folder.jpg",
                         "front.jpg", "front.png", "albumart.jpg", "AlbumArt.jpg"];

        for name in &candidates {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        // Fallback: any jpg/png in the directory
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("png")
                        || ext.eq_ignore_ascii_case("jpeg") || ext.eq_ignore_ascii_case("webp")
                    {
                        return Some(p);
                    }
                }
            }
        }

        None
    }
}
