//! Local metadata cache — full album metadata accessible from any thread.
//! Built from MPD's `list_albums_full()` output, with optional batch file-path fetch.
//! Thread: read from any thread (RwLock), write from MPD thread.

use std::collections::HashMap;
use std::sync::RwLock;

/// Composite cache key for an album, matching `cover_key` in the coverart module.
/// Album name alone isn't unique (e.g., "Greatest Hits" by multiple artists).
pub fn album_key(artist: &str, album: &str) -> String {
    format!("{}||{}", artist, album)
}

/// Thread-safe in-memory cache of full album metadata.
///
/// Built once on connect/library-change from `list_albums_full()`.
/// Provides O(1) lookup by `(album_artist, album_name)` composite key.
pub struct MetadataCache {
    /// Primary storage: composite key → AlbumMeta
    albums: RwLock<HashMap<String, crate::mpd::AlbumMeta>>,
    /// Secondary index: album_name → list of composite keys (for album-name-only lookup)
    by_album: RwLock<HashMap<String, Vec<String>>>,
    /// Optional file paths: composite key → list of file URIs
    file_paths: RwLock<HashMap<String, Vec<String>>>,
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataCache {
    pub fn new() -> Self {
        Self {
            albums: RwLock::new(HashMap::new()),
            by_album: RwLock::new(HashMap::new()),
            file_paths: RwLock::new(HashMap::new()),
        }
    }

    /// Build the cache from a flat album metadata list.
    /// Replaces all existing entries.
    pub fn build(&self, albums: Vec<crate::mpd::AlbumMeta>) {
        let mut map: HashMap<String, crate::mpd::AlbumMeta> = HashMap::new();
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();

        for meta in albums {
            let key = album_key(&meta.album_artist, &meta.album);
            by_name
                .entry(meta.album.to_lowercase())
                .or_default()
                .push(key.clone());
            map.insert(key, meta);
        }

        if let Ok(mut guard) = self.albums.write() {
            *guard = map;
        }
        if let Ok(mut guard) = self.by_album.write() {
            *guard = by_name;
        }
    }

    /// Lookup full metadata by artist + album name.
    pub fn get(&self, artist: &str, album: &str) -> Option<crate::mpd::AlbumMeta> {
        let key = album_key(artist, album);
        self.albums.read().ok()?.get(&key).cloned()
    }

    /// Lookup all albums matching a given album name (across different artists).
    pub fn get_by_album(&self, album: &str) -> Vec<crate::mpd::AlbumMeta> {
        let by_name = match self.by_album.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let albums = match self.albums.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        let keys = match by_name.get(&album.to_lowercase()) {
            Some(k) => k,
            None => return Vec::new(),
        };

        keys.iter().filter_map(|k| albums.get(k).cloned()).collect()
    }

    /// Return all cached album metadata in stable order (sorted by album name).
    pub fn all_albums(&self) -> Vec<crate::mpd::AlbumMeta> {
        let albums = match self.albums.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut v: Vec<crate::mpd::AlbumMeta> = albums.values().cloned().collect();
        v.sort_by(|a, b| a.album.to_lowercase().cmp(&b.album.to_lowercase()));
        v
    }

    /// Number of cached albums.
    pub fn len(&self) -> usize {
        self.albums.read().map(|g| g.len()).unwrap_or(0)
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all cached data.
    pub fn clear(&self) {
        if let Ok(mut g) = self.albums.write() {
            g.clear();
        }
        if let Ok(mut g) = self.by_album.write() {
            g.clear();
        }
        if let Ok(mut g) = self.file_paths.write() {
            g.clear();
        }
    }

    /// Store file paths for an album.
    pub fn set_file_paths(&self, artist: &str, album: &str, paths: Vec<String>) {
        let key = album_key(artist, album);
        if let Ok(mut g) = self.file_paths.write() {
            g.insert(key, paths);
        }
    }

    /// Lookup stored file paths for an album.
    pub fn get_file_paths(&self, artist: &str, album: &str) -> Option<Vec<String>> {
        let key = album_key(artist, album);
        self.file_paths.read().ok()?.get(&key).cloned()
    }

    /// Bulk-load file paths from a batch fetch result.
    /// `data` is a flat list of (artist, album, file_uri) tuples from MPD.
    pub fn load_file_paths(&self, data: Vec<(String, String, Vec<String>)>) {
        if let Ok(mut g) = self.file_paths.write() {
            for (artist, album, paths) in data {
                let key = album_key(&artist, &album);
                g.insert(key, paths);
            }
        }
    }

    /// Return album count for change detection.
    pub fn album_count(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_meta(album: &str, artist: &str, year: Option<&str>) -> crate::mpd::AlbumMeta {
        crate::mpd::AlbumMeta {
            album: album.to_string(),
            album_artist: artist.to_string(),
            track_artists: vec![],
            year: year.map(|s| s.to_string()),
            genre: None,
        }
    }

    #[test]
    fn test_build_and_lookup() {
        let cache = MetadataCache::new();
        assert!(cache.is_empty());

        cache.build(vec![
            make_meta("Abbey Road", "The Beatles", Some("1969")),
            make_meta("Kind of Blue", "Miles Davis", Some("1959")),
        ]);

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.album_count(), 2);

        let m = cache.get("The Beatles", "Abbey Road").unwrap();
        assert_eq!(m.year.as_deref(), Some("1969"));

        let m = cache.get("Miles Davis", "Kind of Blue").unwrap();
        assert_eq!(m.year.as_deref(), Some("1959"));

        assert!(cache.get("Nobody", "Nowhere").is_none());
    }

    #[test]
    fn test_get_by_album() {
        let cache = MetadataCache::new();
        cache.build(vec![
            make_meta("Greatest Hits", "Queen", Some("1981")),
            make_meta("Greatest Hits", "Journey", Some("1988")),
        ]);

        let results = cache.get_by_album("Greatest Hits");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_all_albums_sorted() {
        let cache = MetadataCache::new();
        cache.build(vec![
            make_meta("Ziggy Stardust", "David Bowie", None),
            make_meta("Abbey Road", "The Beatles", None),
        ]);

        let all = cache.all_albums();
        assert_eq!(all[0].album, "Abbey Road");
        assert_eq!(all[1].album, "Ziggy Stardust");
    }

    #[test]
    fn test_clear() {
        let cache = MetadataCache::new();
        cache.build(vec![make_meta("Test", "Artist", None)]);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_file_paths() {
        let cache = MetadataCache::new();
        cache.set_file_paths("Artist", "Album", vec!["path/1.flac".into(), "path/2.flac".into()]);

        let paths = cache.get_file_paths("Artist", "Album").unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "path/1.flac");

        cache.clear();
        assert!(cache.get_file_paths("Artist", "Album").is_none());
    }

    #[test]
    fn test_build_replaces() {
        let cache = MetadataCache::new();
        cache.build(vec![make_meta("Album A", "Artist 1", None)]);
        assert_eq!(cache.len(), 1);

        cache.build(vec![make_meta("Album B", "Artist 2", None)]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get("Artist 1", "Album A").is_none());
        assert!(cache.get("Artist 2", "Album B").is_some());
    }
}
