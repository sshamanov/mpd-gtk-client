//! Shared non-GTK presentation types — the bridge between presenters and UI widgets.

/// Display-ready format badge (e.g., "24/96 · FLAC", "DSD128").
#[derive(Debug, Clone, PartialEq)]
pub struct FormatBadge(pub String);

/// Display-ready year badge (e.g., "'84" or "2024").
#[derive(Debug, Clone, PartialEq)]
pub struct YearBadge(pub Option<String>);

/// Group header caption — either a single header or a list of distinct track artists.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupCaption {
    Header(String),
    TrackArtists(Vec<String>),
}

/// Grid cell coordinate — computed row/col + pixel position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCell {
    pub row: usize,
    pub col: usize,
    pub x: f64,
    pub y: f64,
}

