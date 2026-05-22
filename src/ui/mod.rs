//! UI layer — GTK4 widgets and window management. Thread: UI (GTK main loop).

pub mod widgets;
pub(crate) mod grid;
pub(crate) mod queue;
pub(crate) mod theme;
pub(crate) mod now_playing;
pub(crate) mod settings;
pub(crate) mod help;
pub(crate) mod bottom_panel;
pub(crate) mod event_loop;

use crate::config::Config;
use crate::mpd::state_machine::{CommandSender, MpdCommand, MpdEvent, PlaybackUpdate};
use crate::search::{SearchCommand, SearchCommandSender};
use crate::state::SharedState;
use gtk4::prelude::*;
use adw::prelude::*;
use gtk4::{Box, DropTarget, EventControllerKey, Fixed, Label, ListBox, Orientation, Picture, ScrolledWindow};
use gtk4::gdk::DragAction;
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use grid::{AlbumCells, CELL_SLOT_H, CELL_SLOT_W, make_placeholder_cover, reposition};
use queue::{MiniCell, MiniGridData, SharedIds};
use theme::HC_CSS;
use now_playing::find_album_boundary;

pub fn build_ui(
    app: &gtk4::Application,
    state: SharedState,
    cmd_tx: CommandSender,
    event_rx: Arc<Mutex<mpsc::Receiver<MpdEvent>>>,
    conn_params: Arc<Mutex<crate::mpd::ConnectionTarget>>,
    mpris_update_tx: mpsc::Sender<PlaybackUpdate>,
    metadata_cache: std::sync::Arc<crate::metadata::MetadataCache>,
    search_cmd_tx: SearchCommandSender,
    toast_tx: mpsc::SyncSender<MpdEvent>,
) {
            // Clone early for the shutdown timer closure
            let shutdown_app = app.clone();
            let cfg = Config::load();
            let store = crate::state::Store::new(state.clone());

            let window = adw::ApplicationWindow::builder()
                .application(app)
                .default_width(1200)
                .default_height(800)
                .title(crate::strings::WINDOW_TITLE)
                .show_menubar(false)
                .build();

            // Restore window size from saved config (position is best-effort, ignored on Wayland)
            if let Some(ref geo) = cfg.window_geometry {
                window.set_default_size(geo.width, geo.height);
            }

            // Save window geometry on close — reuse already-loaded config
            {
                let w = window.clone();
                let cfg_for_close = cfg.clone();
                window.connect_close_request(move |_| {
                    let cur_w = w.default_width();
                    let cur_h = w.default_height();
                    let mut c = cfg_for_close.clone();
                    c.window_geometry = Some(crate::config::WindowGeometry {
                        width: cur_w.max(1), height: cur_h.max(1),
                        x: 0, y: 0,
                    });
                    let _ = c.save();
                    glib::Propagation::Proceed
                });
            }

            // Layout profile values cached for tick-callback and import-handler access
            let lp = cfg.layout_profile.as_ref()
                .map(|p| p.layout.clone())
                .unwrap_or_default();
            let rail_width_min = std::rc::Rc::new(std::cell::Cell::new(lp.rail_width_min));
            let rail_width_max = std::rc::Rc::new(std::cell::Cell::new(lp.rail_width_max));
            let split_ratio = std::rc::Rc::new(std::cell::Cell::new(lp.split_ratio));

            // Search result caps (mode-aware)
            let search_cap_album = std::rc::Rc::new(std::cell::Cell::new(cfg.search_track_cap_album as usize));
            let search_cap_folder = std::rc::Rc::new(std::cell::Cell::new(cfg.search_track_cap_folder as usize));

            let multi_view = adw::MultiLayoutView::new();

            // --- Left pane: group bar + album grid ---
            let left_pane_box = Box::new(Orientation::Vertical, 0);

            // Group selector bar — linked ToggleButtons, no icons
            let scfg = Config::load();
            let artist_tag = if scfg.use_album_artist { crate::strings::TAG_ALBUM_ARTIST } else { crate::strings::TAG_ARTIST };
            let group_pages: [(&str, &str); 4] = [(crate::strings::GROUP_ALBUMS, crate::strings::GROUP_ALBUMS), (crate::strings::GROUP_ARTISTS, artist_tag), (crate::strings::GROUP_YEARS, crate::strings::TAG_DATE), (crate::strings::GROUP_GENRES, crate::strings::TAG_GENRE)];
            let group_buttons = Box::new(Orientation::Horizontal, 0);
            group_buttons.set_css_classes(&["linked"]);
            group_buttons.set_halign(gtk4::Align::Center);
            // Map display name → ToggleButton for programmatic activation during search restore
            let group_btn_map: std::rc::Rc<std::cell::RefCell<HashMap<String, gtk4::ToggleButton>>> =
                std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            // Track the active group tag for search restore
            let active_group: std::rc::Rc<std::cell::RefCell<String>> =
                std::rc::Rc::new(std::cell::RefCell::new(crate::strings::GROUP_ALBUMS.to_string()));

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
            let loading_label = Label::new(Some(crate::strings::CONNECTING_TO_MPD));
            loading_label.set_halign(gtk4::Align::Center);
            loading_label.set_valign(gtk4::Align::Center);
            loading_label.set_widget_name("album-grid-status");
            let empty_label = Label::new(Some(crate::strings::NO_ALBUMS_FOUND));
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
                let cells_snapshot = resize_cells.borrow().clone();
                reposition(&resize_layout, &cells_snapshot, sw.width() as f64);
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
            folder_search.set_placeholder_text(Some(crate::strings::SEARCH_FILES_PLACEHOLDER));
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

            // Inline note for folder search cap ("Showing 500 of 800 track results")
            let folder_cap_note = Label::new(None);
            folder_cap_note.set_halign(gtk4::Align::Center);
            folder_cap_note.set_margin_top(4);
            folder_cap_note.set_visible(false);

            folder_content.append(&folder_search);
            folder_content.append(&folder_search_results);
            folder_content.append(&folder_cap_note);
            folder_content.append(&folder_browser.borrow().container.clone());

            // Top bar: Album/Folder switch (icons) + refresh + group switcher — single row
            let top_bar = gtk4::Box::new(Orientation::Horizontal, 8);
            top_bar.set_halign(gtk4::Align::Center);
            top_bar.set_margin_top(4);
            top_bar.set_margin_bottom(4);

            // Album/Folder mode toggle with icons
            let mode_btn_album = gtk4::ToggleButton::new();
            mode_btn_album.set_child(Some(&gtk4::Image::from_icon_name("media-optical")));
            mode_btn_album.set_tooltip_text(Some(crate::strings::MODE_ALBUM_TOOLTIP));
            let mode_btn_folder = gtk4::ToggleButton::new();
            mode_btn_folder.set_child(Some(&gtk4::Image::from_icon_name("folder")));
            mode_btn_folder.set_tooltip_text(Some(crate::strings::MODE_FOLDER_TOOLTIP));
            mode_btn_album.set_group(None::<&gtk4::ToggleButton>);
            mode_btn_folder.set_group(Some(&mode_btn_album));
            mode_btn_album.set_active(true);
            top_bar.append(&mode_btn_album);
            top_bar.append(&mode_btn_folder);

            // Refresh button — rescan MPD library
            let update_btn = gtk4::Button::new();
            update_btn.set_child(Some(&gtk4::Image::from_icon_name("view-refresh")));
            update_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_RESCAN));
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
            let mba_store = store.clone();
            let mba_ls = left_scroll.clone();
            let mba_fb = folder_browser.clone();
            let mba_fsc = folder_search_results.clone();
            mode_btn_album.connect_toggled(move |b| {
                if !b.is_active() { return; }
                // Save folder state
                if let Ok(fb) = mba_fb.try_borrow() {
                    let pos = fb.container.first_child()
                        .and_then(|first| first.next_sibling())
                        .and_then(|sibling| sibling.downcast::<gtk4::ScrolledWindow>().ok())
                        .map(|sw| sw.vadjustment().value()).unwrap_or(0.0);
                    mba_store.save_folder_browsing_state(
                        vec![std::path::PathBuf::from(fb.shared_path.borrow().clone())],
                        pos,
                    );
                }
                mba_fsc.set_visible(false);
                // Restore album scroll
                if let Ok(s) = mba_store.get_state().read() {
                    let pos = s.album_browsing.scroll_position.1;
                    let adj = mba_ls.vadjustment();
                    adj.set_value(pos.clamp(0.0, adj.upper() - adj.page_size()));
                }
                mba_mode.set_visible_child(&mba_ac);
                mba_store.switch_mode(crate::state::Mode::Album);
            });
            let mbf_mode = mode_stack.clone();
            let mbf_fc = folder_content.clone();
            let mbf_store = store.clone();
            let mbf_ls = left_scroll.clone();
            let mbf_cmd_tx = cmd_tx.clone();
            let mbf_sp = folder_browser.borrow().shared_path.clone();
            mode_btn_folder.connect_toggled(move |b| {
                if !b.is_active() { return; }
                // Save album scroll
                mbf_store.save_album_scroll_position(mbf_ls.vadjustment().value());
                mbf_mode.set_visible_child(&mbf_fc);
                // Restore folder
                let folder_path = if let Ok(s) = mbf_store.get_state().read() {
                    s.folder_browsing.expanded_paths.last()
                        .map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                } else { String::new() };
                *mbf_sp.borrow_mut() = folder_path.clone();
                let _ = mbf_cmd_tx.send(MpdCommand::ListDirectory(folder_path));
                mbf_store.switch_mode(crate::state::Mode::Folder);
            });

            // Mode switching: use stack transitions, save/restore scroll positions, update AppState
            let ms = mode_stack.clone();
            let ac = album_content.clone();
            let mode_store = store.clone();
            let ls_album = left_scroll.clone();
            let fb_save = folder_browser.clone();
            let fs_container_save = folder_search_results.clone();
            let album_mode_act = gtk4::gio::SimpleAction::new("album-mode", None);
            album_mode_act.connect_activate(move |_, _| {
                // Save folder browsing state before switching
                if let Ok(fb) = fb_save.try_borrow() {
                    let pos = fb.container.first_child()
                        .and_then(|first| first.next_sibling())
                        .and_then(|sibling| sibling.downcast::<gtk4::ScrolledWindow>().ok())
                        .map(|sw| sw.vadjustment().value())
                        .unwrap_or(0.0);
                    mode_store.save_folder_browsing_state(
                        vec![std::path::PathBuf::from(fb.shared_path.borrow().clone())],
                        pos,
                    );
                }
                // Hide folder search overlay when switching away
                fs_container_save.set_visible(false);
                // Restore album scroll position
                if let Ok(s) = mode_store.get_state().read() {
                    let pos = s.album_browsing.scroll_position.1;
                    let adj = ls_album.vadjustment();
                    adj.set_value(pos.clamp(0.0, adj.upper() - adj.page_size()));
                }
                ms.set_visible_child(&ac);
                mode_store.switch_mode(crate::state::Mode::Album);
            });
            app.add_action(&album_mode_act);
            app.set_accels_for_action("app.album-mode", &["<Ctrl>1"]);

            let ms2 = mode_stack.clone();
            let fc = folder_content.clone();
            let mode_store2 = store.clone();
            let cmd_tx2 = cmd_tx.clone();
            let ls_save = left_scroll.clone();
            let fsc2 = folder_search_results.clone();
            let fbb2 = folder_browser.clone();
            let sp2 = folder_browser.borrow().shared_path.clone();
            let folder_mode_act = gtk4::gio::SimpleAction::new("folder-mode", None);
            folder_mode_act.connect_activate(move |_, _| {
                // Save album scroll position before hiding
                mode_store2.save_album_scroll_position(ls_save.vadjustment().value());
                ms2.set_visible_child(&fc);
                // Restore folder browsing state
                let folder_path = if let Ok(s) = mode_store2.get_state().read() {
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
                mode_store2.switch_mode(crate::state::Mode::Folder);
            });
            app.add_action(&folder_mode_act);
            app.set_accels_for_action("app.folder-mode", &["<Ctrl>2"]);

            left_pane_box.append(&mode_content);

            // Album search entry
            let search_entry = gtk4::SearchEntry::new();
            search_entry.set_placeholder_text(Some(crate::strings::SEARCH_ALBUMS_PLACEHOLDER));
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
            app.add_action(&search_action);
            app.set_accels_for_action("app.search", &["<Ctrl>F"]);

            // Track the previously active group for restore on search clear
            let prev_group: std::rc::Rc<std::cell::RefCell<Option<String>>> = std::rc::Rc::new(std::cell::RefCell::new(None));

            // Search generation counter
            let search_gen: std::rc::Rc<std::cell::Cell<u64>> = std::rc::Rc::new(std::cell::Cell::new(0));
            let scmd = search_cmd_tx.clone();

            // stop_search: Escape or clear button
            let stop_btn_map = group_btn_map.clone();
            let prev_group_stop = prev_group.clone();
            search_entry.connect_stop_search(move |_| {
                // Restore the previously active group, fall back to "Albums"
                let display = prev_group_stop.borrow_mut().take().unwrap_or_else(|| crate::strings::GROUP_ALBUMS.into());
                if let Some(btn) = stop_btn_map.borrow().get(&display) {
                    btn.set_active(true);
                }
            });

            // search_changed: debounced with local index check + MPD fallback.
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
                    if let Some(btn) = se_btn_map.borrow().get(crate::strings::GROUP_ALBUMS) {
                        btn.set_active(true);
                    }
                    return;
                }

                if se_group.borrow().is_none() {
                    let name = se_active_group.borrow().clone();
                    let tags = [crate::strings::GROUP_ALBUMS, crate::strings::GROUP_ARTISTS, crate::strings::GROUP_YEARS, crate::strings::GROUP_GENRES];
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
                let cap_album = search_cap_album.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(150),
                    move || {
                        if gen_c.get() != this_gen { return; }
                        // Send to search worker (story 28-3): worker emits SearchResults via event_tx
                        scmd.send(SearchCommand::Search(qc.clone(), this_gen, cap_album.get()));
                    },
                );
            });

            // --- Folder search handler ---
            let fs_cmd = cmd_tx.clone();
            let fs_browser = folder_browser.clone();
            let fs_results = folder_search_results.clone();
            let fs_results2 = folder_search_results.clone();
            let fs_browser2 = folder_browser.clone();
            let fs_cap = search_cap_folder.clone();
            let fs_cap_note = folder_cap_note.clone();
            let fs_cap_note2 = folder_cap_note.clone();
            folder_search.connect_search_changed(move |entry| {
                let q = entry.text().to_string();
                if q.is_empty() {
                    fs_results.set_visible(false);
                    fs_cap_note.set_visible(false);
                    fs_browser.borrow().container.set_visible(true);
                    return;
                }
                let cmd = fs_cmd.clone();
                let cap = fs_cap.get();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(150),
                    move || {
                        let _ = cmd.send(MpdCommand::SearchFiles(q, cap));
                    },
                );
            });
            folder_search.connect_stop_search(move |_| {
                fs_results2.set_visible(false);
                fs_cap_note2.set_visible(false);
                fs_browser2.borrow().container.set_visible(true);
            });

            // Stack directly in scroll area (no overlay — pinned header removed)


            // GtkDropTarget for album grid reorder (plain Albums view only)
            let grid_store = store.clone();
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
                            grid_store.set_custom_album_order(order);
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

            // Inline note for album search cap ("Showing 100 of 250 track results")
            let album_cap_note = Label::new(None);
            album_cap_note.set_halign(gtk4::Align::Center);
            album_cap_note.set_margin_top(2);
            album_cap_note.set_margin_bottom(4);
            album_cap_note.set_visible(false);

            album_content.append(&search_entry);
            album_content.append(&left_stack);
            album_content.append(&album_cap_note);

            // --- Bottom panel (visible when sidebar hidden on narrow windows) ---
            let bp = bottom_panel::build(&cmd_tx);
            left_pane_box.append(&bp.panel);

            multi_view.set_child("left", &left_pane_box);

            // --- Right rail ---
            let right_pane = Box::new(Orientation::Vertical, 0);
            right_pane.set_margin_start(4);
            right_pane.set_margin_end(4);
            // Wrap right_pane in adw::Clamp to enforce a maximum width.
            // set_size_request only sets a MINIMUM in GTK4 — if children request
            // more space (e.g. large cover art textures), the natural size expands.
            // adw::Clamp clamps the child's width to maximum_size, giving us a true max.
            let right_pane_clamp = adw::Clamp::new();
            right_pane_clamp.set_child(Some(&right_pane));
            right_pane_clamp.set_hexpand(false);
            right_pane_clamp.set_maximum_size(rail_width_min.get() as i32);

            // Settings gear button with Ctrl+, shortcut
            let settings_btn = gtk4::Button::new();
            settings_btn.set_child(Some(&gtk4::Image::from_icon_name("emblem-system")));
            settings_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_SETTINGS));
            let btn_clone = settings_btn.clone();
            let sw = window.clone();
            let cp = conn_params.clone();
            let tx = cmd_tx.clone();
            let rwmin = rail_width_min.clone();
            let rwmax = rail_width_max.clone();
            let sratio = split_ratio.clone();
            settings_btn.connect_clicked(move |_| {
                crate::ui::settings::show(&sw, &cp, &tx, &rwmin, &rwmax, &sratio);
            });
            top_bar.append(&settings_btn);

            // Ctrl+, settings shortcut
            let set_action = gtk4::gio::SimpleAction::new("settings", None);
            set_action.connect_activate(move |_, _| { btn_clone.emit_clicked(); });
            app.add_action(&set_action);
            app.set_accels_for_action("app.settings", &["<Ctrl>comma"]);

            // Ctrl+? keyboard shortcuts help
            let shortcuts_window = window.clone();
            let help_act = gtk4::gio::SimpleAction::new("shortcuts", None);
            help_act.connect_activate(move |_, _| {
                crate::ui::help::show(&shortcuts_window);
            });
            app.add_action(&help_act);
            app.set_accels_for_action("app.shortcuts", &["<Ctrl>question"]);

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
            let track_title = gtk4::Button::with_label(crate::strings::NO_TRACK_PLAYING);
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
            prev_album_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_PREV_ALBUM));
            let prev_track_btn = gtk4::Button::from_icon_name("media-seek-backward-symbolic");
            prev_track_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_PREV_TRACK));
            let play_pause_btn = gtk4::Button::new();
            play_pause_btn.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
            play_pause_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_PLAY_PAUSE));
            let next_track_btn = gtk4::Button::from_icon_name("media-seek-forward-symbolic");
            next_track_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_NEXT_TRACK));
            let next_album_btn = gtk4::Button::from_icon_name("media-skip-forward-symbolic");
            next_album_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_NEXT_ALBUM));

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

            multi_view.set_child("right", &right_pane_clamp);

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
            wide_right.set_hexpand(false);
            // Set initial right rail width before first tick callback
            let init_rail_w = rail_width_min.get() as i32;
            wide_right.set_size_request(init_rail_w, -1);
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
            app.add_action(&toggle_bs_act);
            app.set_accels_for_action("app.toggle-sidebar", &["<Ctrl>B"]);

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
                    let start_idx = first_row.saturating_mul(cols).min(cells_binding.len());
                    let end_idx = (first_row + visible_rows).saturating_mul(cols).min(cells_binding.len());
                    if start_idx >= end_idx {
                        return glib::ControlFlow::Break;
                    }
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
            let fc_queue_entries: std::rc::Rc<std::cell::RefCell<Vec<crate::mpd::QueueEntry>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let fc_stack = left_stack.clone();
            let fc_scroll = left_scroll.clone();
            let fc_empty = empty_label.clone();
            let fc_cmd = cmd_tx.clone();
            let fc_fb = folder_browser.clone();
            let fc_fs_list = folder_search_list.clone();
            let fc_fs_container = folder_search_results.clone();
            let fc_album_cap_note = album_cap_note.clone();
            let fc_folder_cap_note = folder_cap_note.clone();
            let fc_ql = queue_list.clone();
            let fc_queue_popover = queue_popover.clone();
            let fc_ids = item_ids_w.clone();
            let fc_scmd = scmd.clone();
            let fc_mc = metadata_cache.clone();
            let fc_toast = toast_overlay.clone();
            let mem_toast = toast_overlay.clone();
            let fc_ev_cover_paths = cover_paths.clone();
            let fc_ev_cover_tex_cache = cover_texture_cache.clone();
            let fc_mini_cw = mini_cover_widgets.clone();
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
            let fc_bottom_panel = bp.panel.clone();
            let fc_bottom_sheet = bottom_sheet.clone();
            let fc_bp_title = bp.title.clone();
            let fc_bp_play = bp.play.clone();
            let fc_track_revealer = track_revealer.clone();
            let fc_last_repos_w: std::cell::Cell<f64> = std::cell::Cell::new(0.0);

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

            // Bottom queue button → toggle BottomSheet in narrow mode
            let bq_mv = fc_multi_view.clone();
            let bq_bs = fc_bottom_sheet.clone();
            bp.queue_btn.connect_clicked(move |_| {
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

            // Build UiHandles for the event dispatch loop (story 43-4)
            let handles = event_loop::UiHandles {
                cmd_tx: fc_cmd.clone(),
                search_cmd_tx: fc_scmd.clone(),
                mpris_update_tx: fc_mpris_update.clone(),
                toast_overlay: fc_toast.clone(),
                state: fc_state.clone(),
                metadata_cache: fc_mc.clone(),
                current_album: fc_current_album.clone(),
                current_song_pos: fc_current_song_pos.clone(),
                track_title: fc_tl.clone(),
                track_artist: fc_ar.clone(),
                track_album: fc_al.clone(),
                track_year: fc_yl.clone(),
                play_pause_btn: fc_play_pause.clone(),
                status_dot: fc_status_dot.clone(),
                pos_label: fc_pos.clone(),
                len_label: fc_len.clone(),
                seekbar: fc_seekbar.clone(),
                fmt_label: fc_fmt_label.clone(),
                bitrate_label: fc_bitrate_label.clone(),
                np_cover: fc_np_cover.clone(),
                np_cover_stack: fc_np_cover_stack.clone(),
                bp_title: fc_bp_title.clone(),
                bp_play: fc_bp_play.clone(),
                queue_list: fc_ql.clone(),
                queue_popover: fc_queue_popover.clone(),
                item_ids: fc_ids.clone(),
                queue_entries: fc_queue_entries.clone(),
                track_listbox: fc_track_listbox.clone(),
                popover_track_data: fc_popover_track_data.clone(),
                album_cells: fc_ev_cells.clone(),
                album_layout: fc_layout.clone(),
                cover_paths: fc_ev_cover_paths.clone(),
                texture_cache: fc_ev_cover_tex_cache.clone(),
                left_stack: fc_stack.clone(),
                left_scroll: fc_scroll.clone(),
                empty_label: fc_empty.clone(),
                album_cap_note: fc_album_cap_note.clone(),
                mini_fixed: fc_mini_fixed.clone(),
                mini_cells: fc_mini_cells.clone(),
                mini_data: fc_mini_data.clone(),
                mini_current: fc_mini_current.clone(),
                mini_cover_widgets: fc_mini_cw.clone(),
                mini_scroll: fc_mini_scroll_ref.clone(),
                folder_browser: fc_fb.clone(),
                folder_search_list: fc_fs_list.clone(),
                folder_search_results: fc_fs_container.clone(),
                folder_cap_note: fc_folder_cap_note.clone(),
                multi_view: fc_multi_view.clone(),
                bottom_panel: fc_bottom_panel.clone(),
                bottom_sheet: fc_bottom_sheet.clone(),
                queue_stack: fc_queue_stack.clone(),
                queue_scroll: fc_queue_scroll_ref.clone(),
                prev_mode: fc_prev_mode.clone(),
                last_repos_w: fc_last_repos_w.clone(),
                shutdown: fc_shutdown.clone(),
                sort_mode: fc_sort_mode,
                search_gen: search_gen.clone(),
                active_group: active_group.clone(),
            };

            let tick_rx = event_rx.clone();
            let tick_toast_tx = toast_tx.clone();
            let tick_rail_min = rail_width_min.clone();
            let tick_wide_right = wide_right.clone();
            let tick_clamp = right_pane_clamp.clone();
            window.add_tick_callback(move |widget, _fc| {
                // Switch layout on narrow windows (<800px), show bottom transport bar
                let win_width = widget.width() as f64;
                let narrow = win_width < 800.0;
                let current_narrow = handles.multi_view.layout_name().as_deref() == Some("narrow");
                if narrow != current_narrow {
                    handles.multi_view.set_layout_name(if narrow { "narrow" } else { "wide" });
                    if !narrow {
                        handles.bottom_sheet.set_open(false);
                    }
                }
                handles.bottom_panel.set_visible(narrow);

                // Update right rail width from layout profile (wide mode only)
                let grid_est_width: f64; // estimated grid viewport width for initial reposition
                let mini_vpw_est: i32;   // estimated mini-grid viewport width for initial rebuild
                if !narrow {
                    let rail_w = tick_rail_min.get() as i32;
                    tick_wide_right.set_size_request(rail_w, -1);
                    tick_clamp.set_maximum_size(rail_w);
                    grid_est_width = (win_width - rail_w as f64).max(200.0);
                    mini_vpw_est = rail_w;
                } else {
                    grid_est_width = win_width.max(200.0);
                    mini_vpw_est = win_width as i32;
                }

                // Reposition album grid when viewport width changes (e.g. window resize)
                let grid_w = handles.left_scroll.width() as f64;
                let last_w = handles.last_repos_w.get();
                if (grid_w - last_w).abs() > 1.0 && grid_w > 0.0 {
                    handles.last_repos_w.set(grid_w);
                    // Clone cells to drop RefCell borrow before reposition() —
                    // layout.move_() can trigger scroll adjustment signals that
                    // re-enter and borrow the same album_cells RefCell.
                    let cells_snapshot = handles.album_cells.borrow().clone();
                    reposition(&handles.album_layout, &cells_snapshot, grid_w);
                }

                // Ensure queue display matches current mode
                let cur_mode = handles.state.read().map(|s| s.mode).unwrap_or(crate::state::Mode::Album);
                if cur_mode != handles.prev_mode.get() {
                    handles.prev_mode.set(cur_mode);
                    match cur_mode {
                        crate::state::Mode::Album => handles.queue_stack.set_visible_child(&handles.mini_scroll),
                        crate::state::Mode::Folder => handles.queue_stack.set_visible_child(&handles.queue_scroll),
                    }
                }

                event_loop::process_events(&handles, &tick_rx, &tick_toast_tx, grid_est_width, mini_vpw_est)
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

            // Memory monitoring timer (30s interval, rate-limited warnings at 60s)
            let mem_last_warning: std::rc::Rc<std::cell::RefCell<Option<std::time::Instant>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            glib::timeout_add_local(std::time::Duration::from_secs(30), move || {
                if let Some(rss_mb) = crate::memory::read_rss_mb() {
                    let threshold = crate::config::Config::load().memory.warning_threshold_mb;
                    if rss_mb >= threshold {
                        let mut last = mem_last_warning.borrow_mut();
                        let now = std::time::Instant::now();
                        let should_warn = match *last {
                            Some(t) => now.duration_since(t).as_secs() >= 60,
                            None => true,
                        };
                        if should_warn {
                            *last = Some(now);
                            let toast = adw::Toast::new(&crate::strings::memory_warning(rss_mb));
                            toast.set_timeout(5);
                            mem_toast.add_toast(toast);
                        }
                    }
                } else {
                    log::debug!("[memory] /proc/self/status unreadable, skipping memory check");
                }
                glib::ControlFlow::Continue
            });
}

