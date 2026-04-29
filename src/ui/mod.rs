//! UI layer — GTK4 widgets and window management. Thread: UI (GTK main loop).

pub mod widgets;

use crate::config::Config;
use crate::mpd::state_machine::{MpdCommand, MpdEvent, PlaybackUpdate};
use crate::search::SearchIndex;
use crate::state::SharedState;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box, EventControllerKey, FlowBox, Label, ListBox, Orientation, Paned, Picture, ScrolledWindow};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use widgets::album_cover;

type SharedIds = std::rc::Rc<std::cell::RefCell<Arc<Mutex<HashMap<i32, i32>>>>>;

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
            let album_grid = FlowBox::new();
            let left_stack = gtk4::Stack::new();
            album_grid.set_min_children_per_line(1);
            album_grid.set_homogeneous(true);
            album_grid.set_selection_mode(gtk4::SelectionMode::Single);
            album_grid.set_activate_on_single_click(false);

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

            // Compute initial columns from default window width
            let init_width = (1200.0 * crate::constants::SHELL_SPLIT_RATIO) as i32;
            let n = (init_width as f64 / 210.0).ceil().max(1.0) as u32;
            album_grid.set_max_children_per_line(n);

            // Shared album name store for double-click handler
            let album_names = Arc::new(Mutex::new(Vec::<String>::new()));
            let dbl_tx = cmd_tx.clone();
            let an_dbl = album_names.clone();
            album_grid.connect_child_activated(move |_grid, child| {
                let idx = child.index() as usize;
                if let Ok(store) = an_dbl.lock() {
                    if let Some(name) = store.get(idx) {
                        let _ = dbl_tx.send(MpdCommand::PlayAlbum(name.clone()));
                    }
                }
            });

            // Wire selected_album_id to AppState
            let state_sel = state.clone();
            album_grid.connect_selected_children_changed(move |grid| {
                let id = grid.selected_children()
                    .first()
                    .map(|w| w.widget_name().to_string());
                if let Ok(mut s) = state_sel.write() {
                    s.album_browsing.selected_album_id = id;
                }
            });

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

            // Search bar with 150ms debounce
            let search_entry = gtk4::SearchEntry::new();
            search_entry.set_placeholder_text(Some("Search albums..."));
            search_entry.set_margin_start(4);
            search_entry.set_margin_end(4);
            search_entry.set_margin_bottom(4);
            search_entry.set_size_request(-1, 32);

            // Ctrl+F focuses the search entry
            let search_action = gtk4::gio::SimpleAction::new("search", None);
            let se_focus = search_entry.clone();
            search_action.connect_activate(move |_, _| { se_focus.grab_focus(); });
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
            let se_album_names = album_names.clone();
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
                let an = se_album_names.clone();
                let tx = se_tx.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(150),
                    move || {
                        if gen_c.get() != this_gen { return; }
                        // Try local index first
                        let Ok(index) = idx.read() else { return; };
                        let results = index.search(&qc);
                        if !results.is_empty() {
                            let names: Vec<String> = results.iter().map(|r: &(String, String)| r.1.clone()).collect();
                            if let Ok(mut store) = an.lock() { *store = names; }
                            let empty_covers = std::collections::HashMap::new();
                            let empty_widgets = CoverWidgets::new(std::collections::HashMap::new());
                            populate_album_grid(&g, &results, &tx, &empty_covers, &empty_widgets);
                            s.set_visible_child(&g);
                        } else {
                            // Fall back to MPD search
                            let _ = tx.send(MpdCommand::Search(qc));
                        }
                    },
                );
            });

            album_content.append(&search_entry);
            album_content.append(&left_scroll);
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
            let cfg = Config::load();
            let sw = window.clone();
            let btn_clone = settings_btn.clone();
            let cp_save = conn_params.clone();
            let stx = cmd_tx.clone();
            settings_btn.connect_clicked(move |_| {
                let d = gtk4::Window::new();
                d.set_title(Some("Settings"));
                d.set_transient_for(Some(&sw));
                d.set_modal(true);
                let content = gtk4::Box::new(Orientation::Vertical, 8);
                content.set_margin_start(12); content.set_margin_end(12);
                content.set_margin_top(8); content.set_margin_bottom(8);
                let host_entry = gtk4::Entry::new();
                host_entry.set_text(&cfg.mpd_host);
                let port_entry = gtk4::Entry::new();
                port_entry.set_text(&cfg.mpd_port.to_string());
                content.append(&Label::new(Some("MPD Host:")));
                content.append(&host_entry);
                content.append(&Label::new(Some("MPD Port:")));
                content.append(&port_entry);
                let port_error = gtk4::Label::new(Some("Invalid port number"));
                port_error.set_css_classes(&["error-label"]);
                port_error.set_visible(false);
                content.append(&port_error);
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
                save_btn.connect_clicked(move |_| {
                    let mut c = Config::load();
                    c.mpd_host = he.text().to_string();
                    match pe.text().parse::<u16>() {
                        Ok(p) if p > 0 => {
                            c.mpd_port = p;
                            perr.set_visible(false);
                            let _ = c.save();
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

            let format_badge = Label::new(None);
            format_badge.set_halign(gtk4::Align::Start);
            format_badge.set_css_classes(&["format-badge"]);
            format_badge.set_visible(false);

            now_playing.append(&playback_icon);
            now_playing.append(&track_title);
            now_playing.append(&track_artist);
            now_playing.append(&track_album);
            now_playing.append(&time_display);
            now_playing.append(&format_badge);
            right_pane.append(&now_playing);

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
            let item_ids_w: SharedIds = std::rc::Rc::new(std::cell::RefCell::new(Arc::new(Mutex::new(HashMap::new()))));
            let ql_del = queue_list.clone();
            let cmd_del = cmd_tx.clone();
            let ids_del = item_ids_w.clone();
            let kc = EventControllerKey::new();
            kc.connect_key_pressed(move |_ctrl, key, _code, _mods| {
                let ids = ids_del.borrow().clone();
                if let Ok(m) = ids.lock() {
                    match key {
                        gtk4::gdk::Key::Delete | gtk4::gdk::Key::KP_Delete => {
                            let idx = ql_del.selected_row().map(|r| r.index()).unwrap_or(-1);
                            if let Some(id) = m.get(&idx) {
                                let _ = cmd_del.send(MpdCommand::DeleteId(*id));
                            }
                        }
                        gtk4::gdk::Key::Up if _mods.contains(gtk4::gdk::ModifierType::SHIFT_MASK) => {
                            let idx = ql_del.selected_row().map(|r| r.index()).unwrap_or(-1);
                            if idx > 0 {
                                if let Some(id) = m.get(&idx) {
                                    let _ = cmd_del.send(MpdCommand::MoveId(*id, idx - 1));
                                }
                            }
                        }
                        gtk4::gdk::Key::Down if _mods.contains(gtk4::gdk::ModifierType::SHIFT_MASK) => {
                            let idx = ql_del.selected_row().map(|r| r.index()).unwrap_or(-1);
                            if let Some(id) = m.get(&idx) {
                                let _ = cmd_del.send(MpdCommand::MoveId(*id, idx + 1));
                            }
                        }
                        _ => {}
                    }
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
            paned.set_position((default_w * crate::constants::SHELL_SPLIT_RATIO) as i32);

            // Grid populates on startup via set_active(true) on the "Albums" button above
            let _ = cmd_tx.send(MpdCommand::ListQueue);

            // 30s queue polling timer
            let poll_tx = cmd_tx.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(30), move || {
                let _ = poll_tx.send(MpdCommand::ListQueue);
                glib::ControlFlow::Continue
            });

            // --- Idle callback for MPD events ---
            let rx_c = event_rx.clone();
            let ci_c = conn_indicator.clone();
            let tl_c = track_title.clone();
            let ar_c = track_artist.clone();
            let al_c = track_album.clone();
            let pi_c = playback_icon.clone();
            let td_c = time_display.clone();
            let fmt_c = format_badge.clone();
            let np_cover_c = np_cover.clone();
            let grid_c = album_grid.clone();
            let stack_c = left_stack.clone();
            let empty_c = empty_label.clone();
            let cmd_c = cmd_tx.clone();
            let fb_c = folder_browser.clone();
            let ql_c = queue_list.clone();
            let ids_w = item_ids_w.clone();
            let si_c = search_index.clone();
            let toast_q = toast.clone();
            let cover_paths: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Option<String>>>> = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
            let cover_widgets: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, gtk4::Picture>>> = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
            let cp_np = cover_paths.clone();
            let current_song_pos: std::cell::Cell<Option<i32>> = std::cell::Cell::new(None);

            glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
                let mut guard = match rx_c.lock() {
                    Ok(g) => g,
                    Err(poisoned) => {
                        log::error!("MPD event receiver mutex poisoned: {poisoned}");
                        return glib::ControlFlow::Break;
                    }
                };
                // Process at most 64 events per tick to yield to GTK main loop
                let mut batch = 0u32;
                while batch < 64 {
                    let event = match guard.try_recv() {
                        Ok(e) => e,
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
                    };
                    batch += 1;
                    drop(guard);
                    match event {
                        MpdEvent::Connected => {
                            ci_c.set_css_classes(&["connection-indicator", "connected"]);
                            // Reset local search index — MPD library may have changed while disconnected
                            if let Ok(mut idx) = si_c.write() { *idx = SearchIndex::new(); }
                            let _ = cmd_c.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                            let _ = cmd_c.send(MpdCommand::ListQueue);
                        }
                        MpdEvent::Connecting => {
                            ci_c.set_css_classes(&["connection-indicator", "connecting"]);
                        }
                        MpdEvent::Disconnected => {
                            ci_c.set_css_classes(&["connection-indicator", "disconnected"]);
                        }
                        MpdEvent::StateChanged(update) => {
                            current_song_pos.set(update.song.map(|s| s as i32));
                            update_now_playing(NowPlayingWidgets {
    title: &tl_c,
    artist: &ar_c,
    album: &al_c,
    icon: &pi_c,
    time_display: &td_c,
    format_badge: &fmt_c,
    cover: &np_cover_c,
    cover_paths: &cp_np,
}, &update);
                        }
                        MpdEvent::Albums(albums) => {
                            // Build local search index
                            if let Ok(mut idx) = search_index.write() { idx.build(&albums); }
                            if albums.is_empty() {
                                empty_c.set_text("No albums found");
                                stack_c.set_visible_child(&empty_c);
                            } else {
                                let names: Vec<String> = albums.iter().map(|(_, n)| n.clone()).collect();
                                if let Ok(mut store) = album_names.lock() {
                                    *store = names;
                                }
                                let cp = cover_paths.borrow().clone();
                                populate_album_grid(&grid_c, &albums, &cmd_c, &cp, &cover_widgets);
                                stack_c.set_visible_child(&grid_c);
                                let covers_for_fetch = albums.clone();
                                let _ = cmd_c.send(MpdCommand::FetchCovers(covers_for_fetch));
                            }
                        }
                        MpdEvent::AlbumsGrouped(groups) => {
                            let flat: Vec<(String, String)> = groups.iter()
                                .flat_map(|(_, a)| a.clone()).collect();
                            if groups.is_empty() {
                                empty_c.set_text("No albums found");
                                stack_c.set_visible_child(&empty_c);
                            } else {
                                let names: Vec<String> = groups.iter()
                                    .flat_map(|(_, albums)| albums.iter().map(|(_, n)| n.clone()))
                                    .collect();
                                if let Ok(mut store) = album_names.lock() {
                                    *store = names;
                                }
                                // Only rebuild search index if flat list changed
                                let need_index = if let Ok(idx) = search_index.read() {
                                    idx.album_count() != flat.len()
                                } else { true };
                                if need_index {
                                    if let Ok(mut idx) = search_index.write() { idx.build(&flat); }
                                }
                                let cp = cover_paths.borrow().clone();
                                populate_grouped_grid(&grid_c, &groups, &cmd_c, &cp, &cover_widgets);
                                stack_c.set_visible_child(&grid_c);
                                // Only trigger cover fetch if we didn't already
                                if need_index {
                                    let _ = cmd_c.send(MpdCommand::FetchCovers(flat));
                                }
                            }
                        }
                        MpdEvent::SearchResults(results) => {
                            if results.is_empty() {
                                empty_c.set_text("No results found");
                                stack_c.set_visible_child(&empty_c);
                            } else {
                                let names: Vec<String> = results.iter().map(|(_, n)| n.clone()).collect();
                                if let Ok(mut store) = album_names.lock() {
                                    *store = names;
                                }
                                let cp = cover_paths.borrow().clone();
                                populate_album_grid(&grid_c, &results, &cmd_c, &cp, &cover_widgets);
                                stack_c.set_visible_child(&grid_c);
                            }
                        }
                        MpdEvent::DirectoryListing(path, entries) => {
                            if let Ok(mut fb) = fb_c.try_borrow_mut() {
                                fb.set_entries(&path, entries);
                            }
                        }
                        MpdEvent::Queue(queue) => {
                            while let Some(child) = ql_c.first_child() {
                                ql_c.remove(&child);
                            }
                            let csp = current_song_pos.get();
                            let q_tx = cmd_c.clone();
                            let item_ids: Arc<Mutex<HashMap<i32, i32>>> = Arc::new(Mutex::new(HashMap::new()));
                            let mut current_row: Option<gtk4::ListBoxRow> = None;
                            for (row_idx, item) in queue.iter().enumerate() {
                                if let Ok(mut m) = item_ids.lock() {
                                    m.insert(row_idx as i32, item.id);
                                }
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
                                rclick.connect_pressed(move |gest, _n, _x, _y| {
                                    let pop = gtk4::Popover::new();
                                    let popbox = Box::new(Orientation::Vertical, 0);
                                    let btn_play = gtk4::Button::with_label("Play Now");
                                    let btn_rem = gtk4::Button::with_label("Remove");
                                    let tp = tx_pop.clone();
                                    let ip = ipos;
                                    btn_play.connect_clicked(move |_| {
                                        let _ = tp.send(MpdCommand::PlayPosition(ip));
                                    });
                                    let tr = tx_pop.clone();
                                    btn_rem.connect_clicked(move |_| {
                                        let _ = tr.send(MpdCommand::DeleteId(iid));
                                    });
                                    popbox.append(&btn_play);
                                    popbox.append(&btn_rem);
                                    pop.set_child(Some(&popbox));
                                    if let Some(ref w) = gest.widget() { pop.set_parent(w); }
                                    pop.popup();
                                });
                                ql_c.append(&row);
                            }
                            // Select the current track's row in the list
                            if let Some(ref cr) = current_row {
                                ql_c.select_row(Some(cr));
                            }
                            // Refresh shared item_ids for Delete key lookup
                            *ids_w.borrow_mut() = item_ids.clone();
                        }
                        MpdEvent::CoverPaths(paths) => {
                            let mut cp = cover_paths.borrow_mut();
                            let widgets = cover_widgets.borrow();
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
                        MpdEvent::LibraryChanged => {
                            if let Ok(mut idx) = si_c.write() { *idx = SearchIndex::new(); }
                            let _ = cmd_c.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                        }
                        MpdEvent::Error(msg) => {
                            ci_c.set_css_classes(&["connection-indicator", "error"]);
                            toast_q.borrow().show_toast(&format!("MPD Error: {msg}"));
                        }
                    }
                    guard = match rx_c.lock() {
                        Ok(g) => g,
                        Err(poisoned) => {
                            log::error!("MPD event receiver mutex poisoned: {poisoned}");
                            return glib::ControlFlow::Break;
                        }
                    };
                }
                if let Err(mpsc::TryRecvError::Disconnected) = guard.try_recv() {
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            });

            window.present();

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
                 .error-label { color: #f44336; font-size: 0.85em; }"
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

/// Populate the album grid with group headers and album cells.
type CoverWidgets = std::cell::RefCell<std::collections::HashMap<String, gtk4::Picture>>;

fn populate_grouped_grid(grid: &FlowBox, groups: &[(String, Vec<(String, String)>)], cmd_tx: &mpsc::Sender<MpdCommand>, cover_paths: &std::collections::HashMap<String, Option<String>>, cover_widgets: &CoverWidgets) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    grid.set_homogeneous(true);

    for (header, albums) in groups {
        // Header label styled as bold, sized same as cover cells
        let header_label = Label::new(Some(&format!("{} ({})", header, albums.len())));
        header_label.set_halign(gtk4::Align::Start);
        header_label.set_valign(gtk4::Align::Center);
        header_label.set_size_request(200, 250);
        header_label.set_css_classes(&["album-group-header"]);
        header_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        grid.append(&header_label);

        for (i, (artist, album_name)) in albums.iter().enumerate() {
            let album_id = format!("{header}-album-{i}");
            let display_artist = if artist.is_empty() { "Unknown Artist" } else { artist };
            let cover_path = cover_paths.get(album_name).and_then(|o| o.as_deref());
            let cell = album_cover::create_album_cover(
                &album_id, album_name, album_name, display_artist, cover_path, cover_widgets, cmd_tx.clone());
            grid.append(&cell);
        }
    }
}

/// Populate the album grid from loaded album data.
/// Each cell gets hover buttons wired to cmd_tx for MPD commands.
fn populate_album_grid(grid: &FlowBox, albums: &[(String, String)], cmd_tx: &mpsc::Sender<MpdCommand>, cover_paths: &std::collections::HashMap<String, Option<String>>, cover_widgets: &CoverWidgets) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    grid.set_homogeneous(true);

    for (i, (artist, album_name)) in albums.iter().enumerate() {
        let album_id = format!("album-{i}");
        let display_artist = if artist.is_empty() { "Unknown Artist" } else { artist };
        let cover_path = cover_paths.get(album_name).and_then(|o| o.as_deref());
        let cell = album_cover::create_album_cover(
            &album_id, album_name, album_name, display_artist, cover_path, cover_widgets, cmd_tx.clone());
        grid.append(&cell);
    }
}

struct NowPlayingWidgets<'a> {
    title: &'a Label,
    artist: &'a Label,
    album: &'a Label,
    icon: &'a Label,
    time_display: &'a Label,
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
        }
        _ => {
            w.time_display.set_text("--:-- / --:--");
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

