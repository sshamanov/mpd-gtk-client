//! Mini queue grid types and layout building.

use crate::mpd::state_machine::{CommandSender, MpdCommand};
use gtk4::prelude::*;
use gtk4::{Box, Orientation, Picture};
use std::collections::HashMap;

/// Item in the mini queue grid (album-level grouping).
#[derive(Clone)]
pub(crate) struct MiniGridItem {
    pub album: String,
    pub artist: String,
    pub first_pos: i32,
}

/// Backing data store for the mini queue grid.
pub(crate) type MiniGridData = std::rc::Rc<std::cell::RefCell<Vec<MiniGridItem>>>;

/// Positioned cell in the mini queue Fixed layout — used for drag hit-testing.
pub(crate) struct MiniCell {
    pub x: f64,
    pub y: f64,
    pub w: i32,
    pub h: i32,
    pub container: Box,
    pub _album_key: String,
}

pub(crate) type SharedIds = std::rc::Rc<std::cell::RefCell<HashMap<i32, i32>>>;

/// Rebuild the GtkFixed mini-queue layout from item data.
/// Returns positioned cell rects for drag hit-testing.
pub(crate) fn rebuild_mini_fixed(
    fixed: &gtk4::Fixed,
    items: &[MiniGridItem],
    cover_paths: &std::rc::Rc<std::cell::RefCell<HashMap<String, Option<String>>>>,
    cover_widgets: &std::rc::Rc<std::cell::RefCell<HashMap<String, Picture>>>,
    current_album: &Option<String>,
    cmd_tx: &CommandSender,
    viewport_width: i32,
) -> Vec<MiniCell> {
    let mut cells = Vec::new();

    while let Some(child) = fixed.first_child() {
        fixed.remove(&child);
    }

    if items.is_empty() {
        fixed.set_size_request(-1, 0);
        return cells;
    }

    let cell_size: i32 = 90;
    let spacing: i32 = 8;
    let step = cell_size + spacing;
    let cols = if viewport_width > 0 {
        ((viewport_width + spacing) / step).max(1) as usize
    } else {
        3
    };

    let total_row_width = (cols as i32 * step - spacing).max(0);
    let offset_x = ((viewport_width - total_row_width) / 2).max(0) as f64;

    for (i, item) in items.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = offset_x + (col as i32 * step) as f64;
        let y = (row as i32 * step) as f64;

        let container = Box::new(Orientation::Vertical, 0);
        container.set_size_request(cell_size, cell_size);

        let cover = Picture::new();
        cover.set_size_request(cell_size, cell_size);
        cover.set_content_fit(gtk4::ContentFit::Cover);
        cover.set_css_classes(&["mini-queue-cover"]);
        container.append(&cover);

        let key = crate::coverart::cover_key(&item.artist, &item.album);
        if let Some(path) = cover_paths.borrow().get(&key).and_then(|o| o.as_deref()) {
            cover.set_filename(Some(path));
            cover.set_visible(true);
        } else {
            cover.set_visible(false);
        }
        cover_widgets.borrow_mut().insert(key.clone(), cover.clone());

        let tooltip = if item.artist.is_empty() {
            item.album.clone()
        } else {
            format!("{} - {}", item.artist, item.album)
        };
        container.set_tooltip_text(Some(&tooltip));

        let is_current = current_album.as_deref() == Some(&item.album);
        if is_current {
            container.set_css_classes(&["mini-queue-cell", "mini-queue-current"]);
        } else {
            container.set_css_classes(&["mini-queue-cell"]);
        }

        let tx = cmd_tx.clone();
        let pos = item.first_pos;
        let dbl = gtk4::GestureClick::new();
        dbl.set_button(1);
        dbl.connect_pressed(move |_gest, n_clicks, _x, _y| {
            if n_clicks == 2 {
                let _ = tx.send(MpdCommand::PlayPosition(pos));
            }
        });
        container.add_controller(dbl);

        cells.push(MiniCell {
            x,
            y,
            w: cell_size,
            h: cell_size,
            container: container.clone(),
            _album_key: key,
        });
        fixed.put(&container, x, y);
    }

    let n_rows = (items.len() + cols - 1) / cols;
    let total_h = (n_rows as i32 * step).max(1);
    fixed.set_size_request(-1, total_h);

    cells
}
