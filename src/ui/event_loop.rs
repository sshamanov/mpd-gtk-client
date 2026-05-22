//! Event loop — drains MPD events from the channel on each frame-clock tick
//! and dispatches to UI widgets. Extracted from ui/mod.rs tick callback (story 43-4).

use crate::coverart::cover_key;
use crate::mpd::state_machine::{CommandSender, MpdCommand, MpdEvent, PlaybackUpdate};
use crate::search::{SearchCommand, SearchCommandSender};
use crate::state::SharedState;
use crate::ui::grid::{
    format_year_badge, group_caption_for_view, placeholder_texture, reposition, AlbumCell,
    AlbumCells, CAPTION_H,
};
use crate::ui::now_playing::{handle_now_playing, NowPlayingWidgets};
use crate::ui::queue::{rebuild_mini_fixed, MiniCell, MiniGridData, MiniGridItem, SharedIds};
use crate::ui::widgets::folder_tree::FolderBrowser;
use gtk4::prelude::*;
use gtk4::{Box, DragSource, Fixed, Label, ListBox, Orientation, Picture,
    ScrolledWindow, Stack};
use gtk4::gdk::DragAction;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// All widget references and shared state the event dispatch loop needs.
pub(crate) struct UiHandles {
    // --- Channels ---
    pub cmd_tx: CommandSender,
    pub search_cmd_tx: SearchCommandSender,
    pub mpris_update_tx: mpsc::Sender<PlaybackUpdate>,
    pub toast_overlay: adw::ToastOverlay,

    // --- State ---
    pub state: SharedState,
    pub metadata_cache: std::sync::Arc<crate::metadata::MetadataCache>,
    pub current_album: Rc<std::cell::RefCell<Option<String>>>,
    pub current_song_pos: Cell<Option<i32>>,

    // --- Now-playing widgets ---
    pub track_title: gtk4::Button,
    pub track_artist: Label,
    pub track_album: Label,
    pub track_year: Label,
    pub play_pause_btn: gtk4::Button,
    pub status_dot: Box,
    pub pos_label: Label,
    pub len_label: Label,
    pub seekbar: gtk4::Scale,
    pub fmt_label: Label,
    pub bitrate_label: Label,
    pub np_cover: Picture,
    pub np_cover_stack: Stack,

    // --- Bottom panel ---
    pub bp_title: Label,
    pub bp_play: gtk4::Button,

    // --- Queue ---
    pub queue_list: ListBox,
    pub queue_popover: Rc<std::cell::RefCell<Option<gtk4::Popover>>>,
    pub item_ids: SharedIds,
    pub queue_entries: Rc<std::cell::RefCell<Vec<crate::mpd::QueueEntry>>>,
    pub track_listbox: ListBox,
    pub popover_track_data: Rc<std::cell::RefCell<Vec<(String, String, f64)>>>,

    // --- Grid ---
    pub album_cells: AlbumCells,
    pub album_layout: Fixed,
    pub cover_paths: Rc<std::cell::RefCell<HashMap<String, Option<String>>>>,
    pub texture_cache: Rc<std::cell::RefCell<HashMap<String, gdk4::Texture>>>,
    pub left_stack: Stack,
    pub left_scroll: ScrolledWindow,
    pub empty_label: Label,
    pub album_cap_note: Label,

    // --- Mini grid ---
    pub mini_fixed: Fixed,
    pub mini_cells: Rc<std::cell::RefCell<Vec<MiniCell>>>,
    pub mini_data: MiniGridData,
    pub mini_current: Rc<std::cell::RefCell<Option<String>>>,
    pub mini_cover_widgets: Rc<std::cell::RefCell<HashMap<String, Picture>>>,
    pub mini_scroll: ScrolledWindow,

    // --- Folder ---
    pub folder_browser: Rc<std::cell::RefCell<FolderBrowser>>,
    pub folder_search_list: ListBox,
    pub folder_search_results: ScrolledWindow,
    pub folder_cap_note: Label,

    // --- Layout ---
    pub multi_view: adw::MultiLayoutView,
    pub bottom_panel: Box,
    pub bottom_sheet: adw::BottomSheet,
    pub queue_stack: Stack,
    pub queue_scroll: ScrolledWindow,
    pub prev_mode: Cell<crate::state::Mode>,
    pub last_repos_w: Cell<f64>,

