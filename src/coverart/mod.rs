//! Cover art subsystem — multi-provider fetch, cache, and delivery pipeline. Thread: background thread pool.

use std::path::PathBuf;

pub mod providers;

pub struct CoverFetcher;

impl Default for CoverFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverFetcher {
    pub fn new() -> Self {
        Self
    }

    pub fn fetch_cover(&self, _album_id: &str) -> Option<PathBuf> {
        None
    }
}
