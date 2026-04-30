//! UI layer — GTK4 widgets and window management. Thread: UI (GTK main loop).

pub mod widgets;

use crate::config::Config;
use crate::mpd::state_machine::{MpdCommand, MpdEvent, PlaybackUpdate};
use crate::search::SearchIndex;
use crate::state::SharedState;
use gtk4::prelude::*;
use gtk4::gio::ListStore;
use gtk4::{Application, ApplicationWindow, Box, Button, EventControllerKey, GridView, Label, ListBox, NoSelection, Orientation, Overlay, Paned, Picture, ScrolledWindow, SignalListItemFactory, StringObject};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};

/// Album grid item types with pre-computed placeholder RGB.
#[derive(Clone)]
enum AlbumGridItem {
    Header { name: String, count: u32 },
    Album { artist: String, name: String, album_id: String, pr: f64, pg: f64, pb: f64 },
}

/// Backing data store for the album grid.
type AlbumGridData = std::rc::Rc<std::cell::RefCell<Vec<AlbumGridItem>>>;

/// Store a string value on a GLib Object (safe wrapper for use in factory closures).
unsafe fn widget_set_str(w: &impl IsA<glib::Object>, key: &str, val: &str) {
    unsafe { w.set_data(key, val.to_string()); }
}
/// Read a string previously stored with widget_set_str.
unsafe fn widget_get_str(w: &impl IsA<glib::Object>, key: &str) -> Option<String> {
    unsafe { w.data::<String>(key).map(|p| p.as_ref().clone()) }
}

/// Pre-compute placeholder RGB from an artist name.
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

/// Data key used for storing album name on overlay widgets (for button closures).
const WIDGET_ALBUM_KEY: &str = "g-album";

/// Batch-populate a ListStore from AlbumGridItem data using StringObject indices.
/// Adds items in batches of 16 per idle cycle for responsive loading.
/// Uses an incrementing generation counter to cancel stale in-flight populates.
fn batch_populate(model: &ListStore, backing: &AlbumGridData, items: Vec<AlbumGridItem>,
                  cover_widgets: &std::rc::Rc<std::cell::RefCell<HashMap<String, gtk4::Picture>>>
                  ) {
    use std::cell::Cell;
    use std::rc::Rc;

    // Cancel previous in-flight populate by incrementing generation.
    // The generation lives on the model as widget data.
    let cur_gen = unsafe {
        model.data::<u64>("bp-gen")
            .map(|p| p.as_ref().wrapping_add(1))
            .unwrap_or(1)
    };
    unsafe { model.set_data("bp-gen", cur_gen); }

    cover_widgets.borrow_mut().clear();
    model.remove_all();
    *backing.borrow_mut() = items;
    let total = backing.borrow().len();
    if total == 0 { return; }

    let m = model.clone();
    let pos = Rc::new(Cell::new(0usize));
    glib::idle_add_local(move || {
        // Abort if this populate was superseded
        if unsafe { m.data::<u64>("bp-gen") }.map_or(true, |p| unsafe { *p.as_ref() } != cur_gen) {
            return glib::ControlFlow::Break;
        }
        let start = pos.get();
        let end = std::cmp::min(start + 16, total);
        for i in start..end {
            m.append(&StringObject::new(&i.to_string()));
        }
        pos.set(end);
        if end >= total { glib::ControlFlow::Break } else { glib::ControlFlow::Continue }
    });
}

/// Calculate which albums are in the visible viewport (plus a buffer of ±1 row)
/// of the album grid, based on scroll position and estimated cell size.
///
/// Falls back to all albums when the layout hasn't settled yet (page_size == 0),
/// which happens during initial population before the first allocation pass.
fn calculate_visible_albums(
    backing: &AlbumGridData,
    vadj: &gtk4::Adjustment,
    grid: &GridView,
) -> Vec<(String, String)> {
    let scroll_top = vadj.value();
    let page_size = vadj.page_size();
    let total_items = backing.borrow().len();
    if total_items == 0 {
        return vec![];
    }

    // Layout not yet settled — return all albums (safe fallback)
    if page_size <= 0.0 {
        let binding = backing.borrow();
        return binding.iter().filter_map(|item| {
            if let AlbumGridItem::Album { artist, name, .. } = item {
                Some((artist.clone(), name.clone()))
            } else {
                None
            }
        }).collect();
    }

    // Estimate items per row from the grid's allocated width.
    // Cell minimum width is 200px per size_request in factory setup.
    let grid_width = grid.width() as f64;
    let items_per_row = if grid_width > 0.0 {
        std::cmp::max(1, (grid_width / 200.0_f64).floor() as usize)
    } else {
        // Unknown grid width — use a reasonable default for a typical window
        4usize
    };

    // Cell minimum height is 250px per size_request.
    let row_height = 250.0_f64;
    let first_row = (scroll_top / row_height).floor() as usize;
    let visible_rows = (page_size / row_height).ceil() as usize + 2; // +2 for buffer rows (±1)

    let start_idx = first_row.saturating_mul(items_per_row);
    let end_idx = (first_row + visible_rows).saturating_mul(items_per_row);

    let binding = backing.borrow();
    let mut visible = Vec::new();
    for i in start_idx..end_idx.min(binding.len()) {
        if let AlbumGridItem::Album { artist, name, .. } = &binding[i] {
            visible.push((artist.clone(), name.clone()));
        }
    }
    visible
}

type SharedIds = std::rc::Rc<std::cell::RefCell<HashMap<i32, i32>>>;

pub struct App {
    state: SharedState,
    event_rx: Arc<Mutex<mpsc::Receiver<MpdEvent>>>,
    cmd_tx: mpsc::Sender<MpdCommand>,
    conn_params: Arc<Mutex<(String, u16)>>,
}

