//! Album grid types, layout constants, and positioning logic. Thread: UI (GTK main).
//! Pure coordinate math — no GTK signals or event handling.

use crate::mpd::AlbumMeta;
use gtk4::prelude::*;

/// Album grid item with optional group overlay badge on first item of each group.
/// Album cell in the GtkLayout coordinate-based grid.
/// Each cell holds its AlbumCoverCell widget, an optional group caption label,
/// and metadata for repositioning and filtering.
#[derive(Clone)]
pub(crate) struct AlbumCell {
    pub cell: crate::ui::widgets::AlbumCoverCell,
    pub caption: Option<gtk4::Label>,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    pub group_value: Option<String>,
}

/// Backing store for the album grid — ordered Vec of AlbumCell.
pub(crate) type AlbumCells = std::rc::Rc<std::cell::RefCell<Vec<AlbumCell>>>;

/// Slot constants for grid layout.
pub(crate) const CELL_SLOT_W: f64 = 216.0; // 200 cover + 16 padding/margin
pub(crate) const CELL_SLOT_H: f64 = 258.0; // 200 cover + 50 metadata + 8 gap
pub(crate) const CAPTION_H: f64 = 32.0;

/// Generate a solid-color placeholder texture from an artist name.
pub(crate) fn placeholder_texture(artist: &str) -> gdk4::Texture {
    let (r, g, b) = placeholder_rgb(artist);
    let r8 = (r * 255.0) as u8;
    let g8 = (g * 255.0) as u8;
    let b8 = (b * 255.0) as u8;
    let img = image::RgbImage::from_pixel(200, 200, image::Rgb([r8, g8, b8]));
    let raw = img.into_raw();
    let rgba: Vec<u8> = raw.chunks(3)
        .flat_map(|chunk| [chunk[0], chunk[1], chunk[2], 255u8])
        .collect();
    let bytes = glib::Bytes::from_owned(rgba);
    gdk4::MemoryTexture::new(200, 200, gdk4::MemoryFormat::R8g8b8a8, &bytes, 200 * 4).into()
}

pub(crate) fn make_placeholder_cover() -> Option<gdk4::Texture> {
    let img = image::RgbImage::from_pixel(200, 200, image::Rgb([0x55u8, 0x55, 0x55]));
    let raw = img.into_raw();
    let rgba: Vec<u8> = raw.chunks(3)
        .flat_map(|chunk| [chunk[0], chunk[1], chunk[2], 255u8])
        .collect();
    let bytes = glib::Bytes::from_owned(rgba);
    let tex: gdk4::Texture = gdk4::MemoryTexture::new(200, 200, gdk4::MemoryFormat::R8g8b8a8, &bytes, 200 * 4).into();
    Some(tex)
}

fn placeholder_rgb(artist: &str) -> (f64, f64, f64) {
    let hash: u64 = artist.bytes().fold(0xcbf29ce484222325u64, |acc, b| {
        (acc ^ (b as u64)).wrapping_mul(0x100000001b3)
    });
    let h = ((hash & 0xFF) as f64) / 255.0;
    let s = 0.35_f64;
    let l = 0.55_f64;
    let c = (1.0_f64 - (2.0_f64 * l - 1.0_f64).abs()) * s;
    let x = c * (1.0_f64 - ((h * 6.0_f64) % 2.0_f64 - 1.0_f64).abs());
    let m = l - c / 2.0_f64;
    let (r1, g1, b1) = if h < 1.0 / 6.0 { (c, x, 0.0) }
        else if h < 2.0 / 6.0 { (x, c, 0.0) }
        else if h < 3.0 / 6.0 { (0.0, c, x) }
        else if h < 4.0 / 6.0 { (0.0, x, c) }
        else if h < 5.0 / 6.0 { (x, 0.0, c) }
        else { (c, 0.0, x) };
    (r1 + m, g1 + m, b1 + m)
}

/// Reposition all album cells and group captions within the GtkLayout.
/// Called on library load, group switch, window resize, and search filter change.
pub(crate) fn reposition(layout: &gtk4::Fixed, cells: &[AlbumCell], width: f64) {
    let cols = ((width / CELL_SLOT_W).floor() as usize).max(1);
    let mut x: usize = 0;
    let mut y: f64 = 0.0;
    let mut current_group: Option<&str> = None;

    for item in cells {
        if !item.cell.is_visible() {
            continue;
        }

        let group_start = item.group_value.as_deref() != current_group;
        if group_start {
            if x > 0 {
                y += CELL_SLOT_H;
            }
            x = 0;
            current_group = item.group_value.as_deref();
            if let Some(ref caption) = item.caption {
                caption.set_visible(true);
                layout.move_(caption, 0.0, y);
                y += CAPTION_H;
            }
        } else if let Some(ref caption) = item.caption {
            caption.set_visible(false);
        }

        layout.move_(&item.cell, x as f64 * CELL_SLOT_W, y);
        x += 1;
        if x >= cols {
            x = 0;
            y += CELL_SLOT_H;
        }
    }

    if x > 0 {
        y += CELL_SLOT_H;
    }
    layout.set_size_request(width as i32, y as i32);
}

/// Compute the group caption for an album based on the active view mode.
pub(crate) fn group_caption_for_view(view_mode: &str, header: &str, meta: &AlbumMeta) -> Option<Vec<String>> {
    crate::presenters::browse::album_grid::group_caption(view_mode, header, meta).map(|gc| match gc {
        crate::presenters::types::GroupCaption::Header(h) => vec![h],
        crate::presenters::types::GroupCaption::TrackArtists(v) => v,
    })
}

pub(crate) fn format_year_badge(year: Option<&str>) -> Option<String> {
    crate::presenters::format::year_badge(year)
}
