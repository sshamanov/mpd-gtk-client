use std::path::PathBuf;

pub struct CoverFetcher;

impl CoverFetcher {
    pub fn new() -> Self {
        Self
    }

    pub async fn fetch_cover(&self, album_id: &str) -> Option<PathBuf> {
        // TODO: implement
        None
    }
}