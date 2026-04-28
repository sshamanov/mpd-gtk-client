//! Local in-memory search index — fast full-text search without MPD round-trips. Thread: UI build (from event data), UI query.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

pub struct SearchIndex {
    albums: Vec<(String, String)>,
    index: RwLock<HashMap<String, Vec<usize>>>,
}

impl Default for SearchIndex {
    fn default() -> Self { Self::new() }
}

impl SearchIndex {
    pub fn new() -> Self {
        Self { albums: Vec::new(), index: RwLock::new(HashMap::new()) }
    }

    /// Build the index from a flat album list.
    pub fn build(&mut self, albums: &[(String, String)]) {
        self.albums = albums.to_vec();
        let mut idx: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, (artist, album)) in self.albums.iter().enumerate() {
            for token in tokenize(artist) {
                idx.entry(token).or_default().push(i);
            }
            for token in tokenize(album) {
                idx.entry(token).or_default().push(i);
            }
        }
        if let Ok(mut guard) = self.index.write() { *guard = idx; }
    }

    /// Return the number of indexed albums.
    pub fn album_count(&self) -> usize { self.albums.len() }

    /// Search the local index. Returns matching (artist, album) pairs.
    pub fn search(&self, query: &str) -> Vec<(String, String)> {
        let Ok(index) = self.index.read() else { return Vec::new(); };
        let tokens: Vec<String> = tokenize(query);
        if tokens.is_empty() { return Vec::new(); }

        let mut result_sets: Vec<HashSet<usize>> = tokens.iter()
            .filter_map(|t| index.get(t))
            .map(|v| v.iter().copied().collect())
            .collect();

        if result_sets.is_empty() { return Vec::new(); }

        // Intersect all token result sets
        let mut results: HashSet<usize> = result_sets.remove(0);
        for s in &result_sets {
            results = results.intersection(s).copied().collect();
        }

        results.iter().filter_map(|&i| self.albums.get(i).cloned()).collect()
    }
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_string())
        .filter(|t| !t.is_empty())
        .collect()
}