    // --- Other ---
    pub shutdown: gtk4::Application,
    pub sort_mode: u32,
    pub search_gen: Rc<Cell<u64>>,
    pub active_group: Rc<std::cell::RefCell<String>>,
}

/// Drain and dispatch MPD events from the receiver. Called once per frame-clock tick.
///
/// `grid_est_width` and `mini_vpw_est` are fallback viewport widths computed by the
/// caller from the window width, used when the actual scroll window reports width 0
/// (e.g. before the first size-allocate).
pub(crate) fn process_events(
    handles: &UiHandles,
    event_rx: &Arc<Mutex<mpsc::Receiver<MpdEvent>>>,
    toast_tx: &mpsc::SyncSender<MpdEvent>,
    grid_est_width: f64,
    mini_vpw_est: i32,
) -> glib::ControlFlow {
    let mut guard = match event_rx.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            log::error!("MPD event receiver mutex poisoned: {poisoned}");
            handles.shutdown.quit();
            return glib::ControlFlow::Break;
        }
    };
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
        let router_event = match &event {
            MpdEvent::Toast { .. } | MpdEvent::Connected | MpdEvent::Disconnected => {
                Some(event.clone())
            }
            _ => None,
        };
        match event {
            MpdEvent::Connected => {
                crate::state::apply_event(&handles.state, &MpdEvent::Connected);
                handles.search_cmd_tx.send(SearchCommand::Reset);
                let _ = handles.cmd_tx.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
                let _ = handles.cmd_tx.send(MpdCommand::ListQueue);
            }
            MpdEvent::Connecting => {
                crate::state::apply_event(&handles.state, &MpdEvent::Connecting);
            }
            MpdEvent::Disconnected => {
                crate::state::apply_event(&handles.state, &MpdEvent::Disconnected);
            }
            MpdEvent::StateChanged(update) => {
                crate::state::apply_event(
                    &handles.state,
                    &MpdEvent::StateChanged(update.clone()),
                );
                handles.current_song_pos.set(update.song.map(|s| s as i32));
                let album_changed = handles.current_album.borrow().as_deref()
                    != update.album.as_deref();
                *handles.current_album.borrow_mut() = update.album.clone();
                if album_changed {
                    if let Some(ref album) = update.album {
                        let _ = handles.cmd_tx.send(MpdCommand::ListAlbumTracks(album.clone()));
                    } else {
                        handles.track_listbox.remove_all();
                        handles.popover_track_data.borrow_mut().clear();
                    }
                    // Rebuild mini grid to update current-album highlight
                    *handles.mini_current.borrow_mut() = update.album.clone();
                    let items = handles.mini_data.borrow();
                    if !items.is_empty() {
                        let raw_vpw = handles.mini_scroll.width();
                        let vpw = if raw_vpw > 0 { raw_vpw } else { mini_vpw_est };
                        let cells = rebuild_mini_fixed(
                            &handles.mini_fixed,
                            &items,
                            &handles.cover_paths,
                            &handles.mini_cover_widgets,
                            &handles.mini_current.borrow(),
                            &handles.cmd_tx,
                            vpw,
                        );
                        *handles.mini_cells.borrow_mut() = cells;
                    }
                }
                // Update popover selection to current track (same-album track change)
                if let Some(ref file) = update.file {
                    let data = handles.popover_track_data.borrow();
                    if let Some(pos) = data.iter().position(|(_, f, _)| f == file) {
                        if let Some(row) = handles.track_listbox.row_at_index(pos as i32) {
                            handles.track_listbox.select_row(Some(&row));
                        }
                    }
                }
                handle_now_playing(
                    &update,
                    NowPlayingWidgets {
                        title: &handles.track_title,
                        artist: &handles.track_artist,
                        album: &handles.track_album,
                        year: &handles.track_year,
                        status_dot: &handles.status_dot,
                        play_pause_btn: &handles.play_pause_btn,
                        pos_label: &handles.pos_label,
                        len_label: &handles.len_label,
                        seekbar: &handles.seekbar,
                        fmt_label: &handles.fmt_label,
                        bitrate_label: &handles.bitrate_label,
                        cover: &handles.np_cover,
                        cover_stack: &handles.np_cover_stack,
                        cover_paths: &handles.cover_paths,
                    },
                    &handles.mpris_update_tx,
                );

                // Update bottom panel now-playing
                handles.bp_title.set_label(
                    update.title.as_deref().unwrap_or(crate::strings::NO_TRACK_PLAYING),
                );
                match update.state.as_str() {
                    "play" => handles.bp_play.set_child(Some(
                        &gtk4::Image::from_icon_name("media-playback-pause-symbolic"),
                    )),
                    _ => handles.bp_play.set_child(Some(
                        &gtk4::Image::from_icon_name("media-playback-start-symbolic"),
                    )),
                }
            }
            MpdEvent::Albums(albums) => {
                let flat_for_index: Vec<(String, String)> = albums
                    .iter()
                    .map(|m| (m.album_artist.clone(), m.album.clone()))
                    .collect();
                handles.search_cmd_tx.send(SearchCommand::BuildIndex(flat_for_index));
                if albums.is_empty() {
                    handles.empty_label.set_text(crate::strings::NO_ALBUMS_FOUND);
                    handles.left_stack.set_visible_child(&handles.empty_label);
                } else {
                    let mut sorted = albums.clone();
                    let sort_idx = handles.sort_mode as usize;
                    match sort_idx {
                        1 => sorted.sort_by(|a, b| a.album.cmp(&b.album)),
                        2 => sorted.sort_by(|a, b| b.album_artist.cmp(&a.album_artist)),
                        3 => sorted.sort_by(|a, b| b.album.cmp(&a.album)),
                        _ => sorted.sort_by(|a, b| a.album_artist.cmp(&b.album_artist)),
                    }
                    // Apply custom session order
                    if let Ok(st) = handles.state.read() {
                        let order = &st.album_browsing.custom_album_order;
                        if !order.is_empty() {
                            sorted.sort_by_key(|m| {
                                order
                                    .iter()
                                    .position(|o| o == &m.album)
                                    .unwrap_or(usize::MAX)
                            });
                        }
                    }
                    // Build AlbumCells
                    let cmd = handles.cmd_tx.clone();
                    let cmd2 = handles.cmd_tx.clone();
                    let all_albums: Vec<(String, String)> = sorted
                        .iter()
                        .map(|m| (m.album_artist.clone(), m.album.clone()))
                        .collect();
                    let layout = handles.album_layout.clone();
                    let cells_rc = handles.album_cells.clone();
                    let cp = handles.cover_paths.clone();
                    let tc = handles.texture_cache.clone();
                    // Clear old widgets from layout
                    while let Some(child) = layout.first_child() {
                        layout.remove(&child);
                    }
                    let mut new_cells: Vec<AlbumCell> = Vec::with_capacity(sorted.len());
                    for (_i, meta) in sorted.iter().enumerate() {
                        let cell = crate::ui::widgets::AlbumCoverCell::new(cmd2.clone());
                        let key = cover_key(&meta.album_artist, &meta.album);
                        cell.set_album(
                            &meta.album,
                            &meta.album_artist,
                            meta.year.as_deref(),
                            &meta.album,
                        );
                        cell.set_year_badge(
                            format_year_badge(meta.year.as_deref()).as_deref(),
                        );
                        let cached = tc.borrow().get(&key).cloned();
                        if let Some(tex) = cached {
                            cell.set_cover_texture(&tex);
                        } else if let Some(p) = cp.borrow().get(&key).and_then(|o| o.clone()) {
                            cell.set_cover_filename(&p);
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
                    let sw = handles.left_scroll.clone();
                    let w = sw.width() as f64;
                    let effective_w = if w > 0.0 { w } else { grid_est_width };
                    let cells_snapshot = cells_rc.borrow().clone();
                    reposition(&layout, &cells_snapshot, effective_w);
                    handles.left_stack.set_visible_child(&handles.left_scroll);
                    if !all_albums.is_empty() {
                        log::debug!(
                            "[ui] initial load: enqueuing {} albums for cover fetch",
                            all_albums.len()
                        );
                        let _ = cmd.send(MpdCommand::FetchCovers(all_albums));
                    }
                }
            }
            MpdEvent::AlbumsGrouped(groups) => {
                let flat: Vec<(String, String)> = groups
                    .iter()
                    .flat_map(|(_, a)| {
                        a.iter()
                            .map(|m| (m.album_artist.clone(), m.album.clone()))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                if groups.is_empty() {
                    handles.empty_label.set_text(crate::strings::NO_ALBUMS_FOUND);
                    handles.left_stack.set_visible_child(&handles.empty_label);
                } else {
                    handles.search_cmd_tx.send(SearchCommand::BuildIndex(flat.clone()));
                    let single_group = groups.len() == 1;
                    let view_mode = handles.active_group.borrow().clone();
                    let layout = handles.album_layout.clone();
                    let cells_rc = handles.album_cells.clone();
                    let cmd = handles.cmd_tx.clone();
                    let cmd2 = handles.cmd_tx.clone();
                    let cp = handles.cover_paths.clone();
                    let tc = handles.texture_cache.clone();
                    while let Some(child) = layout.first_child() {
                        layout.remove(&child);
                    }
                    let mut new_cells: Vec<AlbumCell> = Vec::new();
                    let mut all_albums: Vec<(String, String)> = Vec::new();
                    for (header, albums) in groups.iter() {
                        let group_value: Option<String> =
                            if single_group { None } else { Some(header.clone()) };
                        let mut sorted_albums = albums.clone();
                        if single_group {
                            if let Ok(st) = handles.state.read() {
                                let order = &st.album_browsing.custom_album_order;
                                if !order.is_empty() {
                                    sorted_albums.sort_by_key(|m| {
                                        order
                                            .iter()
                                            .position(|o| o == &m.album)
                                            .unwrap_or(usize::MAX)
                                    });
                                }
                            }
                        }
                        for meta in &sorted_albums {
                            let cell =
                                crate::ui::widgets::AlbumCoverCell::new(cmd2.clone());
                            let key = cover_key(&meta.album_artist, &meta.album);
                            let caption =
                                group_caption_for_view(&view_mode, header, meta);
                            let year_badge = format_year_badge(meta.year.as_deref());
                            cell.set_album(
                                &meta.album,
                                &meta.album_artist,
                                meta.year.as_deref(),
                                &meta.album,
                            );
                            cell.set_group_captions(
                                caption.as_deref().unwrap_or(&[]),
                            );
                            cell.set_year_badge(year_badge.as_deref());
                            let cached = tc.borrow().get(&key).cloned();
                            if let Some(tex) = cached {
                                cell.set_cover_texture(&tex);
                            } else if let Some(p) =
                                cp.borrow().get(&key).and_then(|o| o.clone())
                            {
                                cell.set_cover_filename(&p);
                            } else {
                                cell.set_cover_texture(&placeholder_texture(
                                    &meta.album_artist,
                                ));
                            }
                            let caption_label = if group_value.is_some()
                                && new_cells
                                    .last()
                                    .map_or(true, |c: &AlbumCell| {
                                        c.group_value.as_deref()
                                            != group_value.as_deref()
                                    })
                            {
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
                            all_albums.push((
                                meta.album_artist.clone(),
                                meta.album.clone(),
                            ));
                        }
                    }
                    *cells_rc.borrow_mut() = new_cells;
                    let sw = handles.left_scroll.clone();
                    let w = sw.width() as f64;
                    let effective_w = if w > 0.0 { w } else { grid_est_width };
                    let cells_snapshot = cells_rc.borrow().clone();
                    reposition(&layout, &cells_snapshot, effective_w);
                    handles.left_stack.set_visible_child(&handles.left_scroll);
                    if !all_albums.is_empty() {
                        log::debug!(
                            "[ui] grouped: enqueuing {} albums for cover fetch",
                            all_albums.len()
                        );
                        let _ = cmd.send(MpdCommand::FetchCovers(all_albums));
                    }
                }
            }
            MpdEvent::SearchIndexing => {
                log::debug!("[ui] SearchIndexing — index not yet built");
                handles.empty_label.set_text(crate::strings::INDEXING);
                handles.left_stack.set_visible_child(&handles.empty_label);
                handles.album_cap_note.set_visible(false);
            }
            MpdEvent::SearchResults {
                results,
                generation,
                total,
            } => {
                if generation != 0 && generation != handles.search_gen.get() {
                    log::debug!(
                        "[ui] Discarding stale SearchResults (gen {generation}, current {cur})",
                        cur = handles.search_gen.get()
                    );
                    return glib::ControlFlow::Continue;
                }
                let capped = total > results.len();
                if results.is_empty() {
                    handles.empty_label.set_text(crate::strings::NO_RESULTS_FOUND);
                    handles.left_stack.set_visible_child(&handles.empty_label);
                    handles.album_cap_note.set_visible(false);
                } else {
                    let layout = handles.album_layout.clone();
                    let cells_rc = handles.album_cells.clone();
                    let cp = handles.cover_paths.clone();
                    let tc = handles.texture_cache.clone();
                    let cmd = handles.cmd_tx.clone();
                    while let Some(child) = layout.first_child() {
                        layout.remove(&child);
                    }
                    let mut new_cells: Vec<AlbumCell> =
                        Vec::with_capacity(results.len());
                    for (_i, (artist, name)) in results.iter().enumerate() {
                        let cell =
                            crate::ui::widgets::AlbumCoverCell::new(cmd.clone());
                        let key = cover_key(artist, name);
                        let year = handles
                            .metadata_cache
                            .get(artist, name)
                            .and_then(|m| m.year.clone());
                        let year_badge = year
                            .as_deref()
                            .and_then(|y_str| format_year_badge(Some(y_str)));
                        cell.set_album(name, artist, year.as_deref(), name);
                        cell.set_year_badge(year_badge.as_deref());
                        let cached = tc.borrow().get(&key).cloned();
                        if let Some(tex) = cached {
                            cell.set_cover_texture(&tex);
                        } else if let Some(p) =
                            cp.borrow().get(&key).and_then(|o| o.clone())
                        {
                            cell.set_cover_filename(&p);
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
                    let sw = handles.left_scroll.clone();
                    let w = sw.width() as f64;
                    let effective_w = if w > 0.0 { w } else { grid_est_width };
                    let cells_snapshot = cells_rc.borrow().clone();
                    reposition(&layout, &cells_snapshot, effective_w);
                    handles.left_stack.set_visible_child(&handles.left_scroll);
                    let _ = cmd.send(MpdCommand::FetchCovers(results.clone()));
                    if capped {
                        handles.album_cap_note.set_text(
                            &crate::strings::cap_note(results.len(), total),
                        );
                        handles.album_cap_note.set_visible(true);
                    } else {
                        handles.album_cap_note.set_visible(false);
                    }
                }
            }
            MpdEvent::FileSearchResults(results, total) => {
                handles.folder_search_list.remove_all();
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
                    handles.folder_search_list.append(&row);
                }
                let capped = total > results.len();
                if !results.is_empty() {
                    handles.folder_search_results.set_visible(true);
                    handles.folder_browser.borrow().container.set_visible(false);
                    if capped {
                        handles.folder_cap_note.set_text(
                            &crate::strings::cap_note(results.len(), total),
                        );
                        handles.folder_cap_note.set_visible(true);
                    } else {
                        handles.folder_cap_note.set_visible(false);
                    }
                } else {
                    handles.folder_cap_note.set_visible(false);
                }
            }
            MpdEvent::DirectoryListing(path, entries) => {
                if let Ok(mut fb) = handles.folder_browser.try_borrow_mut() {
                    fb.set_entries(&path, entries);
                }
            }
            MpdEvent::AlbumTracks(tracks) => {
                *handles.popover_track_data.borrow_mut() = tracks.clone();
                handles.track_listbox.remove_all();
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
                        let dur_text = format!(
                            "{}:{:02}",
                            *duration as u64 / 60,
                            *duration as u64 % 60
                        );
                        let dur_lbl = Label::new(Some(&dur_text));
                        dur_lbl.set_halign(gtk4::Align::End);
                        dur_lbl.set_css_classes(&["time-display"]);
                        hbox.append(&dur_lbl);
                    }
                    row.set_child(Some(&hbox));
                    handles.track_listbox.append(&row);
                }
                // Select current track if it belongs to this album
                if let Some(csp) = handles.current_song_pos.get() {
                    let entries = handles.queue_entries.borrow();
                    if let Some(current_entry) =
                        entries.iter().find(|e| e.position == csp)
                    {
                        let current_file = &current_entry.file;
                        let data = handles.popover_track_data.borrow();
                        if let Some(pos) =
                            data.iter().position(|(_, f, _)| f == current_file)
                        {
                            if let Some(row) =
                                handles.track_listbox.row_at_index(pos as i32)
                            {
                                handles.track_listbox.select_row(Some(&row));
                            }
                        }
                    }
                }
            }
            MpdEvent::Queue(queue) => {
                handles.queue_list.remove_all();
                *handles.queue_entries.borrow_mut() = queue.clone();
                let csp = handles.current_song_pos.get();
                let q_tx = handles.cmd_tx.clone();
                let mut item_ids = HashMap::new();
                let mut current_row: Option<gtk4::ListBoxRow> = None;
                for (row_idx, item) in queue.iter().enumerate() {
                    item_ids.insert(row_idx as i32, item.id);
                    let row = gtk4::ListBoxRow::new();
                    let vbox = Box::new(Orientation::Vertical, 0);
                    vbox.set_margin_start(8);
                    vbox.set_margin_top(2);
                    vbox.set_margin_bottom(2);
                    let dur = item
                        .duration
                        .map(|d| {
                            let t = d as u64;
                            format!("{}:{:02}", t / 60, t % 60)
                        })
                        .unwrap_or_default();
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
                        Some(gtk4::gdk::ContentProvider::for_value(&value))
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
                    // Right-click context menu popover
                    let rclick = gtk4::GestureClick::new();
                    rclick.set_button(3);
                    rclick.set_propagation_phase(gtk4::PropagationPhase::Capture);
                    let tx_pop = q_tx.clone();
                    let ipos = item.position;
                    let iid = item.id;
                    let current_song = handles.current_song_pos.get();
                    let qp = handles.queue_popover.clone();
                    let row_for_pop = row.clone();
                    rclick.connect_pressed(move |_gest, _n, x, y| {
                        log::debug!("Queue context menu fired, pos={ipos}");
                        if let Some(ref old) = *qp.borrow() {
                            old.popdown();
                        }
                        let pop = gtk4::Popover::new();
                        let popbox = Box::new(Orientation::Vertical, 0);
                        let btn_play = gtk4::Button::with_label(crate::strings::PLAY_NOW);
                        let btn_next = gtk4::Button::with_label(crate::strings::PLAY_NEXT);
                        let btn_rem = gtk4::Button::with_label(crate::strings::REMOVE);
                        let pop_close = qp.clone();
                        let tp = tx_pop.clone();
                        let ip = ipos;
                        let pc = pop_close.clone();
                        btn_play.connect_clicked(move |_| {
                            let _ = tp.send(MpdCommand::PlayPosition(ip));
                            if let Some(ref p) = *pc.borrow() {
                                p.popdown();
                            }
                        });
                        let tn = tx_pop.clone();
                        let id_next = iid;
                        let target = current_song.map(|p| p + 1).unwrap_or(0);
                        let pc = pop_close.clone();
                        btn_next.connect_clicked(move |_| {
                            let _ = tn.send(MpdCommand::MoveId(id_next, target));
                            if let Some(ref p) = *pc.borrow() {
                                p.popdown();
                            }
                        });
                        let tr = tx_pop.clone();
                        let pc = pop_close.clone();
                        btn_rem.connect_clicked(move |_| {
                            let _ = tr.send(MpdCommand::DeleteId(iid));
                            if let Some(ref p) = *pc.borrow() {
                                p.popdown();
                            }
                        });
                        popbox.append(&btn_play);
                        popbox.append(&btn_next);
                        popbox.append(&btn_rem);
                        pop.set_child(Some(&popbox));
                        pop.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
                            x as i32, y as i32, 1, 1,
                        )));
                        pop.set_parent(
                            row_for_pop.upcast_ref::<gtk4::Widget>(),
                        );
                        pop.popup();
                        *qp.borrow_mut() = Some(pop);
                    });
                    row.add_controller(rclick);
                    handles.queue_list.append(&row);
                }
                // Select the current track's row in the list
                if let Some(ref cr) = current_row {
                    handles.queue_list.select_row(Some(cr));
                }
                // Refresh shared item_ids for key-based delete/move lookup
                *handles.item_ids.borrow_mut() = item_ids;

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
                *handles.mini_current.borrow_mut() = found_current;
                let mini_items: Vec<MiniGridItem> = seen
                    .into_iter()
                    .map(|(album, artist, first_pos)| MiniGridItem {
                        album,
                        artist,
                        first_pos,
                    })
                    .collect();
                *handles.mini_data.borrow_mut() = mini_items;

                // Rebuild Fixed layout with new items
                let current = handles.mini_current.borrow().clone();
                let raw_vpw = handles.mini_scroll.width();
                let vpw = if raw_vpw > 0 { raw_vpw } else { mini_vpw_est };
                let cells = rebuild_mini_fixed(
                    &handles.mini_fixed,
                    &handles.mini_data.borrow(),
                    &handles.cover_paths,
                    &handles.mini_cover_widgets,
                    &current,
                    &handles.cmd_tx,
                    vpw,
                );
                *handles.mini_cells.borrow_mut() = cells;
            }
            MpdEvent::CoverPaths(paths) => {
                let mut cp = handles.cover_paths.borrow_mut();
                let mini_widgets = handles.mini_cover_widgets.borrow();
                let cells = handles.album_cells.borrow();
                for (album, path) in paths {
                    cp.insert(album.clone(), path.clone());
                    if let Some(p) = path.as_deref() {
                        // Update grid cell
                        if let Some(cell) =
                            cells.iter().find(|c| c.album_id == *album)
                        {
                            cell.cell.set_cover_filename(p);
                        }
                        // Update mini queue grid
                        if let Some(pic) = mini_widgets.get(album.as_str()) {
                            pic.set_filename(Some(p));
                            pic.set_visible(true);
                        }
                        // Update now-playing cover
                        let is_current = handles
                            .current_album
                            .borrow()
                            .as_deref()
                            .map(|a| album.ends_with(&format!("||{}", a)))
                            .unwrap_or(false);
                        if is_current {
                            handles.np_cover.set_filename(Some(p));
                            handles.np_cover.set_visible(true);
                        }
                    }
                }
            }
            MpdEvent::CoverRefreshed { album_id, data } => {
                let data_len = data.len();
                log::info!(
                    "[UI] cover refreshed: '{album_id}' ({} bytes RGBA)",
                    data_len
                );
                let rgba = glib::Bytes::from_owned(data);
                let texture = gdk4::MemoryTexture::new(
                    200,
                    200,
                    gdk4::MemoryFormat::R8g8b8a8,
                    &rgba,
                    200 * 4,
                );
                handles
                    .texture_cache
                    .borrow_mut()
                    .insert(album_id.clone(), texture.clone().into());
                log::info!(
                    "[UI] cover refresh: '{album_id}' texture from {data_len} RGBA bytes"
                );
                let cells = handles.album_cells.borrow();
                if let Some(cell) =
                    cells.iter().find(|c| c.album_id == album_id)
                {
                    cell.cell.set_cover_texture(&texture);
                }
                drop(cells);
                if let Some(pic) =
                    handles.mini_cover_widgets.borrow().get(&album_id)
                {
                    pic.set_paintable(Some(&texture));
                    pic.set_visible(true);
                    pic.queue_draw();
                }
                let is_current = handles
                    .current_album
                    .borrow()
                    .as_deref()
                    .map(|a| album_id.ends_with(&format!("||{}", a)))
                    .unwrap_or(false);
                if is_current {
                    handles.np_cover.set_paintable(Some(&texture));
                    handles.np_cover.set_visible(true);
                }
            }
            MpdEvent::LibraryChanged => {
                handles.search_cmd_tx.send(SearchCommand::Reset);
                let _ = handles.cmd_tx.send(MpdCommand::ListAlbumsGrouped("Albums".into()));
            }
            MpdEvent::Error(msg) => {
                handles
                    .toast_overlay
                    .add_toast(adw::Toast::new(&crate::strings::mpd_error(&msg)));
            }
            MpdEvent::Toast { message, level } => {
                let timeout = level.default_timeout_seconds();
                let toast = adw::Toast::new(&message);
                toast.set_timeout(timeout);
                handles.toast_overlay.add_toast(toast);
            }
        }
        if let Some(ev) = router_event {
            let _ = toast_tx.try_send(ev);
        }
        guard = match event_rx.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                log::error!("MPD event receiver mutex poisoned: {poisoned}");
                handles.shutdown.quit();
                return glib::ControlFlow::Break;
            }
        };
    }
    if let Err(mpsc::TryRecvError::Disconnected) = guard.try_recv() {
        return glib::ControlFlow::Break;
    }
    glib::ControlFlow::Continue
}
