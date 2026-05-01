//! Album cover widget helpers — grid cells with cover art, hover buttons, labels. Thread: UI (GTK main loop).

use crate::mpd::state_machine::MpdCommand;
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{Box, Button, DrawingArea, EventControllerKey, Label, Orientation, Overlay, Picture};
use std::path::Path;
use std::sync::mpsc;

const COVER_SIZE: i32 = 200;

/// Create an album cover grid cell with hover action buttons.
/// `cmd_tx` is used to wire button clicks to the MPD background thread.
pub fn create_album_cover(
    album_id: &str,
    album_name: &str,
    title: &str,
    artist: &str,
    cover_path: Option<&str>,
    cover_widgets: &std::cell::RefCell<std::collections::HashMap<String, gtk4::Picture>>,
    cmd_tx: mpsc::Sender<MpdCommand>,
) -> Box {
    let container = Box::new(Orientation::Vertical, 0);
    container.set_size_request(200, 250);
    container.set_widget_name(album_id);
    container.set_css_classes(&["album-cover-cell"]);

    // Overlay: cover area underneath, hover buttons on top
    let overlay = Overlay::new();

    // Cover area
    let cover_area = Box::new(Orientation::Vertical, 0);
    cover_area.set_size_request(COVER_SIZE, COVER_SIZE);

    let (r, g, b) = placeholder_rgb(artist);
    let placeholder = DrawingArea::new();
    placeholder.set_size_request(COVER_SIZE, COVER_SIZE);
    placeholder.set_draw_func(move |_area, cr, _width, _height| {
        cr.set_source_rgb(r, g, b);
        let _ = cr.paint();
    });
    cover_area.append(&placeholder);

    let cover_image = Picture::new();
    cover_image.set_widget_name("cover-image");
    cover_image.set_size_request(COVER_SIZE, COVER_SIZE);
    cover_image.set_halign(gtk4::Align::Center);
    cover_image.set_valign(gtk4::Align::Center);
    let has_cover = cover_path.is_some();
    if let Some(path) = cover_path {
        log::info!("[cover] display: '{title}' -> {path}");
        cover_image.set_filename(Some(path));
    }
    cover_image.set_visible(has_cover);
    placeholder.set_visible(!has_cover);
    // Register for in-place updates when covers arrive asynchronously
    cover_widgets.borrow_mut().insert(album_name.to_string(), cover_image.clone());
    cover_area.append(&cover_image);

    overlay.set_child(Some(&cover_area));

    // Hover buttons: bottom-right, hidden by default, keyboard-accessible via focus
    let an = album_name.to_string();
    let tx_add = cmd_tx.clone();
    let btn_add = Button::with_label("+");
    btn_add.set_css_classes(&["album-cover-hover-btn"]);
    btn_add.set_tooltip_text(Some("Add to queue"));
    btn_add.set_can_focus(true);
    btn_add.connect_clicked(move |_| {
        let _ = tx_add.send(MpdCommand::Add(an.clone()));
    });

    let an = album_name.to_string();
    let tx_next = cmd_tx.clone();
    let btn_next = Button::with_label("←");
    btn_next.set_css_classes(&["album-cover-hover-btn"]);
    btn_next.set_tooltip_text(Some("Play next"));
    btn_next.set_can_focus(true);
    btn_next.connect_clicked(move |_| {
        let _ = tx_next.send(MpdCommand::InsertNext(an.clone()));
    });

    let an = album_name.to_string();
    let tx_play = cmd_tx.clone();
    let btn_play = Button::with_label(">");
    btn_play.set_css_classes(&["album-cover-hover-btn"]);
    btn_play.set_tooltip_text(Some("Clear queue and play"));
    btn_play.set_can_focus(true);
    btn_play.connect_clicked(move |_| {
        let _ = tx_play.send(MpdCommand::PlayAlbum(an.clone()));
    });

    let btn_box = Box::new(Orientation::Horizontal, 2);
    btn_box.set_halign(gtk4::Align::End);
    btn_box.set_valign(gtk4::Align::End);
    btn_box.set_margin_bottom(4);
    btn_box.set_margin_end(4);
    btn_box.append(&btn_add);
    btn_box.append(&btn_next);
    btn_box.append(&btn_play);
    overlay.add_overlay(&btn_box);
    btn_box.set_sensitive(false);

    // Toggle button sensitivity on hover (opacity is cosmetic, sensitivity prevents clicks)
    let btn_sense = btn_box.clone();
    let motion = gtk4::EventControllerMotion::new();
    let btn_sense_e = btn_sense.clone();
    motion.connect_enter(move |_motion, _x, _y| { btn_sense.set_sensitive(true); });
    motion.connect_leave(move |_motion| { btn_sense_e.set_sensitive(false); });
    container.add_controller(motion);

    // Keyboard activation: Enter/Space on the container triggers play
    container.set_can_focus(true);
    let key_an = album_name.to_string();
    let key_tx = cmd_tx.clone();
    let key_ctrl = EventControllerKey::new();
    key_ctrl.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key == Key::Return || key == Key::KP_Enter || key == Key::space {
            let _ = key_tx.send(MpdCommand::PlayAlbum(key_an.clone()));
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    container.add_controller(key_ctrl);

    container.append(&overlay);

    // Title
    let title_label = Label::new(Some(title));
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_max_width_chars(18);
    title_label.set_lines(1);
    container.append(&title_label);

    // Artist
    let artist_label = Label::new(Some(artist));
    artist_label.set_halign(gtk4::Align::Start);
    artist_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    artist_label.set_max_width_chars(18);
    artist_label.set_lines(1);
    container.append(&artist_label);

    container
}

/// Set the cover image (for Epic 4b cover loading).
#[allow(dead_code)]
pub fn set_cover_path(container: &Box, path: &Path) {
    if let Some(overlay) = container.first_child().and_then(|c| c.downcast::<Overlay>().ok()) {
        if let Some(cover_area) = overlay.child().and_then(|c| c.downcast::<Box>().ok()) {
            if let Some(drawing) = cover_area.first_child() {
                drawing.set_visible(false);
            }
            if let Some(sibling) = cover_area.first_child().and_then(|c| c.next_sibling()) {
                if let Ok(pic) = sibling.downcast::<Picture>() {
                    pic.set_filename(Some(path));
                    pic.set_visible(true);
                }
            }
        }
    }
}

fn placeholder_rgb(artist: &str) -> (f64, f64, f64) {
    let hash: u64 = artist.bytes().fold(0xcbf29ce484222325u64, |acc, b| {
        (acc ^ (b as u64)).wrapping_mul(0x100000001b3)
    });
    let h = ((hash & 0xFF) as f64) / 255.0;
    let s = 0.35_f64;
    let l = 0.55_f64;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if h < 1.0 / 6.0 {
        (c, x, 0.0)
    } else if h < 2.0 / 6.0 {
        (x, c, 0.0)
    } else if h < 3.0 / 6.0 {
        (0.0, c, x)
    } else if h < 4.0 / 6.0 {
        (0.0, x, c)
    } else if h < 5.0 / 6.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (r1 + m, g1 + m, b1 + m)
}
