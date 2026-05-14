//! UI layer — GTK4 widgets and window management. Thread: UI (GTK main loop).

pub mod widgets;

use crate::ui::widgets::AlbumCoverCell;
use crate::config::Config;
use crate::mpd::state_machine::{CommandSender, MpdCommand, MpdEvent, PlaybackUpdate};
use crate::mpd::AlbumMeta;
use crate::search::{SearchCommand, SearchCommandSender};
use crate::state::SharedState;
use gtk4::prelude::*;
use adw::prelude::*;
use gtk4::{Application, Box, DragSource, DropTarget, EventControllerKey, Fixed, Label, ListBox, Orientation, Picture, ScrolledWindow};
use gtk4::gdk::{ContentProvider, DragAction};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Album grid item with optional group overlay badge on first item of each group.
#[derive(Clone)]
/// Album cell in the GtkLayout coordinate-based grid.
/// Each cell holds its AlbumCoverCell widget, an optional group caption label,
/// and metadata for repositioning and filtering.
struct AlbumCell {
    cell: AlbumCoverCell,
    caption: Option<gtk4::Label>,
    artist: String,
    album: String,
    album_id: String,
    /// Group label value for detecting group boundaries in reposition().
    group_value: Option<String>,
}

/// Backing store for the album grid — ordered Vec of AlbumCell.
type AlbumCells = std::rc::Rc<std::cell::RefCell<Vec<AlbumCell>>>;

/// Item in the mini queue grid (album-level grouping).
#[derive(Clone)]
struct MiniGridItem {
    album: String,
    artist: String,
    first_pos: i32,
}

/// Backing data store for the mini queue grid.
type MiniGridData = std::rc::Rc<std::cell::RefCell<Vec<MiniGridItem>>>;

/// Positioned cell in the mini queue Fixed layout — used for drag hit-testing.
struct MiniCell {
    x: f64,
    y: f64,
    w: i32,
    h: i32,
    container: gtk4::Box,
    _album_key: String,
}

/// Rebuild the GtkFixed mini-queue layout from item data.
/// Returns positioned cell rects for drag hit-testing.
fn rebuild_mini_fixed(
    fixed: &gtk4::Fixed,
    items: &[MiniGridItem],
    cover_paths: &std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Option<String>>>>,
    cover_widgets: &std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, gtk4::Picture>>>,
    current_album: &Option<String>,
    cmd_tx: &CommandSender,
    viewport_width: i32,
) -> Vec<MiniCell> {
    let mut cells = Vec::new();

    // Clear all children
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
        3 // default before first allocation
    };

    // Center the grid row horizontally within the viewport
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

        // Double-click → play album at first queue position
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


