//! Cover art subsystem — MPD albumart command with disk cache. Thread: background.

pub mod actual_read;
pub mod provider;

pub use actual_read::ActualRead;
pub use provider::{CachedCover, CoverProvider};

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

    pub fn fetch_cover(&mut self, album_name: &str, adapter: &mut MpdAdapter) -> Option<PathBuf> {
        if let Some(cached) = self.cache.get(album_name) {
            return cached.clone();
        }
        let result = fetch_via_mpd(album_name, adapter, &self.cache_dir);
        self.cache.insert(album_name.to_string(), result.clone());
        result
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

fn fetch_via_mpd(album_name: &str, adapter: &mut MpdAdapter, cache_dir: &std::path::Path) -> Option<PathBuf> {
    let data = adapter.albumart(album_name).ok()??;
    log::info!("[cover] '{album_name}': got {} bytes", data.len());
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    album_name.hash(&mut hasher);
    let hash = format!("{:016x}", hasher.finish());
    let path = cache_dir.join(format!("{hash}.jpg"));
    match std::fs::File::create(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(&data) {
                log::error!("[cover] '{album_name}': write failed: {e}");
                return None;
            }
            log::info!("[cover] '{album_name}': saved to {}", path.display());
            Some(path)
        }
        Err(e) => {
            log::error!("[cover] '{album_name}': create failed: {e}");
            None
        }
    }
}
