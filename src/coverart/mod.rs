//! Cover art subsystem — MPD albumart command with disk cache. Thread: background.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use crate::mpd::MpdAdapter;

pub struct CoverFetcher {
    cache: HashMap<String, Option<PathBuf>>,
    cache_dir: PathBuf,
}

impl Default for CoverFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverFetcher {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("mpd-client")
            .join("covers");
        let _ = std::fs::create_dir_all(&cache_dir);
        Self { cache: HashMap::new(), cache_dir }
    }

    /// Fetch cover art via MPD albumart command. Caches to disk for GTK Picture display.
    pub fn fetch_cover(&mut self, album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
        if let Some(cached) = self.cache.get(album_name) {
            return cached.clone();
        }
        let result = self.fetch_via_mpd(album_name, adapter);
        self.cache.insert(album_name.to_string(), result.clone());
        result
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    fn fetch_via_mpd(&self, album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
        let data = adapter.albumart(album_name).ok()??;
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        album_name.hash(&mut hasher);
        let hash = format!("{:016x}", hasher.finish());
        let path = self.cache_dir.join(format!("{hash}.jpg"));
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = f.write_all(&data);
        }
        Some(path)
    }
}