/// Given a Picture widget inside a cover_overlay (Overlay), find and hide
/// the placeholder DrawingArea overlay that sits on top of it.
/// Generate a solid-color placeholder texture from an artist name.
fn placeholder_texture(artist: &str) -> gdk4::Texture {
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

fn make_placeholder_cover() -> Option<gdk4::Texture> {
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

// ── GtkLayout coordinate-based grid ──

/// Slot constants for grid layout.
const CELL_SLOT_W: f64 = 216.0; // 200 cover + 16 padding/margin
const CELL_SLOT_H: f64 = 250.0; // 200 cover + 50 metadata
const CAPTION_H: f64 = 32.0;

/// Reposition all album cells and group captions within the GtkLayout.
/// Called on library load, group switch, window resize, and search filter change.
fn reposition(layout: &gtk4::Fixed, cells: &[AlbumCell], width: f64) {
    let cols = ((width / CELL_SLOT_W).floor() as usize).max(1);
    let mut x: usize = 0;
    let mut y: f64 = 0.0;
    let mut current_group: Option<&str> = None;

    for item in cells {
        // Hide cells not matching search filter
        if !item.cell.is_visible() {
            continue;
        }

        let group_start = item.group_value.as_deref() != current_group;
        if group_start {
            current_group = item.group_value.as_deref();
            if let Some(ref caption) = item.caption {
                caption.set_visible(true);
                layout.move_(caption, 0.0, y);
                y += CAPTION_H;
            }
            x = 0;
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
/// - Albums (flat): no caption
/// - Years / Genres: every album gets the group header as caption
/// - Artists: show track artists that differ from the AlbumArtist
fn group_caption_for_view(view_mode: &str, header: &str, meta: &AlbumMeta) -> Option<Vec<String>> {
    crate::presenters::browse::album_grid::group_caption(view_mode, header, meta).map(|gc| match gc {
        crate::presenters::types::GroupCaption::Header(h) => vec![h],
        crate::presenters::types::GroupCaption::TrackArtists(v) => v,
    })
}

fn format_year_badge(year: Option<&str>) -> Option<String> {
    crate::presenters::format::year_badge(year)
}

type SharedIds = std::rc::Rc<std::cell::RefCell<HashMap<i32, i32>>>;

/// High-contrast CSS overrides for WCAG 2.1 AA compliance.
/// Applied at PRIORITY_APPLICATION + 1 to override the base theme.
const HC_CSS: &str = "\
@define-color theme_bg_color #000000;
@define-color theme_fg_color #ffffff;
@define-color theme_base_color #000000;
@define-color theme_text_color #ffffff;
@define-color theme_selected_bg_color #4A90D9;
@define-color theme_selected_fg_color #ffffff;
@define-color theme_unfocused_bg_color #000000;
@define-color theme_unfocused_fg_color #ffffff;
@define-color theme_unfocused_base_color #000000;
@define-color theme_unfocused_text_color #ffffff;
@define-color borders #ffffff;
@define-color theme_link_color #7AB5F5;
@define-color error_color #FF6B6B;
@define-color warning_color #FFD93D;
@define-color success_color #6BCB77;
#connection-indicator.disconnected { background-color: #FF6B6B; }
#connection-indicator.connected { background-color: #6BCB77; }
#connection-indicator.connecting { background-color: #FFD93D; }
.queue-artist { color: #CCCCCC; }
.format-badge { color: #CCCCCC; }
.error-label { color: #FF6B6B; }
#album-grid-status { color: #CCCCCC; }
.breadcrumb-sep { color: #CCCCCC; }
.queue-current { background-color: #4A90D9; }
";

pub struct App {
    state: SharedState,
    event_rx: Arc<Mutex<mpsc::Receiver<MpdEvent>>>,
    cmd_tx: CommandSender,
    conn_params: Arc<Mutex<crate::mpd::ConnectionTarget>>,
    /// Sender for MPRIS PropertiesChanged signal emissions (drops unused when !mpris feature).
    mpris_update_tx: mpsc::Sender<crate::mpd::state_machine::PlaybackUpdate>,
    metadata_cache: std::sync::Arc<crate::metadata::MetadataCache>,
    /// Sender for search worker commands (story 28-3).
    search_cmd_tx: SearchCommandSender,
    /// Sender for NotificationRouter — forwards MpdEvent clones for desktop notification dispatch.
    toast_tx: mpsc::SyncSender<MpdEvent>,
}

impl App {
    pub fn new(
        state: SharedState,
        event_rx: mpsc::Receiver<MpdEvent>,
        cmd_tx: CommandSender,
        conn_params: Arc<Mutex<crate::mpd::ConnectionTarget>>,
        mpris_update_tx: mpsc::Sender<crate::mpd::state_machine::PlaybackUpdate>,
        metadata_cache: std::sync::Arc<crate::metadata::MetadataCache>,
        search_cmd_tx: SearchCommandSender,
        toast_tx: mpsc::SyncSender<MpdEvent>,
    ) -> Self {
        Self {
            state,
            event_rx: Arc::new(Mutex::new(event_rx)),
            cmd_tx,
            conn_params,
            mpris_update_tx,
            metadata_cache,
            search_cmd_tx,
            toast_tx,
        }
    }

    pub fn run(&self) {
        let application = Application::builder()
            .application_id("com.github.schaman.mpd-client")
            .build();

        let event_rx = self.event_rx.clone();
        let cmd_tx = self.cmd_tx.clone();
        let state = self.state.clone();
        let conn_params = self.conn_params.clone();

        // Clone for use inside connect_activate (to avoid capturing application itself)
        let app_clone = application.clone();

        // Register Ctrl+Q quit action
        let quit_action = gtk4::gio::SimpleAction::new("quit", None);
        let app_handle = app_clone.clone();
        quit_action.connect_activate(move |_, _| {
            app_handle.quit();
        });
        application.add_action(&quit_action);
        application.set_accels_for_action("app.quit", &["<Ctrl>Q"]);

        // Mode switching actions

        let mpris_update_tx = self.mpris_update_tx.clone();
        let metadata_cache = self.metadata_cache.clone();
        let search_cmd_tx = self.search_cmd_tx.clone();
        let toast_tx = self.toast_tx.clone();

        application.connect_activate(move |window_app| {
            // Clone early for the shutdown timer closure; window_app is consumed by the builder below.
            let shutdown_app = window_app.clone();
            let cfg = Config::load();

            let window = adw::ApplicationWindow::builder()
                .application(window_app)
                .default_width(1200)
                .default_height(800)
                .title("MPD Client")
                .show_menubar(false)
                .build();

            // Restore window size from saved config (position is best-effort, ignored on Wayland)
            if let Some(ref geo) = cfg.window_geometry {
                window.set_default_size(geo.width, geo.height);
            }

            // Save window geometry on close (using glib::ObjectExt::set_data pattern)
            {
                let w = window.clone();
                window.connect_close_request(move |_| {
                    let cur_w = w.default_width();
                    let cur_h = w.default_height();
                    let mut c = crate::config::Config::load();
                    c.window_geometry = Some(crate::config::WindowGeometry {
                        width: cur_w.max(1), height: cur_h.max(1),
                        x: 0, y: 0,
                    });
                    let _ = c.save();
                    glib::Propagation::Proceed
                });
            }

            let multi_view = adw::MultiLayoutView::new();

            // --- Left pane: group bar + album grid ---
            let left_pane_box = Box::new(Orientation::Vertical, 0);

            // Group selector bar — linked ToggleButtons, no icons
            let scfg = Config::load();
            let artist_tag = if scfg.use_album_artist { "AlbumArtist" } else { "Artist" };
            let group_pages: [(&str, &str); 4] = [("Albums", "Albums"), ("Artists", artist_tag), ("Years", "Date"), ("Genres", "Genre")];
            let group_buttons = Box::new(Orientation::Horizontal, 0);
            group_buttons.set_css_classes(&["linked"]);
            group_buttons.set_halign(gtk4::Align::Center);
            // Map display name → ToggleButton for programmatic activation during search restore
            let group_btn_map: std::rc::Rc<std::cell::RefCell<HashMap<String, gtk4::ToggleButton>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            // Track the active group tag for search restore
            let active_group: std::rc::Rc<std::cell::RefCell<String>> =
                std::rc::Rc::new(std::cell::RefCell::new("Albums".to_string()));

            let gtx = cmd_tx.clone();
            let mut prev_btn: Option<gtk4::ToggleButton> = None;
            for (i, (display, tag)) in group_pages.iter().enumerate() {
                let btn = gtk4::ToggleButton::with_label(*display);
                if i == 0 {
                    btn.set_active(true);
                }
                let pb = prev_btn.replace(btn.clone());
                if let Some(p) = pb {
                    btn.set_group(Some(&p));
                }
                group_btn_map.borrow_mut().insert(display.to_string(), btn.clone());
                let ag = active_group.clone();
                let tx = gtx.clone();
                let display_str = display.to_string();
                let tag_str = tag.to_string();
                btn.connect_toggled(move |b| {
                    if b.is_active() {
                        *ag.borrow_mut() = display_str.clone();
                        let _ = tx.send(MpdCommand::ListAlbumsGrouped(tag_str.clone()));
                    }
                });
                group_buttons.append(&btn);
            }

            let left_scroll = ScrolledWindow::new();
            left_scroll.set_vexpand(true);
            left_scroll.set_hexpand(true);

            // --- GtkLayout coordinate-based grid ---
            let album_cells: AlbumCells = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let left_stack = gtk4::Stack::new();

            // Shared cover tracking
            let cover_paths: std::rc::Rc<std::cell::RefCell<HashMap<String, Option<String>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            // Texture cache — avoids repeated disk read+decode during scroll
            let cover_texture_cache: std::rc::Rc<std::cell::RefCell<HashMap<String, gdk4::Texture>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));

            // GtkLayout: fixed-position container — all widgets positioned via coordinates
            let album_layout = gtk4::Fixed::new();
            album_layout.set_hexpand(false);
            album_layout.set_vexpand(false);

            // Loading / empty-state labels
            let loading_label = Label::new(Some("Connecting to MPD..."));
            loading_label.set_halign(gtk4::Align::Center);
            loading_label.set_valign(gtk4::Align::Center);
            loading_label.set_widget_name("album-grid-status");
            let empty_label = Label::new(Some("No albums found"));
            empty_label.set_halign(gtk4::Align::Center);
            empty_label.set_valign(gtk4::Align::Center);
            empty_label.set_visible(false);
            empty_label.set_widget_name("album-grid-status");

            left_scroll.set_child(Some(&album_layout));
            left_stack.add_child(&loading_label);
            left_stack.add_child(&empty_label);
            left_stack.add_child(&left_scroll);
            left_stack.set_visible_child(&loading_label);

            // Resize: reposition all cells when scrolled window width changes
            let resize_sw = left_scroll.clone();
            let resize_cells = album_cells.clone();
            let resize_layout = album_layout.clone();
            resize_sw.connect_notify_local(Some("width"), move |sw, _| {
                let cells = resize_cells.borrow();
                reposition(&resize_layout, &cells, sw.width() as f64);
            });
            // Album mode content box (group bar + search + grid)
            let album_content = Box::new(Orientation::Vertical, 0);
            album_content.set_vexpand(true);
            album_content.set_hexpand(true);
            album_content.set_widget_name("album-content");

            // Folder mode content box (folder tree)
            let folder_content = Box::new(Orientation::Vertical, 0);
            folder_content.set_vexpand(true);
            folder_content.set_hexpand(true);
            folder_content.set_widget_name("folder-content");

            let folder_browser = std::rc::Rc::new(std::cell::RefCell::new(
                widgets::folder_tree::FolderBrowser::new(cmd_tx.clone())));
            // Folder mode search entry + results overlay
            let folder_search = gtk4::SearchEntry::new();
            folder_search.set_placeholder_text(Some("Search files..."));
            folder_search.set_margin_start(4);
            folder_search.set_margin_end(4);
            folder_search.set_margin_bottom(4);

            let folder_search_list = gtk4::ListBox::new();
            folder_search_list.set_selection_mode(gtk4::SelectionMode::Single);
            let folder_search_results = gtk4::ScrolledWindow::new();
            folder_search_results.set_child(Some(&folder_search_list));
            folder_search_results.set_vexpand(true);
            folder_search_results.set_has_frame(true);
            folder_search_results.set_visible(false);

            folder_content.append(&folder_search);
            folder_content.append(&folder_search_results);
            folder_content.append(&folder_browser.borrow().container.clone());

            // Top bar: Album/Folder switch (icons) + refresh + group switcher — single row
            let top_bar = gtk4::Box::new(Orientation::Horizontal, 8);
            top_bar.set_halign(gtk4::Align::Center);
            top_bar.set_margin_top(4);
            top_bar.set_margin_bottom(4);

            // Album/Folder mode toggle with icons
            let mode_btn_album = gtk4::ToggleButton::new();
            mode_btn_album.set_child(Some(&gtk4::Image::from_icon_name("media-optical")));
            mode_btn_album.set_tooltip_text(Some("Album Mode (Ctrl+1)"));
            let mode_btn_folder = gtk4::ToggleButton::new();
            mode_btn_folder.set_child(Some(&gtk4::Image::from_icon_name("folder")));
            mode_btn_folder.set_tooltip_text(Some("Folder Mode (Ctrl+2)"));
            mode_btn_album.set_group(None::<&gtk4::ToggleButton>);
            mode_btn_folder.set_group(Some(&mode_btn_album));
            mode_btn_album.set_active(true);
            top_bar.append(&mode_btn_album);
            top_bar.append(&mode_btn_folder);

            // Refresh button — rescan MPD library
            let update_btn = gtk4::Button::new();
            update_btn.set_child(Some(&gtk4::Image::from_icon_name("view-refresh")));
            update_btn.set_tooltip_text(Some("Rescan MPD music library"));
            let update_cmd = cmd_tx.clone();
            update_btn.connect_clicked(move |_| {
                let _ = update_cmd.send(MpdCommand::Update);
            });
            top_bar.append(&update_btn);

            // Group switcher (Albums/Artists/Years/Genres) — no icons
            top_bar.append(&group_buttons);

            // Mode stack with Crossfade transition for album/folder switching
            let mode_stack = gtk4::Stack::new();
            mode_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
            mode_stack.set_transition_duration(300);
            mode_stack.add_child(&album_content);
            mode_stack.add_child(&folder_content);
            mode_stack.set_visible_child(&album_content);

            // Top bar + mode stack
            let mode_content = gtk4::Box::new(Orientation::Vertical, 0);
            mode_content.set_vexpand(true);
            mode_content.set_hexpand(true);
            mode_content.append(&top_bar);
            mode_content.append(&mode_stack);

            // Connect toggle buttons to mode switching
            let mba_mode = mode_stack.clone();
            let mba_ac = album_content.clone();
            let mba_state = state.clone();
            let mba_ls = left_scroll.clone();
            let mba_fb = folder_browser.clone();
            let mba_fsc = folder_search_results.clone();
            mode_btn_album.connect_toggled(move |b| {
                if !b.is_active() { return; }
                // Save folder state
                if let Ok(fb) = mba_fb.try_borrow() {
                    if let Ok(mut s) = mba_state.write() {
                        s.folder_browsing.expanded_paths =
                            vec![std::path::PathBuf::from(fb.shared_path.borrow().clone())];
                        s.folder_browsing.scroll_position = fb.container.first_child()
                            .and_then(|first| first.next_sibling())
                            .and_then(|sibling| sibling.downcast::<gtk4::ScrolledWindow>().ok())
                            .map(|sw| sw.vadjustment().value()).unwrap_or(0.0);
                    }
                }
                mba_fsc.set_visible(false);
                // Restore album scroll
                if let Ok(s) = mba_state.read() {
                    let pos = s.album_browsing.scroll_position.1;
                    let adj = mba_ls.vadjustment();
                    adj.set_value(pos.clamp(0.0, adj.upper() - adj.page_size()));
                }
                mba_mode.set_visible_child(&mba_ac);
                if let Ok(mut s) = mba_state.write() { s.mode = crate::state::Mode::Album; }
            });
            let mbf_mode = mode_stack.clone();
            let mbf_fc = folder_content.clone();
            let mbf_state = state.clone();
            let mbf_ls = left_scroll.clone();
            let mbf_cmd_tx = cmd_tx.clone();
            let mbf_sp = folder_browser.borrow().shared_path.clone();
            mode_btn_folder.connect_toggled(move |b| {
                if !b.is_active() { return; }
                // Save album scroll
                if let Ok(mut s) = mbf_state.write() {
                    s.album_browsing.scroll_position.1 = mbf_ls.vadjustment().value();
                }
                mbf_mode.set_visible_child(&mbf_fc);
                // Restore folder
                let folder_path = if let Ok(s) = mbf_state.read() {
                    s.folder_browsing.expanded_paths.last()
                        .map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                } else { String::new() };
                *mbf_sp.borrow_mut() = folder_path.clone();
                let _ = mbf_cmd_tx.send(MpdCommand::ListDirectory(folder_path));
                if let Ok(mut s) = mbf_state.write() { s.mode = crate::state::Mode::Folder; }
            });

            // Mode switching: use stack transitions, save/restore scroll positions, update AppState
            let ms = mode_stack.clone();
            let ac = album_content.clone();
            let mode_state = state.clone();
            let ls_album = left_scroll.clone();
            let fb_save = folder_browser.clone();
            let fs_container_save = folder_search_results.clone();
            let album_mode_act = gtk4::gio::SimpleAction::new("album-mode", None);
            album_mode_act.connect_activate(move |_, _| {
                // Save folder browsing state before switching
                if let Ok(fb) = fb_save.try_borrow() {
                    if let Ok(mut s) = mode_state.write() {
                        s.folder_browsing.expanded_paths =
                            vec![std::path::PathBuf::from(fb.shared_path.borrow().clone())];
                        // Capture actual folder scroll position from the ScrolledWindow child of container
                        let pos = fb.container.first_child()
                            .and_then(|first| first.next_sibling())
                            .and_then(|sibling| sibling.downcast::<gtk4::ScrolledWindow>().ok())
                            .map(|sw| sw.vadjustment().value())
                            .unwrap_or(0.0);
                        s.folder_browsing.scroll_position = pos;
                    }
                }
                // Hide folder search overlay when switching away
                fs_container_save.set_visible(false);
                // Restore album scroll position
                if let Ok(s) = mode_state.read() {
                    let pos = s.album_browsing.scroll_position.1;
                    let adj = ls_album.vadjustment();
                    adj.set_value(pos.clamp(0.0, adj.upper() - adj.page_size()));
                }
                ms.set_visible_child(&ac);
                if let Ok(mut s) = mode_state.write() { s.mode = crate::state::Mode::Album; }
            });
            app_clone.add_action(&album_mode_act);
            app_clone.set_accels_for_action("app.album-mode", &["<Ctrl>1"]);

            let ms2 = mode_stack.clone();
            let fc = folder_content.clone();
            let mode_state2 = state.clone();
            let cmd_tx2 = cmd_tx.clone();
            let ls_save = left_scroll.clone();
            let fsc2 = folder_search_results.clone();
            let fbb2 = folder_browser.clone();
            let sp2 = folder_browser.borrow().shared_path.clone();
            let folder_mode_act = gtk4::gio::SimpleAction::new("folder-mode", None);
            folder_mode_act.connect_activate(move |_, _| {
                // Save album scroll position before hiding
                if let Ok(mut s) = mode_state2.write() {
                    s.album_browsing.scroll_position.1 = ls_save.vadjustment().value();
                }
                ms2.set_visible_child(&fc);
                // Restore folder browsing state
                let folder_path = if let Ok(s) = mode_state2.read() {
                    s.folder_browsing.expanded_paths.last()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else { String::new() };
                *sp2.borrow_mut() = folder_path.clone();
                let _ = cmd_tx2.send(MpdCommand::ListDirectory(folder_path));
                // Reset folder search overlay when switching to folder mode
                fsc2.set_visible(false);
                if let Ok(fbb) = fbb2.try_borrow() {
                    fbb.container.set_visible(true);
                }
                if let Ok(mut s) = mode_state2.write() { s.mode = crate::state::Mode::Folder; }
            });
            app_clone.add_action(&folder_mode_act);
            app_clone.set_accels_for_action("app.folder-mode", &["<Ctrl>2"]);

            left_pane_box.append(&mode_content);

            // Album search entry
            let search_entry = gtk4::SearchEntry::new();
            search_entry.set_placeholder_text(Some("Search albums..."));
            search_entry.set_margin_start(4);
            search_entry.set_margin_end(4);
            search_entry.set_margin_bottom(4);
            search_entry.set_size_request(-1, 32);

            // Ctrl+F focuses the mode-appropriate search entry
            let search_action = gtk4::gio::SimpleAction::new("search", None);
            let se_focus_album = search_entry.clone();
            let se_focus_folder = folder_search.clone();
            let mode_for_search = state.clone();
            search_action.connect_activate(move |_, _| {
                if let Ok(m) = mode_for_search.read() {
                    match m.mode {
                        crate::state::Mode::Folder => { se_focus_folder.grab_focus(); }
                        _ => { se_focus_album.grab_focus(); }
                    }
                } else {
                    se_focus_album.grab_focus();
                }
            });
            app_clone.add_action(&search_action);
            app_clone.set_accels_for_action("app.search", &["<Ctrl>F"]);

            // Track the previously active group for restore on search clear
            let prev_group: std::rc::Rc<std::cell::RefCell<Option<String>>> = std::rc::Rc::new(std::cell::RefCell::new(None));

            // Search generation counter
            let search_gen: std::rc::Rc<std::cell::Cell<u64>> = std::rc::Rc::new(std::cell::Cell::new(0));
            let scmd = search_cmd_tx.clone();
            let ftx = toast_tx.clone();

            // stop_search: Escape or clear button
            let stop_btn_map = group_btn_map.clone();
            let prev_group_stop = prev_group.clone();
            search_entry.connect_stop_search(move |_| {
                // Restore the previously active group, fall back to "Albums"
                let display = prev_group_stop.borrow_mut().take().unwrap_or_else(|| "Albums".into());
                if let Some(btn) = stop_btn_map.borrow().get(&display) {
                    btn.set_active(true);
                }
            });

            // search_changed: debounced with local index check + MPD fallback.
            let se_tx = cmd_tx.clone();
            let se_gen = search_gen.clone();
            let se_group = prev_group.clone();
            let se_active_group = active_group.clone();
            let se_btn_map = group_btn_map.clone();
            let se_scmd = scmd.clone();
            let _se_stack = left_stack.clone();
            let _se_scroll = left_scroll.clone();
            search_entry.connect_search_changed(move |entry| {
                let q = entry.text().to_string();

                if q.is_empty() {
                    if let Some(btn) = se_btn_map.borrow().get("Albums") {
                        btn.set_active(true);
                    }
                    return;
                }

                if se_group.borrow().is_none() {
                    let name = se_active_group.borrow().clone();
                    let tags = ["Albums", "Artists", "Years", "Genres"];
                    if tags.contains(&name.as_str()) {
                        *se_group.borrow_mut() = Some(name);
                    }
                }

                let cur = se_gen.get();
                se_gen.set(cur.wrapping_add(1));
                let this_gen = cur.wrapping_add(1);

                let qc = q.clone();
                let gen_c = se_gen.clone();
                let scmd = se_scmd.clone();
                let tx = se_tx.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(150),
                    move || {
                        if gen_c.get() != this_gen { return; }
                        // Send to search worker (story 28-3): worker emits SearchResults via event_tx
                        scmd.send(SearchCommand::Search(qc.clone(), this_gen));
                        // MPD search fallback (fires unconditionally; local results arrive first)
                        let _ = tx.send(MpdCommand::Search(qc));
                    },
                );
            });

            // --- Folder search handler ---
            let fs_cmd = cmd_tx.clone();
            let fs_browser = folder_browser.clone();
            let fs_results = folder_search_results.clone();
            let fs_results2 = folder_search_results.clone();
            let fs_browser2 = folder_browser.clone();
            folder_search.connect_search_changed(move |entry| {
                let q = entry.text().to_string();
                if q.is_empty() {
                    fs_results.set_visible(false);
                    fs_browser.borrow().container.set_visible(true);
                    return;
                }
                let cmd = fs_cmd.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(150),
                    move || {
                        let _ = cmd.send(MpdCommand::SearchFiles(q));
                    },
                );
            });
            folder_search.connect_stop_search(move |_| {
                fs_results2.set_visible(false);
                fs_browser2.borrow().container.set_visible(true);
            });

            // Stack directly in scroll area (no overlay — pinned header removed)


            // GtkDropTarget for album grid reorder (plain Albums view only)
            let grid_state = state.clone();
            let grid_cells = album_cells.clone();
            let grid_vadj = left_scroll.vadjustment();
            let grid_layout = album_layout.clone();
            let grid_target = DropTarget::new(String::static_type(), DragAction::MOVE);
            grid_target.connect_drop(move |target, value, x, y| {
                // Only allow reorder when ungrouped (no group captions)
                let has_groups = grid_cells.borrow().iter().any(|c| c.group_value.is_some());
                if has_groups {
                    return false;
                }

                if let Ok(s) = value.get::<String>() {
                    if !s.contains(':') {
                        let scroll_top = grid_vadj.value();
                        let adjusted_y = scroll_top + y;
                        let col = (x / CELL_SLOT_W).floor() as usize;
                        let row = (adjusted_y / CELL_SLOT_H).floor() as usize;
                        let grid_width = target.widget().and_then(|w| w.downcast::<gtk4::ScrolledWindow>().ok())
                            .map(|sw| sw.width()).unwrap_or(800);
                        let cols_per_row = std::cmp::max(1, (grid_width as f64 / CELL_SLOT_W) as usize);
                        let target_idx = row.saturating_mul(cols_per_row) + col;

                        let mut cells = grid_cells.borrow_mut();
                        let source_pos = cells.iter().position(|c| c.album == s);
                        if let Some(src) = source_pos {
                            if src == target_idx.min(cells.len().saturating_sub(1)) {
                                return true;
                            }
                            let item = cells.remove(src);
                            let insert_at = if target_idx > src {
                                target_idx.saturating_sub(1).min(cells.len())
                            } else {
                                target_idx.min(cells.len())
                            };
                            cells.insert(insert_at, item);

                            let order: Vec<String> = cells.iter().map(|c| c.album.clone()).collect();
                            if let Ok(mut st) = grid_state.write() {
                                st.album_browsing.custom_album_order = order;
                            }
                            drop(cells);
                            let cells_ref = grid_cells.borrow();
                            reposition(&grid_layout, &cells_ref, grid_width as f64);
                        }
                        return true;
                    }
                }
                false
            });
            left_scroll.add_controller(grid_target);

            album_content.append(&search_entry);
            album_content.append(&left_stack);

            // --- Bottom panel (visible when sidebar hidden on narrow windows) ---
            let bottom_panel = Box::new(Orientation::Horizontal, 8);
            bottom_panel.set_margin_start(8);
            bottom_panel.set_margin_end(8);
            bottom_panel.set_margin_top(4);
            bottom_panel.set_margin_bottom(4);
            bottom_panel.set_size_request(-1, 44);
            bottom_panel.set_vexpand(false);
            bottom_panel.set_css_classes(&["bottom-panel"]);
            bottom_panel.set_visible(false);

            // Bottom track info
            let bp_title = Label::new(Some(""));
            bp_title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            bp_title.set_max_width_chars(24);
            bp_title.set_halign(gtk4::Align::Start);
            bp_title.set_valign(gtk4::Align::Center);
            bp_title.set_css_classes(&["caption"]);

            // Bottom transport
            let bp_prev = gtk4::Button::from_icon_name("media-seek-backward-symbolic");
            bp_prev.set_valign(gtk4::Align::Center);
            let bp_play = gtk4::Button::new();
            bp_play.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
            bp_play.set_valign(gtk4::Align::Center);
            let bp_next = gtk4::Button::from_icon_name("media-seek-forward-symbolic");
            bp_next.set_valign(gtk4::Align::Center);

            let bp_transport = Box::new(Orientation::Horizontal, 2);
            bp_transport.set_halign(gtk4::Align::Center);
            bp_transport.set_hexpand(true);
            bp_transport.append(&bp_prev);
            bp_transport.append(&bp_play);
            bp_transport.append(&bp_next);

            // Bottom mini-grid toggle → reveals mini-grid popover
            let bp_queue_btn = gtk4::Button::from_icon_name("view-grid-symbolic");
            bp_queue_btn.set_valign(gtk4::Align::Center);
            bp_queue_btn.set_tooltip_text(Some("Queue"));

            bottom_panel.append(&bp_title);
            bottom_panel.append(&bp_transport);
            bottom_panel.append(&bp_queue_btn);
            left_pane_box.append(&bottom_panel);

            multi_view.set_child("left", &left_pane_box);

            // --- Right rail ---
            let right_pane = Box::new(Orientation::Vertical, 0);
            right_pane.set_margin_start(4);
            right_pane.set_margin_end(4);

            // Settings gear button with Ctrl+, shortcut
            let settings_btn = gtk4::Button::new();
            settings_btn.set_child(Some(&gtk4::Image::from_icon_name("emblem-system")));
            settings_btn.set_tooltip_text(Some("Settings (Ctrl+,)"));
            let sw = window.clone();
            let btn_clone = settings_btn.clone();
            let cp_save = conn_params.clone();
            let stx = cmd_tx.clone();
            settings_btn.connect_clicked(move |_| {
                let scfg = Config::load();
                let d = gtk4::Window::new();
                d.set_title(Some("Settings"));
                d.set_transient_for(Some(&sw));
                d.set_modal(true);
                let content = gtk4::Box::new(Orientation::Vertical, 8);
                content.set_margin_start(12); content.set_margin_end(12);
                content.set_margin_top(8); content.set_margin_bottom(8);
                let host_entry = gtk4::Entry::new();
                host_entry.set_text(&scfg.mpd_host);
                let port_entry = gtk4::Entry::new();
                port_entry.set_text(&scfg.mpd_port.to_string());
                content.append(&Label::new(Some("MPD Host:")));
                content.append(&host_entry);
                content.append(&Label::new(Some("MPD Port:")));
                content.append(&port_entry);
                let port_error = gtk4::Label::new(Some("Invalid port number"));
                port_error.set_css_classes(&["error-label"]);
                port_error.set_visible(false);
                content.append(&port_error);

                // Profile selector (hidden when ≤1 profile)
                let profile_names: Vec<String> = scfg.profiles.as_ref().map_or_else(Vec::new, |p| {
                    let mut names: Vec<String> = p.keys().cloned().collect();
                    names.sort();
                    names
                });
                let profile_dropdown = gtk4::DropDown::from_strings(
                    &profile_names.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
                );
                profile_dropdown.set_visible(profile_names.len() > 1);
                if profile_names.len() > 1 {
                    content.append(&Label::new(Some("Profile:")));
                    content.append(&profile_dropdown);
                }
                // Pre-select current profile
                if let Some(ref cur) = scfg.default_profile.or(scfg.last_profile) {
                    if let Some(pos) = profile_names.iter().position(|n| n == cur) {
                        profile_dropdown.set_selected(pos as u32);
                    }
                }

                // High contrast toggle
                let hc_check = gtk4::CheckButton::with_label("High Contrast Mode");
                hc_check.set_active(scfg.high_contrast);
                hc_check.set_margin_top(8);
                content.append(&hc_check);

                let auto_start_check = gtk4::CheckButton::with_label("Auto-start on login");
                auto_start_check.set_active(scfg.auto_start);
                auto_start_check.set_margin_top(4);
                content.append(&auto_start_check);

                let artist_check = gtk4::CheckButton::with_label("Group Artists by Album Artist");
                artist_check.set_active(scfg.use_album_artist);
                artist_check.set_margin_top(4);
                artist_check.set_tooltip_text(Some("When enabled, Artists view groups by AlbumArtist tag. When disabled, uses Artist tag."));
                content.append(&artist_check);

                let btn_box = gtk4::Box::new(Orientation::Horizontal, 8);
                btn_box.set_margin_top(8);
                let save_btn = gtk4::Button::with_label("Save");
                let cancel_btn = gtk4::Button::with_label("Cancel");
                let dw = d.clone();
                let he = host_entry.clone();
                let pe = port_entry.clone();
                let perr = port_error.clone();
                let cp = cp_save.clone();
                let tx = stx.clone();
                let pd_profile = profile_dropdown.clone();
                let pd_names = profile_names.clone();
                let hc_checkbox = hc_check.clone();
                let auto_start_box = auto_start_check.clone();
                let artist_checkbox = artist_check.clone();
                save_btn.connect_clicked(move |_| {
                    let mut c = Config::load();
                    c.high_contrast = hc_checkbox.is_active();
                    c.use_album_artist = artist_checkbox.is_active();
                    let new_auto_start = auto_start_box.is_active();
                    if new_auto_start != c.auto_start {
                        c.auto_start = new_auto_start;
                        if new_auto_start {
                            let _ = Config::install_autostart();
                        } else {
                            Config::remove_autostart();
                        }
                    }

                    // Apply selected profile first (overrides host/port)
                    if pd_names.len() > 1 {
                        let idx = pd_profile.selected() as usize;
                        if let Some(name) = pd_names.get(idx) {
                            if let Some(ref profiles) = c.profiles.clone() {
                                if let Some(profile) = profiles.get(name) {
                                    let host = profile.host.trim().to_string();
                                    c.mpd_host = host.clone();
                                    c.mpd_port = profile.port;
                                    c.last_profile = Some(name.clone());
                                    // Determine connection target from profile
                                    if let Ok(mut params) = cp.lock() {
                                        *params = if host.starts_with('/') || host.starts_with('~') {
                                            crate::mpd::ConnectionTarget::Unix(host)
                                        } else {
                                            crate::mpd::ConnectionTarget::Tcp(host, profile.port)
                                        };
                                    }
                                }
                            }
                        }
                    } else {
                        // No profile selected — use manual host/port
                        c.mpd_host = he.text().to_string();
                        match pe.text().parse::<u16>() {
                            Ok(p) if p > 0 => {
                                c.mpd_port = p;
                                perr.set_visible(false);
                            }
                            _ => {
                                perr.set_text("Invalid port (1-65535)");
                                perr.set_visible(true);
                                return;
                            }
                        }
                        if let Ok(mut params) = cp.lock() {
                            let host = c.mpd_host.trim().to_string();
                            *params = if host.starts_with('/') || host.starts_with('~') {
                                crate::mpd::ConnectionTarget::Unix(host)
                            } else if host.is_empty() || host == "auto" {
                                crate::mpd::ConnectionTarget::Auto
                            } else {
                                crate::mpd::ConnectionTarget::Tcp(host, c.mpd_port)
                            };
                        }
                    }

                    let _ = c.save();
                    let _ = tx.send(MpdCommand::Reconnect);
                    dw.close();
                });
                let dw2 = d.clone();
                cancel_btn.connect_clicked(move |_| { dw2.close(); });
                btn_box.append(&save_btn);
                btn_box.append(&cancel_btn);
                content.append(&btn_box);
                d.set_child(Some(&content));
                d.present();
            });
            top_bar.append(&settings_btn);

            // Ctrl+, settings shortcut
            let set_action = gtk4::gio::SimpleAction::new("settings", None);
            set_action.connect_activate(move |_, _| { btn_clone.emit_clicked(); });
            app_clone.add_action(&set_action);
            app_clone.set_accels_for_action("app.settings", &["<Ctrl>comma"]);

            // Ctrl+? keyboard shortcuts help
            let shortcuts_window = window.clone();
            let help_act = gtk4::gio::SimpleAction::new("shortcuts", None);
            help_act.connect_activate(move |_, _| {
                let d = gtk4::Window::new();
                d.set_title(Some("Keyboard Shortcuts"));
                d.set_transient_for(Some(&shortcuts_window));
                d.set_modal(true);
                let content = gtk4::Box::new(Orientation::Vertical, 4);
                content.set_margin_start(16); content.set_margin_end(16);
                content.set_margin_top(12); content.set_margin_bottom(12);
                let shortcuts = [
                    ("Ctrl+1", "Album Mode"),
                    ("Ctrl+2", "Folder Mode"),
                    ("Ctrl+F", "Search albums"),
                    ("Ctrl+,", "Settings"),
                    ("Ctrl+?", "Keyboard Shortcuts"),
                    ("Ctrl+Q", "Quit"),
                    ("", ""),
                    ("Double-click album", "Play album"),
                    ("Double-click queue item", "Play track at position"),
                    ("Right-click queue item", "Context menu (Play Now / Remove)"),
                    ("Delete", "Remove selected queue item"),
                    ("Shift+Up", "Move queue item up"),
                    ("Shift+Down", "Move queue item down"),
                ];
                for (key, desc) in &shortcuts {
                    let row = gtk4::Box::new(Orientation::Horizontal, 16);
                    let key_lbl = gtk4::Label::new(Some(key));
                    key_lbl.set_halign(gtk4::Align::Start);
                    key_lbl.set_css_classes(&["shortcut-key"]);
                    key_lbl.set_size_request(180, -1);
                    let desc_lbl = gtk4::Label::new(Some(desc));
                    desc_lbl.set_halign(gtk4::Align::Start);
                    row.append(&key_lbl);
                    row.append(&desc_lbl);
                    content.append(&row);
                }
                let close_btn = gtk4::Button::with_label("Close");
                close_btn.set_margin_top(8);
                close_btn.set_halign(gtk4::Align::End);
                let dw = d.clone();
                close_btn.connect_clicked(move |_| { dw.close(); });
                content.append(&close_btn);
                d.set_child(Some(&content));
                d.present();
            });
            app_clone.add_action(&help_act);
            app_clone.set_accels_for_action("app.shortcuts", &["<Ctrl>question"]);

            let now_playing = Box::new(Orientation::Vertical, 6);
            now_playing.set_margin_start(12);
            now_playing.set_margin_end(12);
            now_playing.set_margin_top(12);

            // --- Now Playing: [1] Cover (with placeholder) ---
            let np_cover = Picture::new();
            np_cover.set_size_request(200, 200);
            np_cover.set_halign(gtk4::Align::Center);
            np_cover.set_valign(gtk4::Align::Center);
            np_cover.set_content_fit(gtk4::ContentFit::Contain);
            np_cover.set_visible(false);
            // Placeholder: Picture with a generated neutral texture matching cover dimensions
            let np_cover_placeholder = Picture::new();
            np_cover_placeholder.set_size_request(200, 200);
            np_cover_placeholder.set_halign(gtk4::Align::Center);
            np_cover_placeholder.set_valign(gtk4::Align::Center);
            np_cover_placeholder.set_content_fit(gtk4::ContentFit::Contain);
            let ph_tex = make_placeholder_cover();
            np_cover_placeholder.set_paintable(ph_tex.as_ref());
            let np_cover_stack = gtk4::Stack::new();
            np_cover_stack.add_named(&np_cover_placeholder, Some("placeholder"));
            np_cover_stack.add_named(&np_cover, Some("cover"));
            np_cover_stack.set_visible_child_name("placeholder");
            now_playing.append(&np_cover_stack);

            // --- [2] Track title button (flat, click toggles track-list drawer) ---
            let track_title = gtk4::Button::with_label("No track playing");
            track_title.set_halign(gtk4::Align::Fill);
            track_title.set_hexpand(true);
            track_title.set_css_classes(&["track-title-btn"]);
            if let Some(lbl) = track_title.child().and_then(|c| c.downcast::<Label>().ok()) {
                lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                lbl.set_max_width_chars(32);
                lbl.set_halign(gtk4::Align::Start);
            }

            // Track data for popover: (title, file, duration)
            let popover_track_data: std::rc::Rc<std::cell::RefCell<Vec<(String, String, f64)>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

            // Simple ListBox — no model, no factory. Rows built imperatively.
            let track_listbox = gtk4::ListBox::new();
            track_listbox.set_selection_mode(gtk4::SelectionMode::Single);
            track_listbox.set_css_classes(&["track-list-popover"]);

            // Revealer drawer below track title (full right-rail width, no bubble clipping)
            let track_popover_scroll = ScrolledWindow::new();
            track_popover_scroll.set_child(Some(&track_listbox));
            track_popover_scroll.set_max_content_height(300);
            track_popover_scroll.set_propagate_natural_height(true);
            track_popover_scroll.set_hexpand(true);

            let track_revealer = gtk4::Revealer::new();
            track_revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
            track_revealer.set_transition_duration(200);
            track_revealer.set_child(Some(&track_popover_scroll));
            track_revealer.set_reveal_child(false);
            track_revealer.set_hexpand(true);

            // Click title button → toggle drawer
            let tr_toggle = track_revealer.clone();
            track_title.connect_clicked(move |_| {
                tr_toggle.set_reveal_child(!tr_toggle.reveals_child());
            });
            now_playing.append(&track_title);
            now_playing.append(&track_revealer);

            // --- [3] Artist line ---
            let track_artist = Label::new(Some(""));
            track_artist.set_halign(gtk4::Align::Start);
            track_artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            track_artist.set_max_width_chars(28);
            track_artist.set_css_classes(&["track-artist"]);
            now_playing.append(&track_artist);

            // --- [4] Album + Year on same line ---
            let track_album = Label::new(Some(""));
            track_album.set_halign(gtk4::Align::Start);
            track_album.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            track_album.set_max_width_chars(24);
            track_album.set_hexpand(true);

            let track_year = Label::new(Some(""));
            track_year.set_halign(gtk4::Align::End);
            track_year.set_valign(gtk4::Align::Center);
            track_year.set_css_classes(&["track-year"]);

            let album_year_row = Box::new(Orientation::Horizontal, 6);
            album_year_row.append(&track_album);
            album_year_row.append(&track_year);
            now_playing.append(&album_year_row);

            // --- [5] Info row: format (left) + bitrate (right) ---
            let fmt_label = Label::new(None);
            fmt_label.set_halign(gtk4::Align::Start);
            fmt_label.set_hexpand(true);
            fmt_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            fmt_label.set_max_width_chars(18);
            fmt_label.set_css_classes(&["tech-data"]);
            fmt_label.set_visible(false);

            let bitrate_label = Label::new(None);
            bitrate_label.set_halign(gtk4::Align::End);
            bitrate_label.set_css_classes(&["tech-data"]);
            bitrate_label.set_visible(false);

            let info_row = Box::new(Orientation::Horizontal, 8);
            info_row.append(&fmt_label);
            info_row.append(&bitrate_label);
            now_playing.append(&info_row);

            // --- [6] Controls row: transport (center) + dot (right) ---
            let pos_label = Label::new(Some("--:--"));
            pos_label.set_css_classes(&["time-display"]);
            pos_label.set_halign(gtk4::Align::Start);
            pos_label.set_hexpand(true);
            pos_label.set_valign(gtk4::Align::Center);

            let len_label = Label::new(Some("--:--"));
            len_label.set_css_classes(&["time-display"]);
            len_label.set_halign(gtk4::Align::End);
            len_label.set_valign(gtk4::Align::Center);

            let prev_album_btn = gtk4::Button::from_icon_name("media-skip-backward-symbolic");
            prev_album_btn.set_tooltip_text(Some("Previous Album"));
            let prev_track_btn = gtk4::Button::from_icon_name("media-seek-backward-symbolic");
            prev_track_btn.set_tooltip_text(Some("Previous Track"));
            let play_pause_btn = gtk4::Button::new();
            play_pause_btn.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
            play_pause_btn.set_tooltip_text(Some("Play/Pause"));
            let next_track_btn = gtk4::Button::from_icon_name("media-seek-forward-symbolic");
            next_track_btn.set_tooltip_text(Some("Next Track"));
            let next_album_btn = gtk4::Button::from_icon_name("media-skip-forward-symbolic");
            next_album_btn.set_tooltip_text(Some("Next Album"));

            let transport_box = Box::new(Orientation::Horizontal, 4);
            transport_box.set_halign(gtk4::Align::Center);
            transport_box.set_hexpand(true);
            transport_box.append(&prev_album_btn);
            transport_box.append(&prev_track_btn);
            transport_box.append(&play_pause_btn);
            transport_box.append(&next_track_btn);
            transport_box.append(&next_album_btn);

            let status_dot = Box::new(Orientation::Horizontal, 0);
            status_dot.set_size_request(12, 12);
            status_dot.set_halign(gtk4::Align::End);
            status_dot.set_valign(gtk4::Align::Center);
            status_dot.set_css_classes(&["status-dot", "stopped"]);

            // Overlay status dot on top-right of transport row to keep buttons centered
            let controls_overlay = gtk4::Overlay::new();
            controls_overlay.set_valign(gtk4::Align::Center);
            controls_overlay.set_child(Some(&transport_box));
            status_dot.set_halign(gtk4::Align::End);
            status_dot.set_valign(gtk4::Align::Center);
            status_dot.set_margin_end(2);
            controls_overlay.add_overlay(&status_dot);
            now_playing.append(&controls_overlay);

            // --- [6b] Time row: position (left) + length (right) ---
            let time_row = Box::new(Orientation::Horizontal, 4);
            time_row.set_valign(gtk4::Align::Center);
            time_row.set_hexpand(true);
            time_row.append(&pos_label);
            time_row.append(&len_label);
            now_playing.append(&time_row);

            // --- [7] Seekbar ---
            let seek_adjustment = gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 5.0, 0.0);
            let seekbar = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&seek_adjustment));
            seekbar.set_hexpand(true);
            seekbar.set_draw_value(false);
            seekbar.set_css_classes(&["seekbar"]);
            now_playing.append(&seekbar);

            right_pane.append(&now_playing);

            // --- [7] Queue stack (mini grid / track list) ---
            let queue_list = ListBox::new();
            queue_list.set_selection_mode(gtk4::SelectionMode::Single);
            // Popover storage for context menu — kept alive to prevent premature GC
            let queue_popover: std::rc::Rc<std::cell::RefCell<Option<gtk4::Popover>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let queue_scroll = ScrolledWindow::new();
            queue_scroll.set_child(Some(&queue_list));
            queue_scroll.set_vexpand(true);

            // Mini queue grid — GtkFixed manual layout (full control over positions + drag feedback)
            let mini_grid_data: MiniGridData = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let mini_cover_widgets: std::rc::Rc<std::cell::RefCell<HashMap<String, Picture>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            let mini_current_album: std::rc::Rc<std::cell::RefCell<Option<String>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));

            let mini_fixed = Fixed::new();
            mini_fixed.set_vexpand(true);
            mini_fixed.set_size_request(-1, 90); // minimum one-row height so drops work before Queue data

            // Cell registry for drag hit-testing
            let mini_cells: std::rc::Rc<std::cell::RefCell<Vec<MiniCell>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

            // Drag target on Fixed — motion/leave highlight, drop inserts at position
            let mg_drop_cmd = cmd_tx.clone();
            let _mg_drop_data = mini_grid_data.clone();
            let mg_drop_target = DropTarget::new(String::static_type(), DragAction::COPY);

            let mg_m_cells = mini_cells.clone();
            mg_drop_target.connect_motion(move |_target, x, y| {
                let cells = mg_m_cells.borrow();
                let mut found: Option<usize> = None;
                for (i, c) in cells.iter().enumerate() {
                    if x >= c.x && x < c.x + c.w as f64 && y >= c.y && y < c.y + c.h as f64 {
                        found = Some(i);
                        break;
                    }
                }
                // Update CSS classes — only the hovered cell gets the highlight
                for (i, c) in cells.iter().enumerate() {
                    if Some(i) == found {
                        c.container.add_css_class("mini-queue-drop-target");
                    } else {
                        c.container.remove_css_class("mini-queue-drop-target");
                    }
                }
                gtk4::gdk::DragAction::COPY
            });

            let mg_l_cells = mini_cells.clone();
            mg_drop_target.connect_leave(move |_target| {
                for c in mg_l_cells.borrow().iter() {
                    c.container.remove_css_class("mini-queue-drop-target");
                }
            });

            let mg_do_data = mini_grid_data.clone();
            let mg_do_cells = mini_cells.clone();
            mg_drop_target.connect_drop(move |_target, value, x, y| {
                // Clear highlights
                for c in mg_do_cells.borrow().iter() {
                    c.container.remove_css_class("mini-queue-drop-target");
                }
                if let Ok(s) = value.get::<String>() {
                    if s.contains(':') || s.trim().is_empty() {
                        return false;
                    }
                    // Hit-test for insert position, or append to end if grid empty
                    let cells = mg_do_cells.borrow();
                    let binding = mg_do_data.borrow();
                    let mut placed = false;
                    if !cells.is_empty() && !binding.is_empty() {
                        let target_idx = cells.iter()
                            .position(|c| x >= c.x && x < c.x + c.w as f64 && y >= c.y && y < c.y + c.h as f64);
                        if let Some(idx) = target_idx {
                            if let Some(item) = binding.get(idx) {
                                let _ = mg_drop_cmd.send(MpdCommand::AddAt(s.clone(), item.first_pos));
                                placed = true;
                            }
                        }
                    }
                    if !placed {
                        let _ = mg_drop_cmd.send(MpdCommand::Add(s));
                    }
                    return true;
                }
                false
            });
            mini_fixed.add_controller(mg_drop_target);

            // Stack switching between mini grid (Album Mode) and track list (Folder Mode)
            let queue_stack = gtk4::Stack::new();
            let mini_scroll = ScrolledWindow::new();
            mini_scroll.set_child(Some(&mini_fixed));
            mini_scroll.set_vexpand(true);
            mini_scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
            queue_stack.add_child(&mini_scroll);
            queue_stack.add_child(&queue_scroll);
            queue_stack.set_visible_child(&mini_scroll); // Album Mode by default
            right_pane.append(&queue_stack);

            // GtkDropTarget for receiving album drops from the grid (COPY) and acting as
            // safety buffer for queue reorder drops (MOVE) — prevents propagation to removal target
            let dt_cmd = cmd_tx.clone();
            let drop_target = DropTarget::new(String::static_type(), DragAction::COPY | DragAction::MOVE);
            drop_target.connect_drop(move |target, value, _x, _y| {
                // Clear highlight on drop (in case leave doesn't fire)
                if let Some(w) = target.widget() {
                    w.remove_css_class("queue-drop-highlight");
                }
                if let Ok(s) = value.get::<String>() {
                    if s.contains(':') {
                        // Reorder drop on queue_stack margins (not on queue_list) — safe no-op
                        return true;
                    }
                    let _ = dt_cmd.send(MpdCommand::Add(s));
                    return true;
                }
                false
            });
            drop_target.connect_enter(move |target, _x, _y| {
                if let Some(w) = target.widget() {
                    w.add_css_class("queue-drop-highlight");
                }
                DragAction::COPY | DragAction::MOVE
            });
            drop_target.connect_leave(move |target| {
                if let Some(w) = target.widget() {
                    w.remove_css_class("queue-drop-highlight");
                }
            });
            queue_stack.add_controller(drop_target);

            // GtkDropTarget for now-playing zone: album drop → clear queue + play album
            let removal_target = DropTarget::new(String::static_type(), DragAction::COPY | DragAction::MOVE);
            removal_target.connect_enter(|_target, _x, _y| {
                DragAction::COPY | DragAction::MOVE
            });
            let rmv_cmd = cmd_tx.clone();
            removal_target.connect_drop(move |_target, value, _x, _y| {
                if let Ok(s) = value.get::<String>() {
                    if s.trim().is_empty() {
                        return false;
                    }
                    // Queue row drag ("id:pos") → remove item
                    if let Some((id_str, _pos_str)) = s.split_once(':') {
                        if let Ok(drag_id) = id_str.parse::<i32>() {
                            let _ = rmv_cmd.send(MpdCommand::DeleteId(drag_id));
                            return true;
                        }
                    }
                    // Album drop → clear + play album
                    let _ = rmv_cmd.send(MpdCommand::PlayAlbum(s));
                    return true;
                }
                false
            });
            right_pane.add_controller(removal_target);

            // Shared item_ids for Delete key — updated by Queue event handler
            let item_ids_w: SharedIds = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            let ql_del = queue_list.clone();
            let cmd_del = cmd_tx.clone();
            let ids_del = item_ids_w.clone();
            let kc = EventControllerKey::new();
            kc.connect_key_pressed(move |_ctrl, key, _code, _mods| {
                // NOTE: `ids_del` is an Rc<RefCell<HashMap<i32, i32>>>.
                // All access is on the GTK main thread, so RefCell is safe.
                match key {
                    gtk4::gdk::Key::Delete | gtk4::gdk::Key::KP_Delete => {
                        let idx = ql_del.selected_row().map(|r| r.index()).unwrap_or(-1);
                        let id = ids_del.borrow().get(&idx).copied();
                        if let Some(id) = id {
                            let _ = cmd_del.send(MpdCommand::DeleteId(id));
                        }
                    }
                    gtk4::gdk::Key::Up if _mods.contains(gtk4::gdk::ModifierType::SHIFT_MASK) => {
                        let idx = ql_del.selected_row().map(|r| r.index()).unwrap_or(-1);
                        if idx > 0 {
                            let id = ids_del.borrow().get(&idx).copied();
                            if let Some(id) = id {
                                let _ = cmd_del.send(MpdCommand::MoveId(id, idx - 1));
                                // Optimistic update: reflect the move in the local map so
                                // rapid key presses read correct index->id mappings before
                                // the queue refresh arrives (fixes race condition).
                                let mut ids = ids_del.borrow_mut();
                                if let Some(removed) = ids.remove(&idx) {
                                    // Shift item at idx-1 up to idx,
                                    // then place the moved item at idx-1.
                                    if let Some(v) = ids.remove(&(idx - 1)) {
                                        ids.insert(idx, v);
                                    }
                                    ids.insert(idx - 1, removed);
                                }
                            }
                        }
                    }
                    gtk4::gdk::Key::Down if _mods.contains(gtk4::gdk::ModifierType::SHIFT_MASK) => {
                        let idx = ql_del.selected_row().map(|r| r.index()).unwrap_or(-1);
                        let id = ids_del.borrow().get(&idx).copied();
                        if let Some(id) = id {
                            let _ = cmd_del.send(MpdCommand::MoveId(id, idx + 1));
                            // Optimistic update: reflect the move in the local map.
                            let mut ids = ids_del.borrow_mut();
                            if let Some(removed) = ids.remove(&idx) {
                                if ids.contains_key(&(idx + 1)) {
                                    let v = ids.remove(&(idx + 1)).unwrap();
                                    ids.insert(idx, v);
                                    ids.insert(idx + 1, removed);
                                } else {
                                    // Last item in queue — MoveId to idx+1 is a no-op in MPD.
                                    ids.insert(idx, removed);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                gtk4::glib::Propagation::Proceed
            });
            queue_list.add_controller(kc);

            // GtkDropTarget for drag-reorder within the queue
            let indicator_tracker: std::rc::Rc<std::cell::RefCell<Option<gtk4::ListBoxRow>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let reorder_target = DropTarget::new(String::static_type(), DragAction::MOVE);

            let tr_on_motion = indicator_tracker.clone();
            let tr_list = queue_list.clone();
            reorder_target.connect_motion(move |_target, _x, y| {
                // Find the row at the cursor position using accumulated heights
                let mut y_accum = 0i32;
                let mut child = tr_list.first_child();
                let mut target_idx = -1i32;
                while let Some(row_widget) = child {
                    if let Some(row) = row_widget.downcast_ref::<gtk4::ListBoxRow>() {
                        let row_h = row.height();
                        if y >= y_accum as f64 && y < (y_accum + row_h) as f64 {
                            let midpoint = y_accum + row_h / 2;
                            target_idx = if y < midpoint as f64 { row.index() } else { row.index() + 1 };
                            break;
                        }
                        y_accum += row_h;
                        target_idx = row.index() + 1; // past last row = end of queue
                    }
                    child = row_widget.next_sibling();
                }

                // Update drop indicator
                if let Some(prev) = tr_on_motion.borrow_mut().take() {
                    prev.remove_css_class("drop-indicator-row");
                }
                if target_idx >= 0 {
                    if let Some(target_row) = tr_list.first_child() {
                        let mut idx = 0i32;
                        let mut child = Some(target_row);
                        while let Some(row_widget) = child {
                            if let Some(row) = row_widget.downcast_ref::<gtk4::ListBoxRow>() {
                                if idx == target_idx {
                                    row.add_css_class("drop-indicator-row");
                                    *tr_on_motion.borrow_mut() = Some(row.clone());
                                    break;
                                }
                                idx += 1;
                            }
                            child = row_widget.next_sibling();
                        }
                    }
                }
                DragAction::MOVE
            });

            let tr_drop = indicator_tracker.clone();
            let tr_list_drop = queue_list.clone();
            let tr_cmd = cmd_tx.clone();
            let tr_add_cmd = cmd_tx.clone();
            reorder_target.connect_drop(move |_target, value, _x, y| {
                // Clear drop indicator
                if let Some(prev) = tr_drop.borrow_mut().take() {
                    prev.remove_css_class("drop-indicator-row");
                }

                if let Ok(s) = value.get::<String>() {
                    if let Some((id_str, _pos_str)) = s.split_once(':') {
                        // Queue reorder: format "id:pos"
                        if let Ok(drag_id) = id_str.parse::<i32>() {
                            // Calculate target position from y using same midpoint logic as motion
                            let mut y_accum = 0i32;
                            let mut child = tr_list_drop.first_child();
                            let mut target_pos = 0i32;
                            while let Some(row_widget) = child {
                                if let Some(row) = row_widget.downcast_ref::<gtk4::ListBoxRow>() {
                                    let row_h = row.height();
                                    if y >= y_accum as f64 && y < (y_accum + row_h) as f64 {
                                        let midpoint = y_accum + row_h / 2;
                                        target_pos = if y < midpoint as f64 { row.index() } else { row.index() + 1 };
                                        break;
                                    }
                                    y_accum += row_h;
                                    target_pos = row.index() + 1;
                                }
                                child = row_widget.next_sibling();
                            }
                            let _ = tr_cmd.send(MpdCommand::MoveId(drag_id, target_pos));
                            return true;
                        }
                    } else {
                        // Album drop from grid — forward to Add
                        let _ = tr_add_cmd.send(MpdCommand::Add(s));
                        return true;
                    }
                }
                false
            });

            let tr_leave = indicator_tracker.clone();
            reorder_target.connect_leave(move |_target| {
                if let Some(prev) = tr_leave.borrow_mut().take() {
                    prev.remove_css_class("drop-indicator-row");
                }
            });
            queue_list.add_controller(reorder_target);

            multi_view.set_child("right", &right_pane);

            // --- MultiLayoutView: wide and narrow layouts ---
            let bottom_sheet = adw::BottomSheet::new();
            // Narrow layout: BottomSheet shows right_pane (via slot "right") on demand
            let narrow_content = Box::new(Orientation::Vertical, 0);
            let narrow_left = adw::LayoutSlot::new("left");
            narrow_left.set_vexpand(true);
            narrow_content.append(&narrow_left);
            bottom_sheet.set_content(Some(&narrow_content));
            let narrow_right_slot = adw::LayoutSlot::new("right");
            bottom_sheet.set_sheet(Some(&narrow_right_slot));
            let narrow_layout = adw::Layout::new(&bottom_sheet);
            narrow_layout.set_name(Some("narrow"));
            multi_view.add_layout(narrow_layout);

            // Wide layout: side-by-side split (left pane + right pane)
            let wide_box = Box::new(Orientation::Horizontal, 0);
            let wide_left = adw::LayoutSlot::new("left");
            let wide_right = adw::LayoutSlot::new("right");
            wide_left.set_hexpand(true);
            wide_right.set_size_request(320, -1);
            wide_box.append(&wide_left);
            wide_box.append(&wide_right);
            let wide_layout = adw::Layout::new(&wide_box);
            wide_layout.set_name(Some("wide"));
            multi_view.add_layout(wide_layout);

            // Start in correct layout based on configured window geometry
            let init_narrow = cfg.window_geometry.as_ref()
                .map(|g| g.width < 800)
                .unwrap_or(false);
            multi_view.set_layout_name(if init_narrow { "narrow" } else { "wide" });

            // Toast overlay for notifications (libadwaita)
            let toast_overlay = adw::ToastOverlay::new();
            toast_overlay.set_child(Some(&multi_view));
            window.set_content(Some(&toast_overlay));

            // Ctrl+B toggles BottomSheet in narrow mode, no-op in wide
            let bs_toggle = bottom_sheet.clone();
            let mv_toggle_ref = multi_view.clone();
            let toggle_bs_act = gtk4::gio::SimpleAction::new("toggle-sidebar", None);
            toggle_bs_act.connect_activate(move |_, _| {
                if mv_toggle_ref.layout_name().as_deref() == Some("narrow") {
                    bs_toggle.set_open(!bs_toggle.is_open());
                }
            });
            app_clone.add_action(&toggle_bs_act);
            app_clone.set_accels_for_action("app.toggle-sidebar", &["<Ctrl>B"]);

            // Grid populates on startup via set_active(true) on the "Albums" button above
            let _ = cmd_tx.send(MpdCommand::ListQueue);

            let poll_tx = cmd_tx.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(30), move || {
                let _ = poll_tx.send(MpdCommand::ListQueue);
                glib::ControlFlow::Continue
            });

            window.present();

            // --- Scroll-aware cover loading: debounce timer + vadjustment handler ---
            // Generation counter: each scroll event bumps the generation. The debounce
            // callback checks if it's still the latest; old callbacks become no-ops.
            // This avoids SourceId::remove() races (SourceId can become invalid if
            // the callback fires before we finish setting up the next timer).
            let scroll_gen: std::rc::Rc<std::sync::atomic::AtomicUsize> =
                std::rc::Rc::new(std::sync::atomic::AtomicUsize::new(0));

            let sv_adj = left_scroll.vadjustment();
            let sv_cells = album_cells.clone();
            let sv_cmd = cmd_tx.clone();
            let sv_cp = cover_paths.clone();
            let sv_sw = left_scroll.clone();
            sv_adj.connect_value_changed(move |adj| {
                let gen_id = scroll_gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                let cells = sv_cells.clone();
                let cmd = sv_cmd.clone();
                let adj_clone = adj.clone();
                let check_gen = scroll_gen.clone();
                let cp = sv_cp.clone();
                let sw = sv_sw.clone();
                glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
                    if check_gen.load(std::sync::atomic::Ordering::Relaxed) != gen_id {
                        return glib::ControlFlow::Break;
                    }
                    // Estimate visible range from scroll position and known cell size
                    let scroll_top = adj_clone.value();
                    let page_size = adj_clone.page_size();
                    if page_size <= 0.0 {
                        return glib::ControlFlow::Break;
                    }
                    let cols = ((sw.width() as f64 / CELL_SLOT_W).floor() as usize).max(1);
                    let first_row = (scroll_top / CELL_SLOT_H).floor() as usize;
                    let visible_rows = (page_size / CELL_SLOT_H).ceil() as usize + 2;
                    let cells_binding = cells.borrow();
                    let start_idx = first_row.saturating_mul(cols);
                    let end_idx = (first_row + visible_rows).saturating_mul(cols).min(cells_binding.len());
                    let new_albums: Vec<_> = cells_binding[start_idx..end_idx].iter()
                        .filter(|c| {
                            let key = crate::coverart::cover_key(&c.artist, &c.album);
                            !cp.borrow().contains_key(&key)
                        })
                        .map(|c| (c.artist.clone(), c.album.clone()))
                        .collect();
                    drop(cells_binding);
                    if !new_albums.is_empty() {
                        log::debug!(
                            "[ui] scroll stop: enqueuing {} visible albums for cover fetch",
                            new_albums.len()
                        );
                        let _ = cmd.send(MpdCommand::FetchCovers(new_albums));
                    }
                    glib::ControlFlow::Break
                });
            });

            // --- Frame clock tick callback for MPD events (replaces 30ms timer) ---
            // add_tick_callback fires once per display refresh (vsync-aligned).
            // Replaces the fixed 30ms timer that could fire mid-frame or during
            // layout passes, which starved the GTK main loop under heavy load.
            // Pinned header removed — user reported it doesn't work as expected

            let fc_rx = event_rx.clone();
            let fc_tl = track_title.clone();
            let fc_ar = track_artist.clone();
            let fc_al = track_album.clone();
            let fc_yl = track_year.clone();
            let fc_seekbar_cmd = cmd_tx.clone();
            seekbar.connect_change_value(move |_, _, value| {
                let pos = value as i64;
                let _ = fc_seekbar_cmd.send(MpdCommand::Seek(pos));
                glib::Propagation::Stop
            });

            let fc_play_pause = play_pause_btn.clone();
            let fc_status_dot = status_dot.clone();
            let fc_pos = pos_label.clone();
            let fc_len = len_label.clone();
            let fc_seekbar = seekbar.clone();
            let fc_fmt_label = fmt_label.clone();
            let fc_bitrate_label = bitrate_label.clone();
            let fc_np_cover = np_cover.clone();
            let fc_np_cover_stack = np_cover_stack.clone();
            let fc_popover_track_data = popover_track_data.clone();
            let fc_track_listbox = track_listbox.clone();
            let fc_track_cmd = cmd_tx.clone();
            let fc_queue_entries: std::rc::Rc<std::cell::RefCell<Vec<crate::mpd::QueueEntry>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let fc_stack = left_stack.clone();
            let fc_scroll = left_scroll.clone();
            let fc_empty = empty_label.clone();
            let fc_cmd = cmd_tx.clone();
            let fc_fb = folder_browser.clone();
            let fc_fs_list = folder_search_list.clone();
            let fc_fs_container = folder_search_results.clone();
            let fc_ql = queue_list.clone();
            let fc_queue_popover = queue_popover.clone();
            let fc_ids = item_ids_w.clone();
            let fc_scmd = scmd.clone();
            let fc_mc = metadata_cache.clone();
            let fc_toast = toast_overlay.clone();
            let fc_ev_cover_paths = cover_paths.clone();
            let fc_ev_cover_tex_cache = cover_texture_cache.clone();
            let fc_mini_cw = mini_cover_widgets.clone();
            let fc_cp_np = fc_ev_cover_paths.clone();
            let fc_ev_cells = album_cells.clone();
            let fc_layout = album_layout.clone();
            let fc_current_song_pos: std::cell::Cell<Option<i32>> = std::cell::Cell::new(None);
            let fc_current_album: std::rc::Rc<std::cell::RefCell<Option<String>>> = std::rc::Rc::new(std::cell::RefCell::new(None));
            let fc_shutdown = shutdown_app.clone();

            // Mini grid captures and mode-aware queue stack switching
            let fc_state = state.clone();
            let fc_mpris_update = mpris_update_tx.clone();
            let fc_mini_fixed = mini_fixed.clone();
            let fc_mini_cells = mini_cells.clone();
            let fc_mini_data = mini_grid_data.clone();
            let fc_mini_current = mini_current_album.clone();
            let fc_queue_stack = queue_stack.clone();
            let fc_mini_scroll_ref = mini_scroll.clone();
            let fc_queue_scroll_ref = queue_scroll.clone();
            let fc_prev_mode: std::cell::Cell<crate::state::Mode> = std::cell::Cell::new(crate::state::Mode::Album);
            let fc_sort_mode: u32 = 0; // fixed to artist sort (was DropDown)
            let fc_multi_view = multi_view.clone();
            let fc_bottom_panel = bottom_panel.clone();
            let fc_bottom_sheet = bottom_sheet.clone();
            let fc_bp_title = bp_title.clone();
            let fc_bp_play = bp_play.clone();
            let fc_track_revealer = track_revealer.clone();

            // Wire popover track listbox: click row → play that track
            let tv_cmd = fc_cmd.clone();
            let tv_data = fc_popover_track_data.clone();
            let tv_entries = fc_queue_entries.clone();
            let tv_revealer = fc_track_revealer.clone();
            let tv_listbox = fc_track_listbox.clone();
            tv_listbox.connect_row_activated(move |_list, row| {
                let idx = row.index() as usize;
                let data = tv_data.borrow();
                if let Some((_, file, _)) = data.get(idx) {
                    let entries = tv_entries.borrow();
                    let target_pos = entries.iter()
                        .find(|e| &e.file == file)
                        .map(|e| e.position);
                    if let Some(pos) = target_pos {
                        let _ = tv_cmd.send(MpdCommand::PlayPosition(pos));
                    }
                }
                tv_revealer.set_reveal_child(false);
            });

            // Wire play/pause button
            let pp_cmd = fc_cmd.clone();
            play_pause_btn.connect_clicked(move |_| { let _ = pp_cmd.send(MpdCommand::Pause); });

            // Wire prev/next track buttons
            let pt_cmd = fc_cmd.clone();
            prev_track_btn.connect_clicked(move |_| { let _ = pt_cmd.send(MpdCommand::Previous); });
            let nt_cmd = fc_cmd.clone();
            next_track_btn.connect_clicked(move |_| { let _ = nt_cmd.send(MpdCommand::Next); });

            // Wire bottom panel transport buttons
            let bp_p_cmd = fc_cmd.clone();
            bp_prev.connect_clicked(move |_| { let _ = bp_p_cmd.send(MpdCommand::Previous); });
            let bp_pl_cmd = fc_cmd.clone();
            bp_play.connect_clicked(move |_| { let _ = bp_pl_cmd.send(MpdCommand::Pause); });
            let bp_n_cmd = fc_cmd.clone();
            bp_next.connect_clicked(move |_| { let _ = bp_n_cmd.send(MpdCommand::Next); });

            // Bottom queue button → toggle BottomSheet in narrow mode
            let bq_mv = fc_multi_view.clone();
            let bq_bs = fc_bottom_sheet.clone();
            bp_queue_btn.connect_clicked(move |_| {
                if bq_mv.layout_name().as_deref() == Some("narrow") {
                    bq_bs.set_open(!bq_bs.is_open());
                }
            });

            // Wire prev/next album buttons
            let pa_cmd = fc_cmd.clone();
            let pa_entries = fc_queue_entries.clone();
            let pa_pos = fc_current_song_pos.clone();
            prev_album_btn.connect_clicked(move |_| {
                let entries = pa_entries.borrow();
                let csp = pa_pos.get();
                if let Some(pos) = find_album_boundary(&entries, csp, false) {
                    let _ = pa_cmd.send(MpdCommand::PlayPosition(pos));
                }
            });
            let na_cmd = fc_cmd.clone();
            let na_entries = fc_queue_entries.clone();
            let na_pos = fc_current_song_pos.clone();
            next_album_btn.connect_clicked(move |_| {
                let entries = na_entries.borrow();
                let csp = na_pos.get();
                if let Some(pos) = find_album_boundary(&entries, csp, true) {
                    let _ = na_cmd.send(MpdCommand::PlayPosition(pos));
                }
            });

            window.add_tick_callback(move |_widget, _fc| {
                // Switch layout on narrow windows (<800px), show bottom transport bar
                let win_width = _widget.width() as f64;
                let narrow = win_width < 800.0;
                let current_narrow = fc_multi_view.layout_name().as_deref() == Some("narrow");
                if narrow != current_narrow {
                    fc_multi_view.set_layout_name(if narrow { "narrow" } else { "wide" });
                    if !narrow {
                        fc_bottom_sheet.set_open(false);
                    }
                }
                fc_bottom_panel.set_visible(narrow);

                // Ensure queue display matches current mode
                let cur_mode = fc_state.read().map(|s| s.mode).unwrap_or(crate::state::Mode::Album);
                if cur_mode != fc_prev_mode.get() {
                    fc_prev_mode.set(cur_mode);
                    match cur_mode {
                        crate::state::Mode::Album => fc_queue_stack.set_visible_child(&fc_mini_scroll_ref),
                        crate::state::Mode::Folder => fc_queue_stack.set_visible_child(&fc_queue_scroll_ref),
                    }
                }

                let mut guard = match fc_rx.lock() {
                    Ok(g) => g,
                    Err(poisoned) => {
                        log::error!("MPD event receiver mutex poisoned: {poisoned}");
                        fc_shutdown.quit();
                        return glib::ControlFlow::Break;
                    }
                };
                // Process at most 64 events per tick to yield to GTK main loop.
                // Frame clock fires once per display refresh (~16ms at 60Hz),
                // so this processing is vsync-aligned, not mid-frame.
                let mut batch = 0u32;
                while batch < 64 {
                    let event = match guard.try_recv() {
                        Ok(e) => e,
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            drop(guard);
                            return glib::ControlFlow::Break;
                        }
                    };
                    batch += 1;
                    drop(guard);
                    let fwd = event.clone();
                    match event {
                        MpdEvent::Connected => {
                            if let Ok(mut app_state) = fc_state.write() {
                                crate::state::reduce(&mut app_state, &MpdEvent::Connected);
                            }
                            fc_scmd.send(SearchCommand::Reset);
                            let _ = fc_cmd.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                            let _ = fc_cmd.send(MpdCommand::ListQueue);
                        }
                        MpdEvent::Connecting => {
                            if let Ok(mut app_state) = fc_state.write() {
                                crate::state::reduce(&mut app_state, &MpdEvent::Connecting);
                            }
                        }
                        MpdEvent::Disconnected => {
                            if let Ok(mut app_state) = fc_state.write() {
                                crate::state::reduce(&mut app_state, &MpdEvent::Disconnected);
                            }
                        }
                        MpdEvent::StateChanged(update) => {
                            // Reduce: write SharedState.current (canonical)
                            if let Ok(mut app_state) = fc_state.write() {
                                crate::state::reduce(&mut app_state, &MpdEvent::StateChanged(update.clone()));
                            }
                            fc_current_song_pos.set(update.song.map(|s| s as i32));
                            let album_changed = fc_current_album.borrow().as_deref() != update.album.as_deref();
                            *fc_current_album.borrow_mut() = update.album.clone();
                            if album_changed {
                                if let Some(ref album) = update.album {
                                    let _ = fc_track_cmd.send(MpdCommand::ListAlbumTracks(album.clone()));
                                } else {
                                    fc_track_listbox.remove_all();
                                    fc_popover_track_data.borrow_mut().clear();
                                }
                                // Rebuild mini grid to update current-album highlight
                                *fc_mini_current.borrow_mut() = update.album.clone();
                                let items = fc_mini_data.borrow();
                                if !items.is_empty() {
                                    let vpw = fc_mini_scroll_ref.width();
                                    let cells = rebuild_mini_fixed(
                                        &fc_mini_fixed,
                                        &items,
                                        &fc_ev_cover_paths,
                                        &fc_mini_cw,
                                        &fc_mini_current.borrow(),
                                        &fc_cmd,
                                        vpw,
                                    );
                                    *fc_mini_cells.borrow_mut() = cells;
                                }
                            }
                            // Update popover selection to current track (same-album track change)
                            if let Some(ref file) = update.file {
                                let data = fc_popover_track_data.borrow();
                                if let Some(pos) = data.iter().position(|(_, f, _)| f == file) {
                                    if let Some(row) = fc_track_listbox.row_at_index(pos as i32) {
                                        fc_track_listbox.select_row(Some(&row));
                                    }
                                }
                            }
                            handle_now_playing(&update, NowPlayingWidgets {
    title: &fc_tl,
    artist: &fc_ar,
    album: &fc_al,
    year: &fc_yl,
    status_dot: &fc_status_dot,
    play_pause_btn: &fc_play_pause,
    pos_label: &fc_pos,
    len_label: &fc_len,
    seekbar: &fc_seekbar,
    fmt_label: &fc_fmt_label,
    bitrate_label: &fc_bitrate_label,
    cover: &fc_np_cover,
    cover_stack: &fc_np_cover_stack,
    cover_paths: &fc_cp_np,
}, &fc_mpris_update);

                            // Update bottom panel now-playing
                            fc_bp_title.set_label(update.title.as_deref().unwrap_or("No track playing"));
                            match update.state.as_str() {
                                "play" => fc_bp_play.set_child(Some(&gtk4::Image::from_icon_name("media-playback-pause-symbolic"))),
                                _ => fc_bp_play.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic"))),
                            }
                        }
                        MpdEvent::Albums(albums) => {
                            let flat_for_index: Vec<(String, String)> = albums.iter()
                                .map(|m| (m.album_artist.clone(), m.album.clone()))
                                .collect();
                            fc_scmd.send(SearchCommand::BuildIndex(flat_for_index));
                            if albums.is_empty() {
                                fc_empty.set_text("No albums found");
                                fc_stack.set_visible_child(&fc_empty);
                            } else {
                                let mut sorted = albums.clone();
                                let sort_idx = fc_sort_mode as usize;
                                match sort_idx {
                                    1 => sorted.sort_by(|a, b| a.album.cmp(&b.album)),
                                    2 => sorted.sort_by(|a, b| b.album_artist.cmp(&a.album_artist)),
                                    3 => sorted.sort_by(|a, b| b.album.cmp(&a.album)),
                                    _ => sorted.sort_by(|a, b| a.album_artist.cmp(&b.album_artist)),
                                }
                                // Apply custom session order
                                if let Ok(st) = fc_state.read() {
                                    let order = &st.album_browsing.custom_album_order;
                                    if !order.is_empty() {
                                        sorted.sort_by_key(|m| {
                                            order.iter().position(|o| o == &m.album).unwrap_or(usize::MAX)
                                        });
                                    }
                                }
                                // Build AlbumCells
                                let cmd = fc_cmd.clone();
                                let cmd2 = fc_cmd.clone();
                                let all_albums: Vec<(String, String)> = sorted.iter()
                                    .map(|m| (m.album_artist.clone(), m.album.clone()))
                                    .collect();
                                let layout = fc_layout.clone();
                                let cells_rc = fc_ev_cells.clone();
                                let cp = fc_ev_cover_paths.clone();
                                let tc = fc_ev_cover_tex_cache.clone();
                                // Clear old widgets from layout
                                while let Some(child) = layout.first_child() {
                                    layout.remove(&child);
                                }
                                let mut new_cells: Vec<AlbumCell> = Vec::with_capacity(sorted.len());
                                for (_i, meta) in sorted.iter().enumerate() {
                                    let cell = AlbumCoverCell::new(cmd2.clone());
                                    let key = crate::coverart::cover_key(&meta.album_artist, &meta.album);
                                    cell.set_album(&meta.album, &meta.album_artist, meta.year.as_deref(), &meta.album);
                                    cell.set_year_badge(format_year_badge(meta.year.as_deref()).as_deref());
                                    // Set cover texture from cache or placeholder
                                    let cached = tc.borrow().get(&key).cloned();
                                    if let Some(tex) = cached {
                                        cell.set_cover_texture(&tex);
                                    } else if let Some(p) = cp.borrow().get(&key).and_then(|o| o.clone()) {
                                        if let Ok(img) = image::open(&p) {
                                            let rgba = image::imageops::resize(&img.to_rgba8(), 200, 200, image::imageops::FilterType::Lanczos3);
                                            let bytes = glib::Bytes::from_owned(rgba.into_raw());
                                            let tex = gdk4::MemoryTexture::new(200, 200, gdk4::MemoryFormat::R8g8b8a8, &bytes, 200 * 4);
                                            tc.borrow_mut().insert(key.clone(), tex.clone().into());
                                            cell.set_cover_texture(&tex);
                                        } else {
                                            cell.set_cover_texture(&placeholder_texture(&meta.album_artist));
                                        }
                                    } else {
                                        cell.set_cover_texture(&placeholder_texture(&meta.album_artist));
                                    }
                                    layout.put(&cell, 0.0, 0.0);
                                    new_cells.push(AlbumCell {
                                        cell,
                                        caption: None,
                                        artist: meta.album_artist.clone(),
                                        album: meta.album.clone(),
                                        album_id: key.clone(),
                                        group_value: None,
                                    });
                                }
                                *cells_rc.borrow_mut() = new_cells;
                                let sw = fc_scroll.clone();
                                let w = sw.width() as f64;
                                if w > 0.0 {
                                    let cells = cells_rc.borrow();
                                    reposition(&layout, &cells, w);
                                }
                                fc_stack.set_visible_child(&fc_scroll);
                                if !all_albums.is_empty() {
                                    log::debug!("[ui] initial load: enqueuing {} albums for cover fetch", all_albums.len());
                                    let _ = cmd.send(MpdCommand::FetchCovers(all_albums));
                                }
                            }
                        }
                        MpdEvent::AlbumsGrouped(groups) => {
                            let flat: Vec<(String, String)> = groups.iter()
                                .flat_map(|(_, a)| a.iter()
                                    .map(|m| (m.album_artist.clone(), m.album.clone()))
                                    .collect::<Vec<_>>())
                                .collect();
                            if groups.is_empty() {
                                fc_empty.set_text("No albums found");
                                fc_stack.set_visible_child(&fc_empty);
                            } else {
                                fc_scmd.send(SearchCommand::BuildIndex(flat.clone()));
                                let single_group = groups.len() == 1;
                                let view_mode = active_group.borrow().clone();
                                let layout = fc_layout.clone();
                                let cells_rc = fc_ev_cells.clone();
                                let cmd = fc_cmd.clone();
                                let cmd2 = fc_cmd.clone();
                                let cp = fc_ev_cover_paths.clone();
                                let tc = fc_ev_cover_tex_cache.clone();
                                while let Some(child) = layout.first_child() {
                                    layout.remove(&child);
                                }
                                let mut new_cells: Vec<AlbumCell> = Vec::new();
                                let mut all_albums: Vec<(String, String)> = Vec::new();
                                for (header, albums) in groups.iter() {
                                    let group_value: Option<String> = if single_group { None } else { Some(header.clone()) };
                                    let mut sorted_albums = albums.clone();
                                    if single_group {
                                        if let Ok(st) = fc_state.read() {
                                            let order = &st.album_browsing.custom_album_order;
                                            if !order.is_empty() {
                                                sorted_albums.sort_by_key(|m| {
                                                    order.iter().position(|o| o == &m.album).unwrap_or(usize::MAX)
                                                });
                                            }
                                        }
                                    }
                                    for meta in &sorted_albums {
                                        let cell = AlbumCoverCell::new(cmd2.clone());
                                        let key = crate::coverart::cover_key(&meta.album_artist, &meta.album);
                                        let caption = group_caption_for_view(&view_mode, header, meta);
                                        let year_badge = format_year_badge(meta.year.as_deref());
                                        cell.set_album(&meta.album, &meta.album_artist, meta.year.as_deref(), &meta.album);
                                        cell.set_group_captions(caption.as_deref().unwrap_or(&[]));
                                        cell.set_year_badge(year_badge.as_deref());
                                        let cached = tc.borrow().get(&key).cloned();
                                        if let Some(tex) = cached {
                                            cell.set_cover_texture(&tex);
                                        } else if let Some(p) = cp.borrow().get(&key).and_then(|o| o.clone()) {
                                            if let Ok(img) = image::open(&p) {
                                                let rgba = image::imageops::resize(&img.to_rgba8(), 200, 200, image::imageops::FilterType::Lanczos3);
                                                let bytes = glib::Bytes::from_owned(rgba.into_raw());
                                                let tex = gdk4::MemoryTexture::new(200, 200, gdk4::MemoryFormat::R8g8b8a8, &bytes, 200 * 4);
                                                tc.borrow_mut().insert(key.clone(), tex.clone().into());
                                                cell.set_cover_texture(&tex);
                                            } else {
                                                cell.set_cover_texture(&placeholder_texture(&meta.album_artist));
                                            }
                                        } else {
                                            cell.set_cover_texture(&placeholder_texture(&meta.album_artist));
                                        }
                                        // Group caption label (created only for first item in group)
                                        let caption_label = if group_value.is_some() && new_cells.last().map_or(true, |c: &AlbumCell| c.group_value.as_deref() != group_value.as_deref()) {
                                            let lbl = Label::new(Some(header));
                                            lbl.set_halign(gtk4::Align::Start);
                                            lbl.set_valign(gtk4::Align::Center);
                                            lbl.set_css_classes(&["group-caption"]);
                                            lbl.set_height_request((CAPTION_H - 4.0) as i32);
                                            layout.put(&lbl, 0.0, 0.0);
                                            Some(lbl)
                                        } else {
                                            None
                                        };
                                        layout.put(&cell, 0.0, 0.0);
                                        new_cells.push(AlbumCell {
                                            cell,
                                            caption: caption_label,
                                            artist: meta.album_artist.clone(),
                                            album: meta.album.clone(),
                                            album_id: key.clone(),
                                            group_value: group_value.clone(),
                                        });
                                        all_albums.push((meta.album_artist.clone(), meta.album.clone()));
                                    }
                                }
                                *cells_rc.borrow_mut() = new_cells;
                                let sw = fc_scroll.clone();
                                let w = sw.width() as f64;
                                if w > 0.0 {
                                    let cells = cells_rc.borrow();
                                    reposition(&layout, &cells, w);
                                }
                                fc_stack.set_visible_child(&fc_scroll);
                                if !all_albums.is_empty() {
                                    log::debug!("[ui] grouped: enqueuing {} albums for cover fetch", all_albums.len());
                                    let _ = cmd.send(MpdCommand::FetchCovers(all_albums));
                                }
                            }
                        }
                        MpdEvent::SearchResults { results, generation } => {
                            // Discard stale results from slower queries
                            if generation != 0 && generation != search_gen.get() {
                                log::debug!("[ui] Discarding stale SearchResults (gen {generation}, current {cur})", cur = search_gen.get());
                                return glib::ControlFlow::Continue;
                            }
                            if results.is_empty() {
                                fc_empty.set_text("No results found");
                                fc_stack.set_visible_child(&fc_empty);
                            } else {
                                let layout = fc_layout.clone();
                                let cells_rc = fc_ev_cells.clone();
                                let cp = fc_ev_cover_paths.clone();
                                let tc = fc_ev_cover_tex_cache.clone();
                                let cmd = fc_cmd.clone();
                                while let Some(child) = layout.first_child() {
                                    layout.remove(&child);
                                }
                                let mut new_cells: Vec<AlbumCell> = Vec::with_capacity(results.len());
                                for (_i, (artist, name)) in results.iter().enumerate() {
                                    let cell = AlbumCoverCell::new(cmd.clone());
                                    let key = crate::coverart::cover_key(artist, name);
                                    let year = fc_mc.get(artist, name).and_then(|m| m.year.clone());
                                    let year_badge = year.as_deref().and_then(|y_str| format_year_badge(Some(y_str)));
                                    cell.set_album(name, artist, year.as_deref(), name);
                                    cell.set_year_badge(year_badge.as_deref());
                                    let cached = tc.borrow().get(&key).cloned();
                                    if let Some(tex) = cached {
                                        cell.set_cover_texture(&tex);
                                    } else if let Some(p) = cp.borrow().get(&key).and_then(|o| o.clone()) {
                                        if let Ok(img) = image::open(&p) {
                                            let rgba = image::imageops::resize(&img.to_rgba8(), 200, 200, image::imageops::FilterType::Lanczos3);
                                            let bytes = glib::Bytes::from_owned(rgba.into_raw());
                                            let tex = gdk4::MemoryTexture::new(200, 200, gdk4::MemoryFormat::R8g8b8a8, &bytes, 200 * 4);
                                            tc.borrow_mut().insert(key.clone(), tex.clone().into());
                                            cell.set_cover_texture(&tex);
                                        } else {
                                            cell.set_cover_texture(&placeholder_texture(artist));
                                        }
                                    } else {
                                        cell.set_cover_texture(&placeholder_texture(artist));
                                    }
                                    layout.put(&cell, 0.0, 0.0);
                                    new_cells.push(AlbumCell {
                                        cell,
                                        caption: None,
                                        artist: artist.clone(),
                                        album: name.clone(),
                                        album_id: key.clone(),
                                        group_value: None,
                                    });
                                }
                                *cells_rc.borrow_mut() = new_cells;
                                let sw = fc_scroll.clone();
                                let w = sw.width() as f64;
                                if w > 0.0 {
                                    let cells = cells_rc.borrow();
                                    reposition(&layout, &cells, w);
                                }
                                fc_stack.set_visible_child(&fc_scroll);
                                // Enqueue cover art fetch for search results
                                let _ = cmd.send(MpdCommand::FetchCovers(results.clone()));
                            }
                        }
                        MpdEvent::FileSearchResults(results) => {
                            // Populate folder search results
                            fc_fs_list.remove_all();
                            for (path, name) in &results {
                                let row = gtk4::ListBoxRow::new();
                                let hbox = Box::new(Orientation::Horizontal, 6);
                                hbox.set_margin_start(12);
                                hbox.set_margin_top(3);
                                hbox.set_margin_bottom(3);
                                hbox.set_css_classes(&["folder-file-row"]);
                                let icon = gtk4::Image::from_icon_name("audio-x-generic-symbolic");
                                icon.set_pixel_size(16);
                                hbox.append(&icon);
                                let lbl = gtk4::Label::new(Some(name));
                                lbl.set_halign(gtk4::Align::Start);
                                lbl.set_hexpand(true);
                                lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                                hbox.append(&lbl);
                                let path_lbl = gtk4::Label::new(Some(path));
                                path_lbl.set_css_classes(&["format-badge"]);
                                hbox.append(&path_lbl);
                                row.set_child(Some(&hbox));
                                fc_fs_list.append(&row);
                            }
                            if !results.is_empty() {
                                fc_fs_container.set_visible(true);
                                fc_fb.borrow().container.set_visible(false);
                            }
                        }
                        MpdEvent::DirectoryListing(path, entries) => {
                            if let Ok(mut fb) = fc_fb.try_borrow_mut() {
                                fb.set_entries(&path, entries);
                            }
                        }
                        MpdEvent::AlbumTracks(tracks) => {
                            *fc_popover_track_data.borrow_mut() = tracks.clone();
                            fc_track_listbox.remove_all();
                            for (title, _file, duration) in &tracks {
                                let row = gtk4::ListBoxRow::new();
                                let hbox = Box::new(Orientation::Horizontal, 12);
                                hbox.set_margin_start(8);
                                hbox.set_margin_end(8);
                                hbox.set_margin_top(4);
                                hbox.set_margin_bottom(4);
                                let title_lbl = Label::new(Some(title));
                                title_lbl.set_halign(gtk4::Align::Start);
                                title_lbl.set_hexpand(true);
                                title_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                                hbox.append(&title_lbl);
                                if *duration > 0.0 {
                                    let dur_text = format!("{}:{:02}", *duration as u64 / 60, *duration as u64 % 60);
                                    let dur_lbl = Label::new(Some(&dur_text));
                                    dur_lbl.set_halign(gtk4::Align::End);
                                    dur_lbl.set_css_classes(&["time-display"]);
                                    hbox.append(&dur_lbl);
                                }
                                row.set_child(Some(&hbox));
                                fc_track_listbox.append(&row);
                            }
                            // Select current track if it belongs to this album
                            if let Some(csp) = fc_current_song_pos.get() {
                                let entries = fc_queue_entries.borrow();
                                if let Some(current_entry) = entries.iter().find(|e| e.position == csp) {
                                    let current_file = &current_entry.file;
                                    let data = fc_popover_track_data.borrow();
                                    if let Some(pos) = data.iter().position(|(_, f, _)| f == current_file) {
                                        if let Some(row) = fc_track_listbox.row_at_index(pos as i32) {
                                            fc_track_listbox.select_row(Some(&row));
                                        }
                                    }
                                }
                            }
                        }
                        MpdEvent::Queue(queue) => {
                            fc_ql.remove_all();
                            *fc_queue_entries.borrow_mut() = queue.clone();
                            let csp = fc_current_song_pos.get();
                            let q_tx = fc_cmd.clone();
                            let mut item_ids = HashMap::new();
                            let mut current_row: Option<gtk4::ListBoxRow> = None;
                            for (row_idx, item) in queue.iter().enumerate() {
                                item_ids.insert(row_idx as i32, item.id);
                                let row = gtk4::ListBoxRow::new();
                                let vbox = Box::new(Orientation::Vertical, 0);
                                vbox.set_margin_start(8);
                                vbox.set_margin_top(2);
                                vbox.set_margin_bottom(2);
                                let dur = item.duration.map(|d| {
                                    let t = d as u64;
                                    format!("{}:{:02}", t / 60, t % 60)
                                }).unwrap_or_default();
                                let title = item.title.as_deref().unwrap_or(&item.file);
                                let title_lbl = Label::new(Some(title));
                                title_lbl.set_halign(gtk4::Align::Start);
                                title_lbl.set_hexpand(true);
                                title_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                                let meta_box = Box::new(Orientation::Horizontal, 4);
                                let artist = item.artist.as_deref().unwrap_or("");
                                let artist_lbl = Label::new(Some(artist));
                                artist_lbl.set_halign(gtk4::Align::Start);
                                artist_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                                artist_lbl.set_css_classes(&["queue-artist"]);
                                let dur_lbl = Label::new(Some(&dur));
                                dur_lbl.set_halign(gtk4::Align::End);
                                meta_box.append(&artist_lbl);
                                meta_box.append(&dur_lbl);
                                vbox.append(&title_lbl);
                                vbox.append(&meta_box);
                                row.set_child(Some(&vbox));
                                if csp == Some(item.position) {
                                    row.set_css_classes(&["queue-current"]);
                                    current_row = Some(row.clone());
                                }
                                // GtkDragSource for drag-reorder
                                let reorder_ds = DragSource::new();
                                reorder_ds.set_actions(DragAction::MOVE);
                                let drag_row_id = item.id;
                                reorder_ds.connect_prepare(move |_source, _x, _y| {
                                    let data = format!("{}:{}", drag_row_id, 0);
                                    let value = glib::Value::from(&data);
                                    Some(ContentProvider::for_value(&value))
                                });
                                row.add_controller(reorder_ds);
                                // Double-click to play
                                let dbl = gtk4::GestureClick::new();
                                dbl.set_button(1);
                                let tx_play = q_tx.clone();
                                let item_pos = item.position;
                                dbl.connect_pressed(move |_gest, n_clicks, _x, _y| {
                                    if n_clicks == 2 {
                                        let _ = tx_play.send(MpdCommand::PlayPosition(item_pos));
                                    }
                                });
                                row.add_controller(dbl);
                                // Right-click context menu popover.
                                // set_button(3) + Capture phase avoids gesture arbitration conflicts.
                                let rclick = gtk4::GestureClick::new();
                                rclick.set_button(3);
                                rclick.set_propagation_phase(gtk4::PropagationPhase::Capture);
                                let tx_pop = q_tx.clone();
                                let ipos = item.position;
                                let iid = item.id;
                                let current_song = fc_current_song_pos.get();
                                let qp = fc_queue_popover.clone();
                                let row_for_pop = row.clone();
                                rclick.connect_pressed(move |_gest, _n, x, y| {
                                    log::debug!("Queue context menu fired, pos={ipos}");
                                    if let Some(ref old) = *qp.borrow() {
                                        old.popdown();
                                    }
                                    let pop = gtk4::Popover::new();
                                    let popbox = Box::new(Orientation::Vertical, 0);
                                    let btn_play = gtk4::Button::with_label("Play Now");
                                    let btn_next = gtk4::Button::with_label("Play Next");
                                    let btn_rem = gtk4::Button::with_label("Remove");
                                    let pop_close = qp.clone();
                                    let tp = tx_pop.clone();
                                    let ip = ipos;
                                    let pc = pop_close.clone();
                                    btn_play.connect_clicked(move |_| {
                                        let _ = tp.send(MpdCommand::PlayPosition(ip));
                                        if let Some(ref p) = *pc.borrow() { p.popdown(); }
                                    });
                                    let tn = tx_pop.clone();
                                    let id_next = iid;
                                    let target = current_song.map(|p| p + 1).unwrap_or(0);
                                    let pc = pop_close.clone();
                                    btn_next.connect_clicked(move |_| {
                                        let _ = tn.send(MpdCommand::MoveId(id_next, target));
                                        if let Some(ref p) = *pc.borrow() { p.popdown(); }
                                    });
                                    let tr = tx_pop.clone();
                                    let pc = pop_close.clone();
                                    btn_rem.connect_clicked(move |_| {
                                        let _ = tr.send(MpdCommand::DeleteId(iid));
                                        if let Some(ref p) = *pc.borrow() { p.popdown(); }
                                    });
                                    popbox.append(&btn_play);
                                    popbox.append(&btn_next);
                                    popbox.append(&btn_rem);
                                    pop.set_child(Some(&popbox));
                                    pop.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                                    pop.set_parent(row_for_pop.upcast_ref::<gtk4::Widget>());
                                    pop.popup();
                                    *qp.borrow_mut() = Some(pop);
                                });
                                row.add_controller(rclick);
                                fc_ql.append(&row);
                            }
                            // Select the current track's row in the list
                            if let Some(ref cr) = current_row {
                                fc_ql.select_row(Some(cr));
                            }
                            // Refresh shared item_ids for key-based delete/move lookup
                            *fc_ids.borrow_mut() = item_ids;

                            // Populate mini grid from queue data (Album Mode album-level grouping)
                            let mut seen: Vec<(String, String, i32)> = Vec::new();
                            let mut found_current: Option<String> = None;
                            for item in &queue {
                                if let Some(ref album) = item.album {
                                    let artist = item.artist.as_deref().unwrap_or("");
                                    if !seen.iter().any(|(a, _, _)| a == album) {
                                        seen.push((album.clone(), artist.to_string(), item.position));
                                    }
                                    if csp == Some(item.position) {
                                        found_current = Some(album.clone());
                                    }
                                }
                            }
                            *fc_mini_current.borrow_mut() = found_current;
                            let mini_items: Vec<MiniGridItem> = seen.into_iter()
                                .map(|(album, artist, first_pos)| MiniGridItem { album, artist, first_pos })
                                .collect();
                            *fc_mini_data.borrow_mut() = mini_items;

                            // Rebuild Fixed layout with new items
                            let current = fc_mini_current.borrow().clone();
                            let vpw = fc_mini_scroll_ref.width();
                            let cells = rebuild_mini_fixed(
                                &fc_mini_fixed,
                                &fc_mini_data.borrow(),
                                &fc_ev_cover_paths,
                                &fc_mini_cw,
                                &current,
                                &fc_cmd,
                                vpw,
                            );
                            *fc_mini_cells.borrow_mut() = cells;
                        }
                        MpdEvent::CoverPaths(paths) => {
                            log::info!("[UI] CoverPaths: {} covers, keys: {:?}", paths.len(), paths.keys().collect::<Vec<_>>());
                            let mut cp = fc_ev_cover_paths.borrow_mut();
                            let mini_widgets = fc_mini_cw.borrow();
                            for (album, path) in &paths {
                                cp.insert(album.clone(), path.clone());
                                if let Some(p) = path.as_deref() {
                                    if let Ok(img) = image::open(p) {
                                        let rgba = image::imageops::resize(&img.to_rgba8(), 200, 200, image::imageops::FilterType::Lanczos3);
                                        let bytes = glib::Bytes::from_owned(rgba.into_raw());
                                        let tex = gdk4::MemoryTexture::new(200, 200, gdk4::MemoryFormat::R8g8b8a8, &bytes, 200 * 4);
                                        fc_ev_cover_tex_cache.borrow_mut().insert(album.clone(), tex.clone().into());
                                        log::info!("[UI] cover push: '{album}' tex={}x{}",
                                            tex.width(), tex.height());
                                        // Update grid cell in-place by album_id
                                        let cells = fc_ev_cells.borrow();
                                        if let Some(cell) = cells.iter().find(|c| &c.album_id == album) {
                                            cell.cell.set_cover_texture(&tex);
                                        }
                                        drop(cells);
                                        // Update mini queue grid
                                        if let Some(pic) = mini_widgets.get(album) {
                                            pic.set_paintable(Some(&tex));
                                            pic.set_visible(true);
                                            pic.queue_draw();
                                        }
                                        let is_current = fc_current_album.borrow().as_deref()
                                            .map(|a| album.ends_with(&format!("||{}", a)))
                                            .unwrap_or(false);
                                        if is_current {
                                            fc_np_cover.set_paintable(Some(&tex));
                                            fc_np_cover.set_visible(true);
                                        }
                                    }
                                }
                            }
                        }
                        MpdEvent::CoverRefreshed { album_id, data } => {
                            let data_len = data.len();
                            log::info!("[UI] cover refreshed: '{album_id}' ({} bytes RGBA)", data_len);
                            let rgba = glib::Bytes::from_owned(data);
                            let texture = gdk4::MemoryTexture::new(
                                200, 200,
                                gdk4::MemoryFormat::R8g8b8a8,
                                &rgba,
                                200 * 4,
                            );
                            fc_ev_cover_tex_cache.borrow_mut().insert(album_id.clone(), texture.clone().into());
                            log::info!("[UI] cover refresh: '{album_id}' texture from {data_len} RGBA bytes");
                            let cells = fc_ev_cells.borrow();
                            if let Some(cell) = cells.iter().find(|c| c.album_id == album_id) {
                                cell.cell.set_cover_texture(&texture);
                            }
                            drop(cells);
                            if let Some(pic) = fc_mini_cw.borrow().get(&album_id) {
                                pic.set_paintable(Some(&texture));
                                pic.set_visible(true);
                                pic.queue_draw();
                            }
                            let is_current = fc_current_album.borrow().as_deref()
                                .map(|a| album_id.ends_with(&format!("||{}", a)))
                                .unwrap_or(false);
                            if is_current {
                                fc_np_cover.set_paintable(Some(&texture));
                                fc_np_cover.set_visible(true);
                            }
                        }
                        MpdEvent::LibraryChanged => {
                            fc_scmd.send(SearchCommand::Reset);
                            let _ = fc_cmd.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                        }
                        MpdEvent::Error(msg) => {
                            fc_toast.add_toast(adw::Toast::new(&format!("MPD Error: {msg}")));
                        }
                        MpdEvent::Toast { message, level } => {
                            let timeout = match level {
                                crate::mpd::state_machine::ToastLevel::Error => 0u32,
                                crate::mpd::state_machine::ToastLevel::Warn => 5,
                                crate::mpd::state_machine::ToastLevel::Info => 3,
                            };
                            let toast = adw::Toast::new(&message);
                            toast.set_timeout(timeout);
                            fc_toast.add_toast(toast);
                        }
                    }
                    let _ = ftx.try_send(fwd);
                    guard = match fc_rx.lock() {
                        Ok(g) => g,
                        Err(poisoned) => {
                            log::error!("MPD event receiver mutex poisoned: {poisoned}");
                            fc_shutdown.quit();
                            return glib::ControlFlow::Break;
                        }
                    };
                }
                if let Err(mpsc::TryRecvError::Disconnected) = guard.try_recv() {
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            });

            // Load CSS from external file (embedded at compile time via include_str!)
            let css = gtk4::CssProvider::new();
            css.load_from_bytes(&glib::Bytes::from_static(include_str!("style.css").as_bytes()));
            gtk4::style_context_add_provider_for_display(
                &gtk4::prelude::WidgetExt::display(&window),
                &css,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );

            // High contrast CSS support (accessibility)
            let hc_css_cell: std::cell::Cell<Option<gtk4::CssProvider>> = std::cell::Cell::new(None);
            let should_enable_hc = cfg.high_contrast
                || std::env::var("GTK_THEME").as_deref() == Ok("HighContrast")
                || std::env::var("GTK_THEME").as_deref() == Ok("Adwaita:highcontrast");
            if should_enable_hc {
                let hc_provider = gtk4::CssProvider::new();
                hc_provider.load_from_string(HC_CSS);
                gtk4::style_context_add_provider_for_display(
                    &gtk4::prelude::WidgetExt::display(&window),
                    &hc_provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
                );
                hc_css_cell.set(Some(hc_provider));
            }
        });

        application.run();
    }
}

/// Find the first track position of the previous (forward=false) or next (forward=true)
/// album in the queue, relative to the currently playing track's album.
fn find_album_boundary(queue: &[crate::mpd::QueueEntry], current_position: Option<i32>, forward: bool) -> Option<i32> {
    let csp = current_position?;
    let current_album = queue.iter()
        .find(|e| e.position == csp)
        .and_then(|e| e.album.clone())?;

    let mut album_order: Vec<&str> = Vec::new();
    for item in queue.iter() {
        if let Some(ref album) = item.album {
            if !album_order.contains(&album.as_str()) {
                album_order.push(album.as_str());
            }
        }
    }

    let cur_idx = album_order.iter().position(|a| *a == current_album.as_str())?;

    if forward {
        let target = album_order.get(cur_idx + 1)?;
        queue.iter().find(|e| e.album.as_deref() == Some(target)).map(|e| e.position)
    } else {
        if cur_idx == 0 { return None; }
        let target = album_order.get(cur_idx - 1)?;
        queue.iter().find(|e| e.album.as_deref() == Some(target)).map(|e| e.position)
    }
}

struct NowPlayingWidgets<'a> {
    title: &'a gtk4::Button,
    artist: &'a Label,
    album: &'a Label,
    year: &'a Label,
    status_dot: &'a Box,
    play_pause_btn: &'a gtk4::Button,
    pos_label: &'a Label,
    len_label: &'a Label,
    seekbar: &'a gtk4::Scale,
    fmt_label: &'a Label,
    bitrate_label: &'a Label,
    cover: &'a Picture,
    cover_stack: &'a gtk4::Stack,
    cover_paths: &'a std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Option<String>>>>,
}

/// View model for now-playing display — derived from PlaybackUpdate.
#[allow(dead_code)]
struct PlaybackDisplay {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    year: Option<String>,
    state: String,
    elapsed: Option<f64>,
    duration: Option<f64>,
    format_badge: Option<String>,
    bitrate: Option<String>,
    volume: i16,
    song_id: Option<u32>,
    file: Option<String>,
}

impl PlaybackDisplay {
    fn from_update(update: &PlaybackUpdate) -> Self {
        let mut format_badge = update.format.clone();
        // Augment with codec from audio_format if available
        if let Some(ref af) = update.audio_format {
            let text = af.display_text();
            if !text.is_empty() {
                if let Some(ref existing) = format_badge {
                    if !existing.contains(&text) {
                        format_badge = Some(format!("{} · {}", text, existing));
                    }
                } else {
                    format_badge = Some(text);
                }
            }
        }
        // Add file extension as codec hint
        if let Some(ref file) = update.file {
            if let Some(ext) = std::path::Path::new(file)
                .extension()
                .and_then(|e| e.to_str())
            {
                let ext_upper = ext.to_uppercase();
                if !format_badge.as_ref().map_or(false, |f| f.contains(&ext_upper)) {
                    if let Some(ref existing) = format_badge {
                        format_badge = Some(format!("{} · {}", existing, ext_upper));
                    } else {
                        format_badge = Some(ext_upper);
                    }
                }
            }
        }

        PlaybackDisplay {
            title: update.title.clone().unwrap_or_default(),
            artist: update.artist.clone(),
            album: update.album.clone(),
            year: update.year.clone(),
            state: update.state.clone(),
            elapsed: update.elapsed,
            duration: update.duration,
            format_badge,
            bitrate: update.bitrate.clone(),
            volume: update.volume,
            song_id: update.song,
            file: update.file.clone(),
        }
    }
}

/// Update now-playing widgets and forward to MPRIS after state has been written by `reduce()`.
/// SharedState.current must already be populated before calling this function.
fn handle_now_playing(
    update: &PlaybackUpdate,
    w: NowPlayingWidgets,
    mpris_tx: &std::sync::mpsc::Sender<PlaybackUpdate>,
) {
    let display = PlaybackDisplay::from_update(update);
    update_now_playing(w, &display);
    let _ = mpris_tx.send(update.clone());
}

fn update_now_playing(
    w: NowPlayingWidgets,
    display: &PlaybackDisplay,
) {
    let has_track = display.artist.is_some() || display.album.is_some();

    if !display.title.is_empty() {
        w.title.set_label(&display.title);
    } else {
        w.title.set_label(if has_track { "" } else { "No track playing" });
    }
    if let Some(ref a) = display.artist {
        w.artist.set_text(a);
        w.artist.set_visible(true);
    } else {
        w.artist.set_visible(false);
        w.artist.set_text("");
    }
    if let Some(ref a) = display.album {
        w.album.set_text(a);
        w.album.set_visible(true);
        if let Some(ref y) = display.year {
            w.year.set_text(y);
            w.year.set_visible(true);
        } else {
            w.year.set_visible(false);
            w.year.set_text("");
        }
        let cp = w.cover_paths.borrow();
        let suffix = format!("||{}", a);
        let path = cp.iter()
            .find(|(k, _)| k.ends_with(&suffix))
            .and_then(|(_, v)| v.as_deref());
        if let Some(p) = path {
            w.cover.set_filename(Some(p));
            w.cover.set_visible(true);
            w.cover_stack.set_visible_child_name("cover");
        }
    } else {
        w.album.set_visible(false);
        w.album.set_text("");
        w.year.set_visible(false);
        w.year.set_text("");
        w.cover.set_visible(false);
        w.cover_stack.set_visible_child_name("placeholder");
    }
    match (display.elapsed, display.duration) {
        (Some(el), Some(dur)) => {
            w.pos_label.set_text(&format!("{}:{:02}", el as u64 / 60, el as u64 % 60));
            w.len_label.set_text(&format!("{}:{:02}", dur as u64 / 60, dur as u64 % 60));
            let adj = w.seekbar.adjustment();
            adj.set_upper(dur);
            adj.set_value(el);
        }
        _ => {
            w.pos_label.set_text("--:--");
            w.len_label.set_text("--:--");
            w.seekbar.adjustment().set_upper(0.0);
            w.seekbar.adjustment().set_value(0.0);
        }
    }

    if let Some(ref fmt) = display.format_badge {
        w.fmt_label.set_text(fmt);
        w.fmt_label.set_visible(true);
    } else {
        w.fmt_label.set_visible(false);
    }
    if let Some(ref br) = display.bitrate {
        w.bitrate_label.set_text(&format!("{}kb/s", br));
        w.bitrate_label.set_visible(true);
    } else {
        w.bitrate_label.set_visible(false);
    }
    match display.state.as_str() {
        "play" => {
            w.play_pause_btn.set_child(Some(&gtk4::Image::from_icon_name("media-playback-pause-symbolic")));
            w.status_dot.set_css_classes(&["status-dot", "playing"]);
        }
        "pause" => {
            w.play_pause_btn.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
            w.status_dot.set_css_classes(&["status-dot", "paused"]);
        }
        "stop" | "" => {
            w.play_pause_btn.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
            w.status_dot.set_css_classes(&["status-dot", "stopped"]);
        }
        _ => {
            log::warn!("Unknown playback state: {}", display.state);
            w.play_pause_btn.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
            w.status_dot.set_css_classes(&["status-dot", "stopped"]);
        }
    }
}

