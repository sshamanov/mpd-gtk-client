//! Cover art subsystem — local filesystem scanning. Thread: background.

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
    pub fn fetch_cover(&mut self, album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
        if let Some(cached) = self.cache.get(album_name) {
            return cached.clone();
        }
        let result = scan_album_dir(album_name, adapter);
        self.cache.insert(album_name.to_string(), result.clone());
        result
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

/// Scan the album's directory for cover image files.
fn scan_album_dir(album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
    let uris = adapter.find_album_uris(album_name).ok()?;
    let first_uri = uris.first()?;
    let file_path = Path::new(first_uri);
    let dir = file_path.parent()?;

    // Try with common MPD music directory prefixes for relative paths
    let prefixes: &[&str] = if dir.is_relative() {
        &["", "/var/lib/mpd/music/", "/var/lib/mpd/"]
    } else {
        &[""]
    };

    let candidates = ["cover.jpg", "cover.png", "folder.jpg", "Folder.jpg",
                     "front.jpg", "front.png", "albumart.jpg", "AlbumArt.jpg",
                     "Cover.jpg", "COVER.jpg"];

    for prefix in prefixes {
        let base = if prefix.is_empty() { dir.to_path_buf() } else { PathBuf::from(prefix).join(dir) };
        for name in &candidates {
            let candidate = base.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        // Fallback: first jpg/png in the directory
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if matches!(ext.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp") {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}
