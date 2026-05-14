//! Local in-memory search index — fast full-text search without MPD round-trips.
//! Thread: Search worker thread (index builds + queries), GTK thread (results display).
//!
//! After story 28-3, the `SearchIndex` is owned by the search worker thread.
//! The GTK thread sends `SearchCommand` via channel and receives results as
//! `MpdEvent::SearchResults`. The index is never locked or accessed from GTK.

pub mod worker;

pub use worker::{SearchCommand, SearchCommandSender};

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
        if let Ok(mut guard) = self.index.write() {
            *guard = idx;
        } else {
            log::error!("[search] RwLock poisoned (build), search index not updated");
        }
    }

    /// Return the number of indexed albums.
    pub fn album_count(&self) -> usize { self.albums.len() }

    /// Search the local index with relevance scoring.
    /// Returns (artist, album, score) sorted by score descending (stable for ties).
    /// Results below MIN_SCORE are excluded.
    pub fn search(&self, query: &str) -> Vec<(String, String, u32)> {
        const MIN_SCORE: u32 = 20;
        const MAX_RESULTS: usize = 200;

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

        // Score each result
        let mut scored: Vec<(String, String, u32)> = results.iter()
            .filter_map(|&i| {
                let (artist, album) = self.albums.get(i)?;
                let score = score_album(query, artist, album);
                if score < MIN_SCORE { return None; }
                Some((artist.clone(), album.clone(), score))
            })
            .collect();

        // Stable sort by score descending (maintain original order for ties)
        scored.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| {
            let a_idx = self.albums.iter().position(|(ar, al)| ar == &a.0 && al == &a.1);
            let b_idx = self.albums.iter().position(|(ar, al)| ar == &b.0 && al == &b.1);
            a_idx.cmp(&b_idx)
        }));

        // Truncate to max results
        scored.truncate(MAX_RESULTS);
        scored
    }
}

/// Score an album against a search query using the PRD weighting scheme.
///
/// - Exact album title match: 100 points
/// - Exact artist match: 80 points
/// - Partial album title match: 60 points x match%
/// - Partial artist match: 40 points x match%
/// - Year/genre exact match: 20 points (not computed — no year/genre in index)
/// - Track title match: 30 points (not computed — no track index here)
pub fn score_album(query: &str, artist: &str, album: &str) -> u32 {
    let q = query.to_lowercase();
    let artist_lower = artist.to_lowercase();
    let album_lower = album.to_lowercase();

    let mut score: u32 = 0;

    // Exact album title match: 100
    if album_lower == q {
        score += 100;
    }

    // Exact artist match: 80
    if artist_lower == q {
        score += 80;
    }

    // Partial album title match: 60 x match%
    if album_lower.contains(&q) || q.contains(&album_lower) {
        let pct = match_pct(&q, &album_lower);
        score += (60.0 * pct) as u32;
    }

    // Partial artist match: 40 x match%
    if artist_lower.contains(&q) || q.contains(&artist_lower) {
        let pct = match_pct(&q, &artist_lower);
        score += (40.0 * pct) as u32;
    }

    score
}

/// Score a folder entry against a search query.
pub fn score_folder(query: &str, filename: &str, folder_path: &str, extension: &str) -> u32 {
    let q = query.to_lowercase();
    let fname = filename.to_lowercase();
    let fpath = folder_path.to_lowercase();

    let mut score: u32 = 0;

    // Exact filename match: 100
    if fname == q {
        score += 100;
    }

    // Exact folder name match: 80
    if fpath.ends_with(&format!("/{q}")) || fpath == q {
        score += 80;
    }

    // Partial path match: 50 x match%
    if fpath.contains(&q) || q.contains(&fpath) {
        let pct = match_pct(&q, &fpath);
        score += (50.0 * pct) as u32;
    }

    // File extension match: 10
    if !extension.is_empty() && q.contains(extension) {
        score += 10;
    }

    score
}

/// Compute match percentage: character-level overlap ratio.
fn match_pct(query: &str, target: &str) -> f64 {
    if query.is_empty() && target.is_empty() {
        return 1.0;
    }
    if query.is_empty() || target.is_empty() {
        return 0.0;
    }

    let q = query.to_lowercase();
    let t = target.to_lowercase();

    // Count characters from query that appear in order within target
    let mut ti = t.chars();
    let matched: usize = q.chars().filter(|qc| ti.any(|tc| tc == *qc)).count();

    let max_len = q.len().max(t.len());
    if max_len == 0 {
        return 0.0;
    }

    2.0 * matched as f64 / (q.len() + t.len()) as f64
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_string())
        .filter(|t| !t.is_empty())
        .collect()
}