impl App {
    pub fn new(
        state: SharedState,
        event_rx: mpsc::Receiver<MpdEvent>,
        cmd_tx: mpsc::Sender<MpdCommand>,
        conn_params: Arc<Mutex<(String, u16)>>,
    ) -> Self {
        Self {
            state,
            event_rx: Arc::new(Mutex::new(event_rx)),
            cmd_tx,
            conn_params,
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

        let file_menu = gtk4::gio::Menu::new();
        file_menu.append(Some("_Quit"), Some("app.quit"));

        // View menu with mode switching
        let view_menu = gtk4::gio::Menu::new();
        view_menu.append(Some("_Album Mode"), Some("app.album-mode"));
        view_menu.append(Some("_Folder Mode"), Some("app.folder-mode"));

        let help_menu = gtk4::gio::Menu::new();
        help_menu.append(Some("_Keyboard Shortcuts"), Some("app.shortcuts"));

        let menubar = gtk4::gio::Menu::new();
        menubar.append_submenu(Some("_File"), &file_menu);
        menubar.append_submenu(Some("_View"), &view_menu);
        menubar.append_submenu(Some("_Help"), &help_menu);
        application.set_menubar(Some(&menubar));

        // Mode switching actions — application.add_action will be called inside connect_activate

        application.connect_activate(move |window_app| {
            // Clone early for the shutdown timer closure; window_app is consumed by the builder below.
            let shutdown_app = window_app.clone();
            let cfg = Config::load();

            let window = ApplicationWindow::builder()
                .application(window_app)
                .default_width(1200)
                .default_height(800)
                .title("MPD Client")
                .show_menubar(true)
                .build();

            let paned = Paned::new(Orientation::Horizontal);

            // --- Left pane: group bar + album grid ---
            let left_pane_box = Box::new(Orientation::Vertical, 0);

            // Group selector bar — display label maps to MPD tag name
            let group_bar = Box::new(Orientation::Horizontal, 2);
            group_bar.set_margin_start(4);
            group_bar.set_margin_end(4);
            group_bar.set_margin_top(4);
            let group_tx = cmd_tx.clone();
            let groups = [("Albums", "Albums"), ("Artists", "Artist"), ("Years", "Date"), ("Genres", "Genre")];
            let mut first_btn: Option<gtk4::ToggleButton> = None;
            for (display, mpd_tag) in &groups {
                let btn = gtk4::ToggleButton::with_label(display);
                btn.set_css_classes(&["group-select-btn"]);
                if let Some(ref fb) = first_btn {
                    btn.set_group(Some(fb));
                } else {
                    btn.set_active(true);
                }
                if first_btn.is_none() {
                    first_btn = Some(btn.clone());
                }
                let tx = group_tx.clone();
                let tag = mpd_tag.to_string();
                btn.connect_toggled(move |b| {
                    if b.is_active() { let _ = tx.send(MpdCommand::ListAlbumsGrouped(tag.clone())); }
                });
                group_bar.append(&btn);
            }

            let left_scroll = ScrolledWindow::new();
            left_scroll.set_vexpand(true);
            left_scroll.set_hexpand(true);

            // --- GtkGridView + factory for virtualized album grid ---
            let album_model: ListStore = ListStore::builder()
                .item_type(StringObject::static_type())
                .build();
            let album_factory = SignalListItemFactory::new();
            let album_grid_data: AlbumGridData = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let left_stack = gtk4::Stack::new();

            // Shared cover tracking
            let cover_paths: std::rc::Rc<std::cell::RefCell<HashMap<String, Option<String>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            let cover_widgets: std::rc::Rc<std::cell::RefCell<HashMap<String, gtk4::Picture>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));

            // Factory setup: create widget shell for each recycled list item.
            // Both header and album widgets are created; visibility toggled in bind.
            let setup_tx = cmd_tx.clone();
            album_factory.connect_setup(move |_factory, item| {
                let list_item = item.downcast_ref::<gtk4::ListItem>().unwrap();
                let container = Box::new(Orientation::Vertical, 0);
                container.set_size_request(200, 250);
                container.set_css_classes(&["album-cover-cell"]);

                // Header label — hidden by default
                let header_label = Label::new(None);
                header_label.set_halign(gtk4::Align::Start);
                header_label.set_valign(gtk4::Align::Center);
                header_label.set_size_request(200, 250);
                header_label.set_css_classes(&["album-group-header"]);
                header_label.set_visible(false);
                container.append(&header_label);

                // Album content — hidden by default
                let album_section = Box::new(Orientation::Vertical, 0);
                album_section.set_visible(false);

                let overlay = Overlay::new();
                let cover_area = Box::new(Orientation::Vertical, 0);
                cover_area.set_size_request(200, 200);

                let placeholder = gtk4::DrawingArea::new();
                placeholder.set_size_request(200, 200);
                cover_area.append(&placeholder);

                let cover_image = Picture::new();
                cover_image.set_widget_name("cover-image");
                cover_image.set_size_request(200, 200);
                cover_image.set_halign(gtk4::Align::Center);
                cover_image.set_valign(gtk4::Align::Center);
                cover_image.set_visible(false);
                cover_area.append(&cover_image);

                overlay.set_child(Some(&cover_area));

                // Hover buttons — album name read from overlay at click time
                let tx_add = setup_tx.clone();
                let btn_add = Button::with_label("+");
                btn_add.set_css_classes(&["album-cover-hover-btn"]);
                btn_add.set_tooltip_text(Some("Add to queue"));
                btn_add.connect_clicked(move |btn| {
                    let name = btn.parent().and_then(|p| p.parent())
                        .and_then(|p| unsafe { widget_get_str(&p, WIDGET_ALBUM_KEY) });
                    if let Some(n) = name { let _ = tx_add.send(MpdCommand::Add(n)); }
                });

                let tx_next = setup_tx.clone();
                let btn_next = Button::with_label("<-");
                btn_next.set_css_classes(&["album-cover-hover-btn"]);
                btn_next.set_tooltip_text(Some("Play next"));
                btn_next.connect_clicked(move |btn| {
                    let name = btn.parent().and_then(|p| p.parent())
                        .and_then(|p| unsafe { widget_get_str(&p, WIDGET_ALBUM_KEY) });
                    if let Some(n) = name { let _ = tx_next.send(MpdCommand::InsertNext(n)); }
                });

                let tx_play = setup_tx.clone();
                let btn_play = Button::with_label(">");
                btn_play.set_css_classes(&["album-cover-hover-btn"]);
                btn_play.set_tooltip_text(Some("Clear queue and play"));
                btn_play.connect_clicked(move |btn| {
                    let name = btn.parent().and_then(|p| p.parent())
                        .and_then(|p| unsafe { widget_get_str(&p, WIDGET_ALBUM_KEY) });
                    if let Some(n) = name { let _ = tx_play.send(MpdCommand::PlayAlbum(n)); }
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

                let btn_sense = btn_box.clone();
                let motion = gtk4::EventControllerMotion::new();
                let btn_sense_e = btn_sense.clone();
                motion.connect_enter(move |_m, _x, _y| { btn_sense.set_sensitive(true); });
                motion.connect_leave(move |_m| { btn_sense_e.set_sensitive(false); });
                overlay.add_controller(motion);

                album_section.append(&overlay);

                let title_label = Label::new(None);
                title_label.set_halign(gtk4::Align::Start);
                title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                title_label.set_max_width_chars(18);
                title_label.set_lines(1);
                album_section.append(&title_label);

                let artist_label = Label::new(None);
                artist_label.set_halign(gtk4::Align::Start);
                artist_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                artist_label.set_max_width_chars(18);
                artist_label.set_lines(1);
                album_section.append(&artist_label);

                // Store album section index in container for bind to find
                container.append(&album_section);
                list_item.set_child(Some(&container));
            });

            // Factory bind: populate widget from model data
            let bind_cp = cover_paths.clone();
            let bind_cw = cover_widgets.clone();
            let bind_data = album_grid_data.clone();
            album_factory.connect_bind(move |_factory, item| {
                let list_item = item.downcast_ref::<gtk4::ListItem>().unwrap();
                let Some(obj) = list_item.item() else { return; };
                let Some(so) = obj.downcast_ref::<StringObject>() else { return; };
                let idx: usize = match so.string().parse() { Ok(i) => i, Err(_) => return };
                let binding = bind_data.borrow();
                let Some(item_data) = binding.get(idx) else { return };
                let container = list_item.child().and_then(|c| c.downcast::<Box>().ok());
                let Some(container) = container else { return };

                // Find header label (first child) and album_section (second child)
                let children: Vec<gtk4::Widget> = {
                    let mut v = Vec::new();
                    let mut child = container.first_child();
                    while let Some(c) = child { v.push(c.clone()); child = c.next_sibling(); }
                    v
                };
                let header_w = children.first().and_then(|c| c.clone().downcast::<Label>().ok());
                let album_w = children.get(1).and_then(|c| c.clone().downcast::<Box>().ok());

                match item_data {
                    AlbumGridItem::Header { name, count } => {
                        if let Some(ref lbl) = header_w { lbl.set_text(&format!("{} ({})", name, count)); lbl.set_visible(true); }
                        if let Some(ref section) = album_w { section.set_visible(false); }
                    }
                    AlbumGridItem::Album { artist, name, album_id: _id, pr, pg, pb } => {
                        if let Some(ref lbl) = header_w { lbl.set_visible(false); }
                        if let Some(ref section) = album_w {
                            section.set_visible(true);
                            // Store album name on overlay for button closures
                            if let Some(overlay) = section.first_child().and_then(|c| c.downcast::<Overlay>().ok()) {
                                unsafe { widget_set_str(&overlay, WIDGET_ALBUM_KEY, name); }
                            }
                            // Update title (second child of section, after overlay)
                            if let Some(t) = section.first_child()
                                .and_then(|c| c.next_sibling())
                                .and_then(|c| c.downcast::<Label>().ok()) { t.set_text(name); }
                            // Update artist (third child of section)
                            if let Some(a) = section.first_child()
                                .and_then(|c| c.next_sibling())
                                .and_then(|c| c.next_sibling())
                                .and_then(|c| c.downcast::<Label>().ok()) {
                                let da = if artist.is_empty() { "Unknown Artist" } else { artist.as_str() };
                                a.set_text(da);
                            }
                            // Update cover area
                            if let Some(overlay) = section.first_child().and_then(|c| c.downcast::<Overlay>().ok()) {
                                if let Some(ca) = overlay.child().and_then(|c| c.downcast::<Box>().ok()) {
                                    let has_cov = bind_cp.borrow().get(name).and_then(|o| o.as_deref()).is_some();
                                    // Placeholder (first child of cover_area)
                                    if let Some(pl) = ca.first_child().and_then(|c| c.downcast::<gtk4::DrawingArea>().ok()) {
                                        pl.set_visible(!has_cov);
                                        let (rp, gp, bp) = (*pr, *pg, *pb);
                                        pl.set_draw_func(move |_area, cr, _w, _h| {
                                            cr.set_source_rgb(rp, gp, bp);
                                            let _ = cr.paint();
                                        });
                                    }
                                    // Cover image (second child of cover_area)
                                    if let Some(pic) = ca.first_child()
                                        .and_then(|c| c.next_sibling())
                                        .and_then(|c| c.downcast::<Picture>().ok()) {
                                        if let Some(p) = bind_cp.borrow().get(name).and_then(|o| o.as_deref()) {
                                            pic.set_filename(Some(p));
                                            pic.set_visible(true);
                                        } else {
                                            pic.set_visible(false);
                                        }
                                        // Register for async cover updates
                                        bind_cw.borrow_mut().insert(name.clone(), pic.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            });

            // Factory unbind: no-op — cover_widgets is cleared on each batch_populate.
            album_factory.connect_unbind(|_, _| {});

            let selection = NoSelection::new(Some(album_model.clone()));
            let album_grid = GridView::new(Some(selection.clone()), Some(album_factory.clone()));
            album_grid.set_min_columns(1);

            // Double-click / Enter activates the item
            let activate_data = album_grid_data.clone();
            let activate_tx = cmd_tx.clone();
            album_grid.connect_activate(move |grid, position| {
                if let Some(model) = grid.model() {
                    if let Some(item) = model.item(position) {
                        if let Some(so) = item.downcast_ref::<StringObject>() {
                            if let Ok(idx) = so.string().parse::<usize>() {
                                let binding = activate_data.borrow();
                                if let Some(AlbumGridItem::Album { name, .. }) = binding.get(idx) {
                                    let _ = activate_tx.send(MpdCommand::PlayAlbum(name.clone()));
                                }
                            }
                        }
                    }
                }
            });

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

            left_stack.add_child(&loading_label);
            left_stack.add_child(&empty_label);
            left_stack.add_child(&album_grid);
            left_stack.set_visible_child(&loading_label);
            left_scroll.set_child(Some(&left_stack));
            // Album mode content box (group bar + search + grid)
            let album_content = Box::new(Orientation::Vertical, 0);
            album_content.set_vexpand(true);
            album_content.set_hexpand(true);
            album_content.set_widget_name("album-content");

            // Folder mode content box (folder tree)
            let folder_content = Box::new(Orientation::Vertical, 0);
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
            folder_search_results.set_visible(false);

            folder_content.append(&folder_search);
            folder_content.append(&folder_search_results);
            folder_content.append(&folder_browser.borrow().container.clone());

            // Show album mode by default
            album_content.set_visible(true);
            folder_content.set_visible(false);

            // Mode switching: toggle visibility, save/restore scroll positions, update AppState
            let ac = album_content.clone();
            let fc = folder_content.clone();
            let mode_state = state.clone();
            let ls_album = left_scroll.clone();
            let album_mode_act = gtk4::gio::SimpleAction::new("album-mode", None);
            album_mode_act.connect_activate(move |_, _| {
                fc.set_visible(false);
                // Restore album scroll position
                if let Ok(s) = mode_state.read() {
                    let pos = s.album_browsing.scroll_position.1;
                    let adj = ls_album.vadjustment();
                    adj.set_value(pos.clamp(0.0, adj.upper() - adj.page_size()));
                }
                ac.set_visible(true);
                if let Ok(mut s) = mode_state.write() { s.mode = crate::state::Mode::Album; }
            });
            app_clone.add_action(&album_mode_act);
            app_clone.set_accels_for_action("app.album-mode", &["<Ctrl>1"]);

            let ac2 = album_content.clone();
            let fc2 = folder_content.clone();
            let mode_state2 = state.clone();
            let cmd_tx2 = cmd_tx.clone();
            let ls_save = left_scroll.clone();
            let folder_mode_act = gtk4::gio::SimpleAction::new("folder-mode", None);
            folder_mode_act.connect_activate(move |_, _| {
                // Save album scroll position before hiding
                if let Ok(mut s) = mode_state2.write() {
                    s.album_browsing.scroll_position.1 = ls_save.vadjustment().value();
                }
                ac2.set_visible(false);
                fc2.set_visible(true);
                let _ = cmd_tx2.send(MpdCommand::ListDirectory(String::new()));
                if let Ok(mut s) = mode_state2.write() { s.mode = crate::state::Mode::Folder; }
            });
            app_clone.add_action(&folder_mode_act);
            app_clone.set_accels_for_action("app.folder-mode", &["<Ctrl>2"]);

            left_pane_box.append(&album_content);
            left_pane_box.append(&folder_content);

            // Album content: group bar + search + grid
            album_content.append(&group_bar);

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

            // Search generation counter & local index
            let search_gen: std::rc::Rc<std::cell::Cell<u64>> = std::rc::Rc::new(std::cell::Cell::new(0));
            let search_index: Arc<RwLock<SearchIndex>> = Arc::new(RwLock::new(SearchIndex::new()));

            // stop_search: Escape or clear button
            let restore_bar = group_bar.clone();
            let prev_group_stop = prev_group.clone();
            search_entry.connect_stop_search(move |_| {
                // Restore the previously active group, fall back to "Albums"
                let tag = prev_group_stop.borrow_mut().take().unwrap_or_else(|| "Albums".into());
                let restore_idx = match tag.as_str() {
                    "Artist" => 1, "Date" => 2, "Genre" => 3, _ => 0,
                };
                let mut idx = 0i32;
                let mut child = restore_bar.first_child();
                while let Some(w) = child {
                    if let Some(btn) = w.downcast_ref::<gtk4::ToggleButton>() {
                        if idx == restore_idx { btn.set_active(true); break; }
                        idx += 1;
                    }
                    child = w.next_sibling();
                }
            });

            // search_changed: debounced with local index check + MPD fallback.
            let se_tx = cmd_tx.clone();
            let se_gen = search_gen.clone();
            let se_group = prev_group.clone();
            let restore_bar2 = group_bar.clone();
            let se_index = search_index.clone();
            let se_grid = album_grid.clone();
            let se_stack = left_stack.clone();
            let se_model = album_model.clone();
            let se_data = album_grid_data.clone();
            let se_cw = cover_widgets.clone();
            search_entry.connect_search_changed(move |entry| {
                let q = entry.text().to_string();

                if q.is_empty() {
                    if let Some(ref btn) = restore_bar2.first_child()
                        .and_then(|c| c.downcast::<gtk4::ToggleButton>().ok())
                    {
                        btn.set_active(true);
                    }
                    return;
                }

                if se_group.borrow().is_none() {
                    let tags = ["Albums", "Artist", "Date", "Genre"];
                    let mut idx = 0i32;
                    let mut child = restore_bar2.first_child();
                    while let Some(w) = child {
                        if let Some(btn) = w.downcast_ref::<gtk4::ToggleButton>() {
                            if btn.is_active() && idx < tags.len() as i32 {
                                *se_group.borrow_mut() = Some(tags[idx as usize].to_string());
                                break;
                            }
                            idx += 1;
                        }
                        child = w.next_sibling();
                    }
                }

                let cur = se_gen.get();
                se_gen.set(cur.wrapping_add(1));
                let this_gen = cur.wrapping_add(1);

                let qc = q.clone();
                let gen_c = se_gen.clone();
                let idx = se_index.clone();
                let g = se_grid.clone();
                let s = se_stack.clone();
                let tx = se_tx.clone();
                let m = se_model.clone();
                let d = se_data.clone();
                let cw = se_cw.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(150),
                    move || {
                        if gen_c.get() != this_gen { return; }
                        // Try local index first
                        let Ok(index) = idx.read() else { return; };
                        let results = index.search(&qc);
                        if !results.is_empty() {
                            let items: Vec<AlbumGridItem> = results.iter()
                                .enumerate()
                                .map(|(i, (artist, name))| {
                                    let (pr, pg, pb) = placeholder_rgb(artist);
                                    AlbumGridItem::Album {
                                        artist: artist.clone(),
                                        name: name.clone(),
                                        album_id: format!("search-{i}"),
                                        pr, pg, pb,
                                    }
                                })
                                .collect();
                            batch_populate(&m, &d, items, &cw);
                            s.set_visible_child(&g);
                        } else {
                            // Fall back to MPD search
                            let _ = tx.send(MpdCommand::Search(qc));
                        }
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

            // Pinned group header (visible during scroll in grouped views)
            let pinned_header = Label::new(None);
            pinned_header.set_css_classes(&["album-group-header", "pinned-group-header"]);
            pinned_header.set_visible(false);

            // Wrap the scroll area in an overlay for the pinned group header
            let album_overlay = gtk4::Overlay::new();
            album_overlay.set_child(Some(&left_scroll));
            pinned_header.set_halign(gtk4::Align::Start);
            pinned_header.set_valign(gtk4::Align::Start);
            pinned_header.set_margin_start(8);
            pinned_header.set_margin_top(4);
            album_overlay.add_overlay(&pinned_header);

            album_content.append(&search_entry);
            album_content.append(&album_overlay);
            paned.set_start_child(Some(&left_pane_box));

            // --- Right rail ---
            let right_pane = Box::new(Orientation::Vertical, 0);

            let conn_indicator = Box::new(Orientation::Horizontal, 0);
            conn_indicator.set_size_request(-1, 4);
            conn_indicator.set_widget_name("connection-indicator");
            conn_indicator.set_css_classes(&["connection-indicator"]);
            right_pane.append(&conn_indicator);

            // Settings gear button (top-right) with Ctrl+, shortcut
            let settings_btn = gtk4::Button::with_label("⚙");
            settings_btn.set_halign(gtk4::Align::End);
            settings_btn.set_margin_end(4);
            settings_btn.set_margin_top(2);
            let sw = window.clone();
            let btn_clone = settings_btn.clone();
            let cp_save = conn_params.clone();
            let stx = cmd_tx.clone();
            let settings_paned = paned.clone();
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

                content.append(&Label::new(Some("Split Ratio:")));
                let split_adj = gtk4::Adjustment::new(scfg.split_ratio, 0.5, 0.9, 0.025, 0.1, 0.0);
                let split_scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&split_adj));
                split_scale.set_draw_value(true);
                split_scale.set_hexpand(true);
                content.append(&split_scale);

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
                let sa = split_adj.clone();
                let sp = settings_paned.clone();
                save_btn.connect_clicked(move |_| {
                    let mut c = Config::load();
                    c.mpd_host = he.text().to_string();
                    c.split_ratio = sa.value();
                    match pe.text().parse::<u16>() {
                        Ok(p) if p > 0 => {
                            c.mpd_port = p;
                            perr.set_visible(false);
                            let _ = c.save();
                            sp.set_position((sp.width() as f64 * c.split_ratio) as i32);
                            // Update shared host/port and trigger reconnect
                            if let Ok(mut params) = cp.lock() {
                                *params = (c.mpd_host.clone(), c.mpd_port);
                            }
                            let _ = tx.send(MpdCommand::Reconnect);
                            dw.close();
                        }
                        _ => {
                            perr.set_text("Invalid port (1-65535)");
                            perr.set_visible(true);
                        }
                    }
                });
                let dw2 = d.clone();
                cancel_btn.connect_clicked(move |_| { dw2.close(); });
                btn_box.append(&save_btn);
                btn_box.append(&cancel_btn);
                content.append(&btn_box);
                d.set_child(Some(&content));
                d.present();
            });
            right_pane.append(&settings_btn);

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

            let np_cover = Picture::new();
            np_cover.set_size_request(120, 120);
            np_cover.set_halign(gtk4::Align::Center);
            np_cover.set_visible(false);
            now_playing.append(&np_cover);

            let track_title = Label::new(Some("No track"));
            track_title.set_halign(gtk4::Align::Start);
            track_title.set_ellipsize(gtk4::pango::EllipsizeMode::End);

            let track_artist = Label::new(Some(""));
            track_artist.set_halign(gtk4::Align::Start);
            track_artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);

            let track_album = Label::new(Some(""));
            track_album.set_halign(gtk4::Align::Start);
            track_album.set_ellipsize(gtk4::pango::EllipsizeMode::End);

            let playback_icon = Label::new(Some("⏹"));
            playback_icon.set_halign(gtk4::Align::Start);

            let time_display = Label::new(Some("--:-- / --:--"));
            time_display.set_halign(gtk4::Align::Start);
            time_display.set_css_classes(&["time-display"]);

            let seek_adjustment = gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 5.0, 0.0);
            let seekbar = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&seek_adjustment));
            seekbar.set_hexpand(true);
            seekbar.set_draw_value(false);
            seekbar.set_css_classes(&["seekbar"]);

            let format_badge = Label::new(None);
            format_badge.set_halign(gtk4::Align::Start);
            format_badge.set_css_classes(&["format-badge"]);
            format_badge.set_visible(false);

            now_playing.append(&playback_icon);
            now_playing.append(&track_title);
            now_playing.append(&track_artist);
            now_playing.append(&track_album);
            now_playing.append(&time_display);
            now_playing.append(&seekbar);
            now_playing.append(&format_badge);
            right_pane.append(&now_playing);

            // Current album track window
            let track_win_label = Label::new(Some("Album Tracks"));
            track_win_label.set_halign(gtk4::Align::Start);
            track_win_label.set_margin_start(12);
            track_win_label.set_margin_top(8);
            track_win_label.set_css_classes(&["queue-header"]);
            right_pane.append(&track_win_label);

            let track_win_list = ListBox::new();
            track_win_list.set_selection_mode(gtk4::SelectionMode::Single);
            let track_win_scroll = ScrolledWindow::new();
            track_win_scroll.set_child(Some(&track_win_list));
            track_win_scroll.set_max_content_height(180);
            track_win_scroll.set_propagate_natural_height(true);
            right_pane.append(&track_win_scroll);

            // Queue display in right rail
            let queue_label = Label::new(Some("Queue"));
            queue_label.set_halign(gtk4::Align::Start);
            queue_label.set_margin_start(12);
            queue_label.set_margin_top(8);
            queue_label.set_css_classes(&["queue-header"]);
            right_pane.append(&queue_label);

            let queue_list = ListBox::new();
            queue_list.set_selection_mode(gtk4::SelectionMode::Single);
            let queue_scroll = ScrolledWindow::new();
            queue_scroll.set_child(Some(&queue_list));
            queue_scroll.set_vexpand(true);
            right_pane.append(&queue_scroll);

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

            paned.set_end_child(Some(&right_pane));

            // Toast overlay for notifications
            let toast = std::rc::Rc::new(std::cell::RefCell::new(widgets::toast::ToastOverlay::new()));
            let main_overlay = gtk4::Overlay::new();
            main_overlay.set_child(Some(&paned));
            main_overlay.add_overlay(toast.borrow().widget());
            window.set_child(Some(&main_overlay));

            let default_w = window.default_width().max(800) as f64;
            paned.set_position((default_w * cfg.split_ratio) as i32);

            // Grid populates on startup via set_active(true) on the "Albums" button above
            let _ = cmd_tx.send(MpdCommand::ListQueue);

            // 30s queue polling timer
            let poll_tx = cmd_tx.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(30), move || {
                let _ = poll_tx.send(MpdCommand::ListQueue);
                glib::ControlFlow::Continue
            });

            window.present();

            // --- Scroll-aware cover loading: debounce timer + vadjustment handler ---
            let scroll_timer: std::rc::Rc<std::cell::Cell<Option<glib::SourceId>>> =
                std::rc::Rc::new(std::cell::Cell::new(None));

            let sv_adj = left_scroll.vadjustment();
            let sv_backing = album_grid_data.clone();
            let sv_grid = album_grid.clone();
            let sv_cmd = cmd_tx.clone();
            let sv_timer = scroll_timer.clone();
            sv_adj.connect_value_changed(move |adj| {
                // Cancel previous debounce timer
                if let Some(id) = sv_timer.take() {
                    id.remove();
                }
                // Start a new 300ms debounce timer; resets on each scroll event
                let backing = sv_backing.clone();
                let grid = sv_grid.clone();
                let cmd = sv_cmd.clone();
                let adj_clone = adj.clone();
                let id = glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
                    let albums = calculate_visible_albums(&backing, &adj_clone, &grid);
                    if !albums.is_empty() {
                        log::debug!(
                            "[ui] scroll stop: enqueuing {} visible albums for cover fetch",
                            albums.len()
                        );
                        let _ = cmd.send(MpdCommand::FetchCovers(albums));
                    }
                    glib::ControlFlow::Break
                });
                sv_timer.set(Some(id));
            });

            // --- Frame clock tick callback for MPD events (replaces 30ms timer) ---
            // add_tick_callback fires once per display refresh (vsync-aligned).
            // Replaces the fixed 30ms timer that could fire mid-frame or during
            // layout passes, which starved the GTK main loop under heavy load.
            // --- Pinned group header: track which group is at the top of the viewport ---
            let ph_backing = album_grid_data.clone();
            let ph_grid = album_grid.clone();
            let ph_label = pinned_header.clone();
            let ph_adj = left_scroll.vadjustment();
            ph_adj.connect_value_changed(move |adj| {
                let binding = ph_backing.borrow();
                // Only show pinned header when there are Header items (grouped view)
                let has_headers = binding.iter().any(|item| matches!(item, AlbumGridItem::Header { .. }));
                if !has_headers {
                    ph_label.set_visible(false);
                    return;
                }
                // Estimate first visible item from scroll position
                let first_visible = (adj.value() / (adj.page_size().max(1.0) / binding.len().max(1) as f64)).max(0.0) as usize;
                let idx = first_visible.min(binding.len().saturating_sub(1));
                // Walk backward to find the nearest preceding Header
                let header = (0..=idx).rev().find_map(|i| {
                    if let AlbumGridItem::Header { name, .. } = &binding[i] {
                        Some(name.clone())
                    } else {
                        None
                    }
                });
                if let Some(name) = header {
                    ph_label.set_text(&name);
                    ph_label.set_visible(true);
                } else {
                    ph_label.set_visible(false);
                }
            });

            let fc_rx = event_rx.clone();
            let fc_ci = conn_indicator.clone();
            let fc_tl = track_title.clone();
            let fc_ar = track_artist.clone();
            let fc_al = track_album.clone();
            let fc_seekbar_cmd = cmd_tx.clone();
            seekbar.connect_change_value(move |_, _, value| {
                let pos = value as i64;
                let _ = fc_seekbar_cmd.send(MpdCommand::Seek(pos));
                glib::Propagation::Stop
            });

            let fc_pi = playback_icon.clone();
            let fc_td = time_display.clone();
            let fc_seekbar = seekbar.clone();
            let fc_fmt = format_badge.clone();
            let fc_np_cover = np_cover.clone();
            let fc_track_win = track_win_list.clone();
            let fc_track_cmd = cmd_tx.clone();
            let fc_grid = album_grid.clone();
            let fc_stack = left_stack.clone();
            let fc_empty = empty_label.clone();
            let fc_cmd = cmd_tx.clone();
            let fc_fb = folder_browser.clone();
            let fc_fs_list = folder_search_list.clone();
            let fc_fs_container = folder_search_results.clone();
            let fc_ql = queue_list.clone();
            let fc_ids = item_ids_w.clone();
            let fc_si = search_index.clone();
            let fc_toast = toast.clone();
            let fc_ev_cover_paths = cover_paths.clone();
            let fc_ev_cover_widgets = cover_widgets.clone();
            let fc_cp_np = fc_ev_cover_paths.clone();
            let fc_ev_model = album_model.clone();
            let fc_ev_data = album_grid_data.clone();
            let fc_vadj = left_scroll.vadjustment();
            let fc_vadj_grid = album_grid.clone();
            let fc_current_song_pos: std::cell::Cell<Option<i32>> = std::cell::Cell::new(None);
            let fc_current_album: std::rc::Rc<std::cell::RefCell<Option<String>>> = std::rc::Rc::new(std::cell::RefCell::new(None));
            let fc_shutdown = shutdown_app.clone();

            window.add_tick_callback(move |_widget, _fc| {
                // Check for shutdown request from SIGINT/SIGTERM signal handlers.
                if crate::SHUTDOWN_REQUESTED.swap(false, Ordering::AcqRel) {
                    fc_shutdown.quit();
                    return glib::ControlFlow::Break;
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
                    match event {
                        MpdEvent::Connected => {
                            fc_ci.set_css_classes(&["connection-indicator", "connected"]);
                            if let Ok(mut idx) = fc_si.write() { *idx = SearchIndex::new(); }
                            let _ = fc_cmd.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                            let _ = fc_cmd.send(MpdCommand::ListQueue);
                        }
                        MpdEvent::Connecting => {
                            fc_ci.set_css_classes(&["connection-indicator", "connecting"]);
                        }
                        MpdEvent::Disconnected => {
                            fc_ci.set_css_classes(&["connection-indicator", "disconnected"]);
                        }
                        MpdEvent::StateChanged(update) => {
                            fc_current_song_pos.set(update.song.map(|s| s as i32));
                            let album_changed = fc_current_album.borrow().as_deref() != update.album.as_deref();
                            *fc_current_album.borrow_mut() = update.album.clone();
                            if album_changed {
                                if let Some(ref album) = update.album {
                                    let _ = fc_track_cmd.send(MpdCommand::ListAlbumTracks(album.clone()));
                                } else {
                                    // Clear the track window
                                    while let Some(child) = fc_track_win.first_child() {
                                        fc_track_win.remove(&child);
                                    }
                                }
                            }
                            update_now_playing(NowPlayingWidgets {
    title: &fc_tl,
    artist: &fc_ar,
    album: &fc_al,
    icon: &fc_pi,
    time_display: &fc_td,
    seekbar: &fc_seekbar,
    format_badge: &fc_fmt,
    cover: &fc_np_cover,
    cover_paths: &fc_cp_np,
}, &update);
                        }
                        MpdEvent::Albums(albums) => {
                            // Build local search index
                            if let Ok(mut idx) = fc_si.write() { idx.build(&albums); }
                            if albums.is_empty() {
                                fc_empty.set_text("No albums found");
                                fc_stack.set_visible_child(&fc_empty);
                            } else {
                                let items: Vec<AlbumGridItem> = albums.iter()
                                    .enumerate()
                                    .map(|(i, (artist, name))| {
                                        let (pr, pg, pb) = placeholder_rgb(artist);
                                        AlbumGridItem::Album {
                                            artist: artist.clone(),
                                            name: name.clone(),
                                            album_id: format!("album-{i}"),
                                            pr, pg, pb,
                                        }
                                    })
                                    .collect();
                                batch_populate(&fc_ev_model, &fc_ev_data, items, &fc_ev_cover_widgets);
                                fc_stack.set_visible_child(&fc_grid);
                                // Scroll-aware: only fetch covers for visible (±1 row) albums
                                let backing = fc_ev_data.clone();
                                let vadj = fc_vadj.clone();
                                let grid = fc_vadj_grid.clone();
                                let cmd = fc_cmd.clone();
                                glib::idle_add_local(move || {
                                    let albums = calculate_visible_albums(&backing, &vadj, &grid);
                                    if !albums.is_empty() {
                                        log::debug!(
                                            "[ui] initial: enqueuing {} visible albums for cover fetch",
                                            albums.len()
                                        );
                                        let _ = cmd.send(MpdCommand::FetchCovers(albums));
                                    }
                                    glib::ControlFlow::Break
                                });
                            }
                        }
                        MpdEvent::AlbumsGrouped(groups) => {
                            let flat: Vec<(String, String)> = groups.iter()
                                .flat_map(|(_, a)| a.clone()).collect();
                            if groups.is_empty() {
                                fc_empty.set_text("No albums found");
                                fc_stack.set_visible_child(&fc_empty);
                            } else {
                                // Only rebuild search index if flat list changed
                                let need_index = if let Ok(idx) = fc_si.read() {
                                    idx.album_count() != flat.len()
                                } else { true };
                                if need_index {
                                    if let Ok(mut idx) = fc_si.write() { idx.build(&flat); }
                                }
                                let items: Vec<AlbumGridItem> = groups.iter()
                                    .flat_map(|(header, albums)| {
                                        let mut group_items: Vec<AlbumGridItem> = Vec::new();
                                        group_items.push(AlbumGridItem::Header {
                                            name: header.clone(),
                                            count: albums.len() as u32,
                                        });
                                        for (artist, name) in albums {
                                            let (pr, pg, pb) = placeholder_rgb(artist);
                                            group_items.push(AlbumGridItem::Album {
                                                artist: artist.clone(),
                                                name: name.clone(),
                                                album_id: format!("{}-{}", header, name),
                                                pr, pg, pb,
                                            });
                                        }
                                        group_items
                                    })
                                    .collect();
                                batch_populate(&fc_ev_model, &fc_ev_data, items, &fc_ev_cover_widgets);
                                fc_stack.set_visible_child(&fc_grid);
                                if need_index {
                                    // Scroll-aware: only fetch covers for visible (±1 row) albums
                                    let backing = fc_ev_data.clone();
                                    let vadj = fc_vadj.clone();
                                    let grid = fc_vadj_grid.clone();
                                    let cmd = fc_cmd.clone();
                                    glib::idle_add_local(move || {
                                        let albums = calculate_visible_albums(&backing, &vadj, &grid);
                                        if !albums.is_empty() {
                                            log::debug!(
                                                "[ui] grouped: enqueuing {} visible albums for cover fetch",
                                                albums.len()
                                            );
                                            let _ = cmd.send(MpdCommand::FetchCovers(albums));
                                        }
                                        glib::ControlFlow::Break
                                    });
                                }
                            }
                        }
                        MpdEvent::SearchResults(results) => {
                            if results.is_empty() {
                                fc_empty.set_text("No results found");
                                fc_stack.set_visible_child(&fc_empty);
                            } else {
                                let items: Vec<AlbumGridItem> = results.iter()
                                    .enumerate()
                                    .map(|(i, (artist, name))| {
                                        let (pr, pg, pb) = placeholder_rgb(artist);
                                        AlbumGridItem::Album {
                                            artist: artist.clone(),
                                            name: name.clone(),
                                            album_id: format!("search-{i}"),
                                            pr, pg, pb,
                                        }
                                    })
                                    .collect();
                                batch_populate(&fc_ev_model, &fc_ev_data, items, &fc_ev_cover_widgets);
                                fc_stack.set_visible_child(&fc_grid);
                            }
                        }
                        MpdEvent::FileSearchResults(results) => {
                            // Populate folder search results
                            while let Some(child) = fc_fs_list.first_child() {
                                fc_fs_list.remove(&child);
                            }
                            for (path, name) in &results {
                                let row = gtk4::ListBoxRow::new();
                                let lbl = gtk4::Label::new(Some(&format!("{name}\n{path}")));
                                lbl.set_halign(gtk4::Align::Start);
                                lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                                row.set_child(Some(&lbl));
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
                            while let Some(child) = fc_track_win.first_child() {
                                fc_track_win.remove(&child);
                            }
                            for (title, _file, duration) in tracks {
                                let row = gtk4::ListBoxRow::new();
                                let hbox = Box::new(Orientation::Horizontal, 6);
                                let title_lbl = Label::new(Some(&title));
                                title_lbl.set_halign(gtk4::Align::Start);
                                title_lbl.set_hexpand(true);
                                title_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                                hbox.append(&title_lbl);
                                if duration > 0.0 {
                                    let dur_lbl = Label::new(Some(&format!("{}:{:02}", duration as u64 / 60, duration as u64 % 60)));
                                    dur_lbl.set_css_classes(&["time-label"]);
                                    hbox.append(&dur_lbl);
                                }
                                row.set_child(Some(&hbox));
                                fc_track_win.append(&row);
                            }
                        }
                        MpdEvent::Queue(queue) => {
                            while let Some(child) = fc_ql.first_child() {
                                fc_ql.remove(&child);
                            }
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
                                // Right-click context menu popover
                                let rclick = gtk4::GestureClick::new();
                                rclick.set_button(3);
                                let tx_pop = q_tx.clone();
                                let ipos = item.position;
                                let iid = item.id;
                                let current_song = fc_current_song_pos.get();
                                rclick.connect_pressed(move |gest, _n, _x, _y| {
                                    let pop = gtk4::Popover::new();
                                    let popbox = Box::new(Orientation::Vertical, 0);
                                    let btn_play = gtk4::Button::with_label("Play Now");
                                    let btn_next = gtk4::Button::with_label("Play Next");
                                    let btn_rem = gtk4::Button::with_label("Remove");
                                    let tp = tx_pop.clone();
                                    let ip = ipos;
                                    btn_play.connect_clicked(move |_| {
                                        let _ = tp.send(MpdCommand::PlayPosition(ip));
                                    });
                                    let tn = tx_pop.clone();
                                    let id_next = iid;
                                    let target = current_song.map(|p| p + 1).unwrap_or(0);
                                    btn_next.connect_clicked(move |_| {
                                        let _ = tn.send(MpdCommand::MoveId(id_next, target));
                                    });
                                    let tr = tx_pop.clone();
                                    btn_rem.connect_clicked(move |_| {
                                        let _ = tr.send(MpdCommand::DeleteId(iid));
                                    });
                                    popbox.append(&btn_play);
                                    popbox.append(&btn_next);
                                    popbox.append(&btn_rem);
                                    pop.set_child(Some(&popbox));
                                    if let Some(ref w) = gest.widget() { pop.set_parent(w); }
                                    pop.popup();
                                });
                                fc_ql.append(&row);
                            }
                            // Select the current track's row in the list
                            if let Some(ref cr) = current_row {
                                fc_ql.select_row(Some(cr));
                            }
                            // Refresh shared item_ids for key-based delete/move lookup
                            *fc_ids.borrow_mut() = item_ids;
                        }
                        MpdEvent::CoverPaths(paths) => {
                            let mut cp = fc_ev_cover_paths.borrow_mut();
                            let widgets = fc_ev_cover_widgets.borrow();
                            for (album, path) in &paths {
                                log::info!("[UI] cover path: '{album}' -> {:?}", path);
                                cp.insert(album.clone(), path.clone());
                                // Update the Picture widget in-place if registered
                                if let Some(p) = path.as_deref() {
                                    if let Some(pic) = widgets.get(album) {
                                        log::info!("[UI] cover update: '{album}' -> {p}");
                                        pic.set_filename(Some(p));
                                        pic.set_visible(true);
                                        pic.queue_draw();
                                    } else {
                                        log::warn!("[UI] cover: no widget registered for '{album}'");
                                    }
                                }
                            }
                        }
                        MpdEvent::CoverRefreshed { album_id, data } => {
                            log::info!("[UI] cover refreshed: '{album_id}' ({} bytes)", data.len());
                            // Decode raw JPEG bytes into a GdkTexture via gdk-pixbuf.
                            // Own the data to satisfy gdk-pixbuf's 'static + Send bound.
                            let owned = data.to_vec();
                            let cursor = std::io::Cursor::new(owned);
                            if let Ok(pixbuf) = gdk_pixbuf::Pixbuf::from_read(cursor) {
                                let texture = gdk4::Texture::for_pixbuf(&pixbuf);
                                // Update album grid widget in-place (widget registry lookup)
                                if let Some(pic) = fc_ev_cover_widgets.borrow().get(&album_id) {
                                    pic.set_paintable(Some(&texture));
                                    pic.set_visible(true);
                                    pic.queue_draw();
                                } else {
                                    log::warn!("[UI] cover refresh: no grid widget registered for '{album_id}'");
                                }
                                // Also update now-playing cover if this is the current album
                                if fc_current_album.borrow().as_deref() == Some(&album_id) {
                                    fc_np_cover.set_paintable(Some(&texture));
                                    fc_np_cover.set_visible(true);
                                }
                            } else {
                                log::warn!("[UI] cover refresh failed: couldn't decode image for '{album_id}'");
                            }
                        }
                        MpdEvent::LibraryChanged => {
                            if let Ok(mut idx) = fc_si.write() { *idx = SearchIndex::new(); }
                            let _ = fc_cmd.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                        }
                        MpdEvent::Error(msg) => {
                            fc_ci.set_css_classes(&["connection-indicator", "error"]);
                            fc_toast.borrow().show_toast(&format!("MPD Error: {msg}"));
                        }
                    }
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

            // Load CSS
            let css = gtk4::CssProvider::new();
            css.load_from_string(
                "#connection-indicator { background-color: gray; }
                 #connection-indicator.connected { background-color: #4CAF50; }
                 #connection-indicator.disconnected { background-color: #f44336; }
                 #connection-indicator.error { background-color: #f44336; }
                 #connection-indicator.connecting { background-color: #FFC107; }
                 .album-cover-cell:selected { border: 2px solid @theme_selected_bg_color; }
                 .album-cover-hover-btn { opacity: 0; transition: opacity 150ms ease-in-out; min-width: 24px; min-height: 24px; padding: 2px; }
                 .album-cover-cell:hover .album-cover-hover-btn { opacity: 1; }
                 .album-group-header { font-weight: bold; font-size: 1.1em; padding: 4px 8px; }
                 .pinned-group-header { background-color: @theme_base_color; border-bottom: 1px solid @borders; }
                 .group-select-btn:checked { background-color: @theme_selected_bg_color; color: @theme_selected_fg_color; }
                 #album-grid-status { color: gray; font-style: italic; padding: 24px; }
                 .breadcrumb-current { font-weight: bold; }
                 .breadcrumb-segment { color: @theme_link_color; }
                 .breadcrumb-segment:hover { text-decoration: underline; }
                 .breadcrumb-root { color: @theme_link_color; }
                 .breadcrumb-root:hover { text-decoration: underline; }
                 .breadcrumb-sep { color: gray; }
                 .queue-current { background-color: @theme_selected_bg_color; }
                 .queue-artist { font-size: 0.85em; color: gray; }
                 .shortcut-key { font-weight: bold; }
                 .error-label { color: #f44336; font-size: 0.85em; }
                 .format-badge { font-size: 0.85em; color: gray; padding: 2px 0; }
                 .seekbar { margin: 4px 0; min-height: 12px; }"
            );
            gtk4::style_context_add_provider_for_display(
                &gtk4::prelude::WidgetExt::display(&window),
                &css,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        });

        application.run();
    }
}

struct NowPlayingWidgets<'a> {
    title: &'a Label,
    artist: &'a Label,
    album: &'a Label,
    icon: &'a Label,
    time_display: &'a Label,
    seekbar: &'a gtk4::Scale,
    format_badge: &'a Label,
    cover: &'a Picture,
    cover_paths: &'a std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Option<String>>>>,
}

fn update_now_playing(
    w: NowPlayingWidgets,
    update: &PlaybackUpdate,
) {
    if let Some(ref t) = update.title {
        w.title.set_text(t);
    } else {
        w.title.set_text("");
    }
    if let Some(ref a) = update.artist {
        w.artist.set_text(a);
    } else {
        w.artist.set_text("");
    }
    if let Some(ref a) = update.album {
        w.album.set_text(a);
        let cp = w.cover_paths.borrow();
        if let Some(Some(path)) = cp.get(a) {
            w.cover.set_filename(Some(path));
            w.cover.set_visible(true);
        } else {
            w.cover.set_visible(false);
        }
    } else {
        w.album.set_text("");
        w.cover.set_visible(false);
    }
    match (update.elapsed, update.duration) {
        (Some(el), Some(dur)) => {
            w.time_display.set_text(&format!("{}:{:02} / {}:{:02}", el as u64 / 60, el as u64 % 60, dur as u64 / 60, dur as u64 % 60));
            let adj = w.seekbar.adjustment();
            adj.set_upper(dur);
            adj.set_value(el);
        }
        _ => {
            w.time_display.set_text("--:-- / --:--");
            w.seekbar.adjustment().set_upper(0.0);
            w.seekbar.adjustment().set_value(0.0);
        }
    }
    if let Some(ref fmt) = update.format {
        w.format_badge.set_text(fmt);
        w.format_badge.set_visible(true);
    } else {
        w.format_badge.set_visible(false);
    }
    match update.state.as_str() {
        "play" => w.icon.set_text("▶"),
        "pause" => w.icon.set_text("⏸"),
        "stop" | "" => w.icon.set_text("⏹"),
        _ => {
            log::warn!("Unknown playback state: {}", update.state);
            w.icon.set_text("⏹");
        }
    }
}

