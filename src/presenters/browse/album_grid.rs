//! Album grid presenter — pure coordinate mapping and group caption computation.
//!
//! No GTK types. Returns `GridCell` positions and `GroupCaption` labels.
//! The UI layer applies these via GTK `.move_()` calls.

use crate::mpd::AlbumMeta;
use crate::presenters::types::{GridCell, GroupCaption};

/// Slot width for an album cover cell.
pub const CELL_SLOT_W: f64 = 170.0;
/// Slot height for an album cover cell (cover + label).
pub const CELL_SLOT_H: f64 = 192.0;
/// Height reserved for group caption header row.
pub const CAPTION_H: f64 = 28.0;

/// Compute grid positions for a sequence of cells.
///
/// `cell_count` — number of visible cells to lay out.
/// `width` — available pixel width.
/// `captions` — per-cell caption (aligned with cells), None = no caption.
///
/// Returns a `Vec<GridCell>` with row, column, and pixel x/y positions.
pub fn compute_grid_layout(
    cell_count: usize,
    width: f64,
    captions: &[Option<GroupCaption>],
) -> Vec<GridCell> {
    let cols = ((width / CELL_SLOT_W).floor() as usize).max(1);
    let mut cells = Vec::with_capacity(cell_count);
    let mut x: usize = 0;
    let mut y: f64 = 0.0;

    for i in 0..cell_count {
        let caption = captions.get(i).and_then(|c| c.as_ref());
        if caption.is_some() {
            y += CAPTION_H;
            x = 0;
        }

        cells.push(GridCell {
            row: (y / CELL_SLOT_H) as usize,
            col: x,
            x: x as f64 * CELL_SLOT_W,
            y,
        });

        x += 1;
        if x >= cols {
            x = 0;
            y += CELL_SLOT_H;
        }
    }

    cells
}

/// Compute total layout height for a given cell count and width.
pub fn layout_height(cell_count: usize, width: f64, captions: &[Option<GroupCaption>]) -> f64 {
    let cols = ((width / CELL_SLOT_W).floor() as usize).max(1);
    let mut x: usize = 0;
    let mut y: f64 = 0.0;

    for i in 0..cell_count {
        let caption = captions.get(i).and_then(|c| c.as_ref());
        if caption.is_some() {
            y += CAPTION_H;
            x = 0;
        }
        x += 1;
        if x >= cols {
            x = 0;
            y += CELL_SLOT_H;
        }
    }
    if x > 0 {
        y += CELL_SLOT_H;
    }
    y
}

/// Compute group caption for an album based on the active view mode.
pub fn group_caption(view_mode: &str, header: &str, meta: &AlbumMeta) -> Option<GroupCaption> {
    match view_mode {
        "Albums" => None,
        "Years" => None,
        "Genres" => Some(GroupCaption::Header(header.to_string())),
        "Artists" => {
            let aa = meta.album_artist.to_lowercase();
            let mut seen = std::collections::HashSet::new();
            let unique: Vec<String> = meta
                .track_artists
                .iter()
                .filter(|a| a.to_lowercase() != aa)
                .filter(|a| seen.insert(a.to_lowercase()))
                .take(6)
                .map(|s| s.to_string())
                .collect();
            if unique.is_empty() {
                None
            } else {
                Some(GroupCaption::TrackArtists(unique))
            }
        }
        _ => None,
    }
}
