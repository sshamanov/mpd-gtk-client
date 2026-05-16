//! Folder browser — expandable tree with inline directory expansion.
//! Directories expand/collapse in-place. Albums are normalized (CUE/DSD).
//! Thread: UI (GTK main loop).

use crate::mpd::DirEntry;
use crate::mpd::state_machine::{CommandSender, MpdCommand};
use gtk4::prelude::*;
use gtk4::{Box, EventControllerKey, Image, Label, ListBox, Orientation, ScrolledWindow};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

pub struct FolderBrowser {
    pub container: Box,
    list: ListBox,
    breadcrumb: Box,
    pub shared_path: Rc<RefCell<String>>,
    #[allow(dead_code)]
    cmd_tx: CommandSender,
    /// Loaded directory contents: full MPD path → entries
    dir_cache: Rc<RefCell<HashMap<String, Vec<DirEntry>>>>,
    /// Which directory paths are currently expanded
    expanded: Rc<RefCell<HashSet<String>>>,
    /// CUE-associated audio file URIs per directory
    cue_tracks: Rc<RefCell<HashMap<String, Vec<String>>>>,
    /// DSD file URIs per directory (for grouped playback)
    dsd_tracks: Rc<RefCell<HashMap<String, Vec<String>>>>,
    /// Whether the root has been loaded
    root_loaded: Rc<RefCell<bool>>,
}

impl FolderBrowser {
    pub fn new(cmd_tx: CommandSender) -> Self {
        let container = Box::new(Orientation::Vertical, 0);
        container.set_vexpand(true);
        container.set_hexpand(true);

        let breadcrumb = Box::new(Orientation::Horizontal, 0);
        breadcrumb.set_margin_start(8);
        breadcrumb.set_margin_bottom(4);
        breadcrumb.set_widget_name("folder-breadcrumb");
        container.append(&breadcrumb);

        let list = ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::Single);
        let scroll = ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_has_frame(true);
        scroll.set_child(Some(&list));
        container.append(&scroll);

        let shared_path: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let dir_cache: Rc<RefCell<HashMap<String, Vec<DirEntry>>>> = Rc::new(RefCell::new(HashMap::new()));
        let expanded: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
        let cue_tracks: Rc<RefCell<HashMap<String, Vec<String>>>> = Rc::new(RefCell::new(HashMap::new()));
        let dsd_tracks: Rc<RefCell<HashMap<String, Vec<String>>>> = Rc::new(RefCell::new(HashMap::new()));
        let root_loaded: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

        // Row activation: expand/collapse directories, play files/albums
        let tx = cmd_tx.clone();
        let sp = shared_path.clone();
        let ct = cue_tracks.clone();
        let dt = dsd_tracks.clone();
        let dc = dir_cache.clone();
        let ex = expanded.clone();
        let list_ref = list.clone();
        list.connect_row_activated(move |_list, row| {
            if let Some(w) = row.child() {
                let name = w.widget_name();
                if name.starts_with("file:") {
                    let filepath = name.strip_prefix("file:").unwrap_or("");
                    let _ = tx.send(MpdCommand::PlayFile(filepath.to_string()));
                } else if name.starts_with("cue:") {
                    let path = name.strip_prefix("cue:").unwrap_or("");
                    if let Some(uris) = ct.borrow().get(path) {
                        let _ = tx.send(MpdCommand::PlayUris(uris.clone()));
                    }
                } else if name.starts_with("dsd:") {
                    let path = name.strip_prefix("dsd:").unwrap_or("");
                    if let Some(uris) = dt.borrow().get(path) {
                        let _ = tx.send(MpdCommand::PlayUris(uris.clone()));
                    }
                } else if name.starts_with("dir:") {
                    let dir_path = name.strip_prefix("dir:").unwrap_or("");
                    let dir_path = dir_path.to_string();
                    // Toggle expand/collapse
                    if ex.borrow().contains(&dir_path) {
                        // Collapse
                        ex.borrow_mut().remove(&dir_path);
                        rebuild_list(&list_ref, &dc, &ex, &ct, &dt, &sp);
                    } else if dc.borrow().contains_key(&dir_path) {
                        // Expand from cache
                        ex.borrow_mut().insert(dir_path.clone());
                        rebuild_list(&list_ref, &dc, &ex, &ct, &dt, &sp);
                    } else {
                        // Fetch from MPD
                        let _ = tx.send(MpdCommand::ListDirectory(dir_path.clone()));
                    }
                } else if name == ".." {
                    let cur = sp.borrow().clone();
                    let parent = Path::new(&cur)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    *sp.borrow_mut() = parent.clone();
                    let _ = tx.send(MpdCommand::ListDirectory(parent));
                }
            }
        });

        // Keyboard: Left = parent
        let tx_kb = cmd_tx.clone();
        let sp_kb = shared_path.clone();
        let key_ctrl = EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_ctrl, key, _code, _mods| {
            if key == gtk4::gdk::Key::Left || key == gtk4::gdk::Key::BackSpace {
                let cur = sp_kb.borrow().clone();
                // At root, parent navigation is meaningless — no-op to avoid sending
                // an empty ListDirectory command.
                if cur.is_empty() || cur == "/" {
                    log::debug!("[folder-tree] BackSpace/Left at root, ignoring");
                    return gtk4::glib::Propagation::Stop;
                }
                let parent = Path::new(&cur)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                *sp_kb.borrow_mut() = parent.clone();
                let _ = tx_kb.send(MpdCommand::ListDirectory(parent));
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        list.add_controller(key_ctrl);

        // Right-click context menu on folder tree rows.
        // set_button(3) + Capture phase avoids gesture arbitration conflicts.
        let folder_popover: Rc<RefCell<Option<gtk4::Popover>>> = Rc::new(RefCell::new(None));
        let rctx_tx = cmd_tx.clone();
        let rctx_list = list.clone();
        let rctx_ct = cue_tracks.clone();
        let rctx_dt = dsd_tracks.clone();
        let rclick_gesture = gtk4::GestureClick::new();
        rclick_gesture.set_button(3);
        rclick_gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let rctx_fp = folder_popover.clone();
        rclick_gesture.connect_pressed(move |_gest, _n, x, y| {
            log::debug!("FolderTree context menu fired, y={y}");
            let list = &rctx_list;
            let row = list.row_at_y(y as i32);
            let Some(row) = row else { return; };
            let Some(w) = row.child() else { return; };
            let name = w.widget_name();
            let tx = rctx_tx.clone();

            if let Some(ref old) = *rctx_fp.borrow() {
                old.popdown();
            }

            let pop = gtk4::Popover::new();
            let popbox = Box::new(Orientation::Vertical, 0);
            let pop_close = rctx_fp.clone();

            if name.starts_with("file:") {
                let filepath = name.strip_prefix("file:").unwrap_or("").to_string();
                let btn_play = gtk4::Button::with_label("Play Now");
                let tp = tx.clone();
                let fp = filepath.clone();
                let pc = pop_close.clone();
                btn_play.connect_clicked(move |_| { let _ = tp.send(MpdCommand::PlayFile(fp.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                let btn_next = gtk4::Button::with_label("Play Next");
                let tn = tx.clone();
                let fnp = filepath.clone();
                let pc = pop_close.clone();
                btn_next.connect_clicked(move |_| { let _ = tn.send(MpdCommand::InsertNextUris(vec![fnp.clone()])); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                let btn_add = gtk4::Button::with_label("Add to Queue");
                let ta = tx.clone();
                let fap = filepath.clone();
                let pc = pop_close.clone();
                btn_add.connect_clicked(move |_| { let _ = ta.send(MpdCommand::AddUris(vec![fap.clone()])); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                popbox.append(&btn_play);
                popbox.append(&btn_next);
                popbox.append(&btn_add);
            } else if name.starts_with("cue:") {
                let path = name.strip_prefix("cue:").unwrap_or("").to_string();
                if let Some(uris) = rctx_ct.borrow().get(&path) {
                    let uris_play = uris.clone();
                    let uris_insert = uris.clone();
                    let uris_add = uris.clone();
                    let btn_play = gtk4::Button::with_label("Play Now");
                    let tp = tx.clone();
                    let pc = pop_close.clone();
                    btn_play.connect_clicked(move |_| { let _ = tp.send(MpdCommand::PlayUris(uris_play.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                    let btn_next = gtk4::Button::with_label("Play Next");
                    let tn = tx.clone();
                    let pc = pop_close.clone();
                    btn_next.connect_clicked(move |_| { let _ = tn.send(MpdCommand::InsertNextUris(uris_insert.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                    let btn_add = gtk4::Button::with_label("Add to Queue");
                    let ta = tx.clone();
                    let pc = pop_close.clone();
                    btn_add.connect_clicked(move |_| { let _ = ta.send(MpdCommand::AddUris(uris_add.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                    popbox.append(&btn_play);
                    popbox.append(&btn_next);
                    popbox.append(&btn_add);
                } else {
                    return;
                }
            } else if name.starts_with("dsd:") {
                let path = name.strip_prefix("dsd:").unwrap_or("").to_string();
                if let Some(uris) = rctx_dt.borrow().get(&path) {
                    let uris_play = uris.clone();
                    let uris_insert = uris.clone();
                    let uris_add = uris.clone();
                    let btn_play = gtk4::Button::with_label("Play Now");
                    let tp = tx.clone();
                    let pc = pop_close.clone();
                    btn_play.connect_clicked(move |_| { let _ = tp.send(MpdCommand::PlayUris(uris_play.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                    let btn_next = gtk4::Button::with_label("Play Next");
                    let tn = tx.clone();
                    let pc = pop_close.clone();
                    btn_next.connect_clicked(move |_| { let _ = tn.send(MpdCommand::InsertNextUris(uris_insert.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                    let btn_add = gtk4::Button::with_label("Add to Queue");
                    let ta = tx.clone();
                    let pc = pop_close.clone();
                    btn_add.connect_clicked(move |_| { let _ = ta.send(MpdCommand::AddUris(uris_add.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                    popbox.append(&btn_play);
                    popbox.append(&btn_next);
                    popbox.append(&btn_add);
                } else {
                    return;
                }
            } else if name.starts_with("dir:") {
                let dirpath = name.strip_prefix("dir:").unwrap_or("").to_string();
                let btn_play = gtk4::Button::with_label("Play Now");
                let tp = tx.clone();
                let dp = dirpath.clone();
                let pc = pop_close.clone();
                btn_play.connect_clicked(move |_| { let _ = tp.send(MpdCommand::PlayDirectory(dp.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                let btn_next = gtk4::Button::with_label("Play Next");
                let tn = tx.clone();
                let dn = dirpath.clone();
                let pc = pop_close.clone();
                btn_next.connect_clicked(move |_| { let _ = tn.send(MpdCommand::InsertNextDirectory(dn.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                let btn_add = gtk4::Button::with_label("Add to Queue");
                let ta = tx.clone();
                let da = dirpath.clone();
                let pc = pop_close.clone();
                btn_add.connect_clicked(move |_| { let _ = ta.send(MpdCommand::AddDirectory(da.clone())); if let Some(ref p) = *pc.borrow() { p.popdown(); } });
                popbox.append(&btn_play);
                popbox.append(&btn_next);
                popbox.append(&btn_add);
            } else {
                return;
            }

            pop.set_child(Some(&popbox));
            pop.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            pop.set_parent(rctx_list.upcast_ref::<gtk4::Widget>());
            pop.popup();
            *rctx_fp.borrow_mut() = Some(pop);
        });
        list.add_controller(rclick_gesture);

        Self {
            container,
            list,
            breadcrumb,
            shared_path,
            cmd_tx,
            dir_cache,
            expanded,
            cue_tracks,
            dsd_tracks,
            root_loaded,
        }
    }

    /// Called when a directory listing arrives from MPD.
    /// If path matches the current root, rebuilds breadcrumb (navigation).
    /// Otherwise just caches and expands in-place (tree expansion).
    pub fn set_entries(&mut self, path: &str, entries: Vec<DirEntry>) {
        let is_root = path == self.shared_path.borrow().as_str()
            || (path.is_empty() && self.shared_path.borrow().is_empty());

        if is_root {
            // Rebuild breadcrumb — this is a root navigation.
            // Remove children one by one — while-let avoids re-entrancy
            // issues (remove() can trigger GTK signals that modify the child
            // list, causing "Tried to remove non-child").
            while let Some(child) = self.breadcrumb.first_child() {
                self.breadcrumb.remove(&child);
            }
            let tx = self.cmd_tx.clone();
            let sp = self.shared_path.clone();
            if path.is_empty() {
                let lbl = Label::new(Some("~"));
                lbl.set_css_classes(&["breadcrumb-root"]);
                let tx_r = tx.clone();
                let sp_r = sp.clone();
                let gesture_r = gtk4::GestureClick::new();
                gesture_r.connect_pressed(move |_g, _n, _x, _y| {
                    *sp_r.borrow_mut() = String::new();
                    let _ = tx_r.send(MpdCommand::ListDirectory(String::new()));
                });
                lbl.add_controller(gesture_r);
                self.breadcrumb.append(&lbl);
            } else {
                let parts: Vec<&str> = path.split('/').collect();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        let sep = Label::new(Some("/"));
                        sep.set_css_classes(&["breadcrumb-sep"]);
                        self.breadcrumb.append(&sep);
                    }
                    let seg_path = parts[..=i].join("/");
                    let lbl = Label::new(Some(part));
                    if i == parts.len() - 1 {
                        lbl.set_css_classes(&["breadcrumb-current"]);
                    } else {
                        lbl.set_css_classes(&["breadcrumb-segment"]);
                        let seg_tx = tx.clone();
                        let sp_seg = sp.clone();
                        let gesture = gtk4::GestureClick::new();
                        gesture.connect_pressed(move |_gest, _n, _x, _y| {
                            *sp_seg.borrow_mut() = seg_path.clone();
                            let _ = seg_tx.send(MpdCommand::ListDirectory(seg_path.clone()));
                        });
                        lbl.add_controller(gesture);
                    }
                    self.breadcrumb.append(&lbl);
                }
            }
        }

        self.add_children(path, entries);
    }

    /// Called when a directory listing arrives from MPD.
    /// Caches the entries, normalizes CUE, and expands the path in the tree.
    fn add_children(&mut self, path: &str, entries: Vec<DirEntry>) {
        // Detect CUE and build normalized view
        let ends_with_ci = |s: &str, ext: &str| -> bool {
            s.len() >= ext.len() && s[s.len() - ext.len()..].to_lowercase() == ext
        };

        let has_cue = entries
            .iter()
            .any(|e| matches!(e, DirEntry::File { name, .. } if ends_with_ci(name, ".cue")));

        // Collect CUE-associated track URIs
        if has_cue {
            let mut uris: Vec<String> = Vec::new();
            for entry in &entries {
                if let DirEntry::File { path: fp, name, .. } = entry {
                    if ends_with_ci(name, ".flac")
                        || ends_with_ci(name, ".wav") || ends_with_ci(name, ".ape")
                        || ends_with_ci(name, ".ogg") || ends_with_ci(name, ".mp3")
                        || ends_with_ci(name, ".m4a") || ends_with_ci(name, ".aiff")
                        || ends_with_ci(name, ".aif") || ends_with_ci(name, ".wv")
                        || ends_with_ci(name, ".wma") || ends_with_ci(name, ".opus")
                        || ends_with_ci(name, ".aac")
                    {
                        uris.push(fp.clone());
                    }
                }
            }
            self.cue_tracks.borrow_mut().insert(path.to_string(), uris);
        }

        // Collect DSD file URIs for grouped playback
        let has_dsd = entries
            .iter()
            .any(|e| matches!(e, DirEntry::File { name, .. } if ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff")));
        if has_dsd {
            let mut uris: Vec<String> = Vec::new();
            for entry in &entries {
                if let DirEntry::File { path: fp, name, .. } = entry {
                    if ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff") {
                        uris.push(fp.clone());
                    }
                }
            }
            self.dsd_tracks.borrow_mut().insert(path.to_string(), uris);
        }

        self.dir_cache.borrow_mut().insert(path.to_string(), entries);
        self.expanded.borrow_mut().insert(path.to_string());
        self.root_loaded.replace(true);

        rebuild_list(
            &self.list,
            &self.dir_cache,
            &self.expanded,
            &self.cue_tracks,
            &self.dsd_tracks,
            &self.shared_path,
        );
    }

    /// Rebuild the list from cache (used after collapse).
    pub fn rebuild(&mut self) {
        rebuild_list(
            &self.list,
            &self.dir_cache,
            &self.expanded,
            &self.cue_tracks,
            &self.dsd_tracks,
            &self.shared_path,
        );
    }
}

/// Recursively rebuild the ListBox from cached directory data.
fn rebuild_list(
    list: &ListBox,
    dir_cache: &Rc<RefCell<HashMap<String, Vec<DirEntry>>>>,
    expanded: &Rc<RefCell<HashSet<String>>>,
    cue_tracks: &Rc<RefCell<HashMap<String, Vec<String>>>>,
    dsd_tracks: &Rc<RefCell<HashMap<String, Vec<String>>>>,
    shared_path: &Rc<RefCell<String>>,
) {
    // Clear all rows. Use remove_all() which handles ListBox internal
    // bookkeeping correctly. The per-row remove() loop can infinite-loop
    // if a child's parent isn't the ListBox ("Tried to remove non-child").
    // remove_all() is safe because it only removes actual rows.
    list.remove_all();

    let root_path = shared_path.borrow().clone();

    // Breadcrumb is managed by set_entries / the caller rebuilding it separately.
    // Here we just rebuild the tree starting from root_path.

    let has_children = dir_cache.borrow().contains_key(&root_path);
    if !has_children {
        // Not loaded yet — show empty
        return;
    }

    // Build tree from root_path at depth 0
    render_dir(
        list,
        &root_path,
        0,
        dir_cache,
        expanded,
        cue_tracks,
        dsd_tracks,
    );

    // Show ".." parent entry if not at root
    if !root_path.is_empty() {
        let parent_row = dir_row("..", "..", 0);
        list.prepend(&parent_row);
    }
}

fn render_dir(
    list: &ListBox,
    path: &str,
    depth: u32,
    dir_cache: &Rc<RefCell<HashMap<String, Vec<DirEntry>>>>,
    expanded: &Rc<RefCell<HashSet<String>>>,
    cue_tracks: &Rc<RefCell<HashMap<String, Vec<String>>>>,
    dsd_tracks: &Rc<RefCell<HashMap<String, Vec<String>>>>,
) {
    let entries = match dir_cache.borrow().get(path) {
        Some(e) => e.clone(),
        None => return,
    };

    let ends_with_ci = |s: &str, ext: &str| -> bool {
        s.len() >= ext.len() && s[s.len() - ext.len()..].to_lowercase() == ext
    };

    let has_cue = entries
        .iter()
        .any(|e| matches!(e, DirEntry::File { name, .. } if ends_with_ci(name, ".cue")));
    let has_dsd = entries
        .iter()
        .any(|e| matches!(e, DirEntry::File { name, .. } if ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff")));

    let mut has_visible = false;

    for entry in &entries {
        match entry {
            DirEntry::Directory { name, .. } => {
                has_visible = true;
                let full_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", path, name)
                };
                let is_expanded = expanded.borrow().contains(&full_path);
                let row = tree_dir_row(name, &full_path, depth, is_expanded);
                list.append(&row);

                // If expanded, render children recursively
                if is_expanded {
                    render_dir(
                        list,
                        &full_path,
                        depth + 1,
                        dir_cache,
                        expanded,
                        cue_tracks,
                        dsd_tracks,
                    );
                }
            }
            DirEntry::File {
                path: fp,
                name,
                duration,
                format,
                audio,
                ..
            } => {
                // Skip CUE files and audio files in CUE folders
                if has_cue && ends_with_ci(name, ".cue") {
                    continue;
                }
                if has_cue
                    && (ends_with_ci(name, ".flac")
                        || ends_with_ci(name, ".wav")
                        || ends_with_ci(name, ".ape")
                        || ends_with_ci(name, ".ogg")
                        || ends_with_ci(name, ".mp3")
                        || ends_with_ci(name, ".m4a")
                        || ends_with_ci(name, ".aiff")
                        || ends_with_ci(name, ".aif")
                        || ends_with_ci(name, ".wv")
                        || ends_with_ci(name, ".wma")
                        || ends_with_ci(name, ".opus")
                        || ends_with_ci(name, ".aac"))
                {
                    continue;
                }
                // DSD files are grouped into a summary row — skip individual rows
                if has_dsd && (ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff")) {
                    continue;
                }
                has_visible = true;
                let badge = format_badge(format, audio);
                let dur = duration.map(format_duration).unwrap_or_default();
                let row = file_row(fp, name, &dur, &badge, depth);
                list.append(&row);
            }
            DirEntry::Playlist { name, .. } => {
                has_visible = true;
                let full_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", path, name)
                };
                let row = tree_dir_row(name, &full_path, depth, false);
                list.append(&row);
            }
        }
    }

    // Normalized CUE summary row with track count tooltip
    if has_cue {
        let track_count = cue_tracks.borrow().get(path).map(|v| v.len()).unwrap_or(0);
        let row = cue_summary_row(path, depth, track_count);
        list.append(&row);
        has_visible = true;
    }

    // Normalized DSD summary row
    if has_dsd {
        let file_count = dsd_tracks.borrow().get(path).map(|v| v.len()).unwrap_or(0);
        let row = dsd_summary_row(path, depth, file_count);
        list.append(&row);
        has_visible = true;
    }

    // Empty state
    if !has_visible && !has_cue && !has_dsd {
        let empty_row = gtk4::ListBoxRow::new();
        let lbl = Label::new(Some("(empty directory)"));
        lbl.set_css_classes(&["album-grid-status"]);
        lbl.set_margin_start((depth * 20 + 8) as i32);
        empty_row.set_child(Some(&lbl));
        list.append(&empty_row);
    }
}

// --- Row builders ---

fn tree_dir_row(name: &str, full_path: &str, depth: u32, expanded: bool) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let hbox = Box::new(Orientation::Horizontal, 4);
    let indent = (depth * 20 + 8) as i32;
    hbox.set_margin_start(indent);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);
    hbox.set_widget_name(&format!("dir:{}", full_path));
    hbox.set_css_classes(&["dir-entry"]);

    // Expander triangle — changes direction based on expand state
    let expander = Label::new(Some(if expanded { "▼" } else { "▶" }));
    expander.set_css_classes(&["tree-expander"]);
    hbox.append(&expander);

    let icon = Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(16);
    icon.set_css_classes(&["dir-icon"]);
    hbox.append(&icon);

    let text = Label::new(Some(name));
    text.set_halign(gtk4::Align::Start);
    text.set_hexpand(true);
    hbox.append(&text);

    row.set_child(Some(&hbox));
    row
}

fn dir_row(name: &str, full_path: &str, depth: u32) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let indent = (depth * 20 + 8) as i32;
    let hbox = Box::new(Orientation::Horizontal, 4);
    hbox.set_margin_start(indent);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);
    hbox.set_widget_name(full_path); // ".." or simple name
    hbox.set_css_classes(&["dir-entry"]);

    let icon = Image::from_icon_name("go-up-symbolic");
    icon.set_pixel_size(16);
    icon.set_css_classes(&["dir-icon"]);
    hbox.append(&icon);

    let text = Label::new(Some(name));
    text.set_halign(gtk4::Align::Start);
    text.set_hexpand(true);
    hbox.append(&text);

    row.set_child(Some(&hbox));
    row
}

fn file_row(
    full_path: &str,
    name: &str,
    duration: &str,
    format_badge: &str,
    depth: u32,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let indent = (depth * 20 + 12) as i32;
    let hbox = Box::new(Orientation::Horizontal, 8);
    hbox.set_margin_start(indent);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);
    hbox.set_widget_name(&format!("file:{}", full_path));
    hbox.set_css_classes(&["folder-file-row"]);

    let icon = Image::from_icon_name("audio-x-generic-symbolic");
    icon.set_pixel_size(16);
    icon.set_css_classes(&["file-icon"]);
    hbox.append(&icon);

    let name_label = Label::new(Some(name));
    name_label.set_halign(gtk4::Align::Start);
    name_label.set_hexpand(true);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&name_label);

    let badge = if format_badge.is_empty() {
        Label::new(Some(duration))
    } else {
        let lbl = Label::new(Some(&format!("{}  {}", format_badge, duration)));
        lbl.set_css_classes(&["format-badge"]);
        lbl
    };
    badge.set_halign(gtk4::Align::End);
    hbox.append(&badge);
    row.set_child(Some(&hbox));
    row
}

fn cue_summary_row(path: &str, depth: u32, track_count: usize) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let indent = (depth * 20 + 8) as i32;
    let hbox = Box::new(Orientation::Horizontal, 4);
    hbox.set_margin_start(indent);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);

    let icon = Image::from_icon_name("media-optical-cd-audio-symbolic");
    icon.set_pixel_size(16);
    hbox.append(&icon);

    let lbl = Label::new(Some("Cue Sheet Album"));
    lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&lbl);

    let badge = Label::new(Some("CUE"));
    badge.set_css_classes(&["normalized-badge"]);
    hbox.append(&badge);

    hbox.set_css_classes(&["dir-entry"]);
    hbox.set_widget_name(&format!("cue:{}", path));
    let tooltip = format!("Cue sheet — {} track{}", track_count, if track_count == 1 { "" } else { "s" });
    row.set_tooltip_text(Some(&tooltip));
    row.set_child(Some(&hbox));
    row
}

fn dsd_summary_row(path: &str, depth: u32, file_count: usize) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let indent = (depth * 20 + 8) as i32;
    let hbox = Box::new(Orientation::Horizontal, 4);
    hbox.set_margin_start(indent);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);

    let icon = Image::from_icon_name("audio-x-generic-symbolic");
    icon.set_pixel_size(16);
    hbox.append(&icon);

    let lbl = Label::new(Some("DSD Album"));
    lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&lbl);

    let badge = Label::new(Some("DSD"));
    badge.set_css_classes(&["normalized-badge"]);
    hbox.append(&badge);

    hbox.set_css_classes(&["dir-entry"]);
    hbox.set_widget_name(&format!("dsd:{}", path));
    let tooltip = format!("DSD folder — {} file{}", file_count, if file_count == 1 { "" } else { "s" });
    row.set_tooltip_text(Some(&tooltip));
    row.set_child(Some(&hbox));
    row
}

// --- Format helpers ---

fn format_duration(secs: f64) -> String {
    if secs < 0.0 {
        return "0:00".into();
    }
    let total = secs as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn format_badge(format: &Option<String>, audio: &Option<String>) -> String {
    match (audio, format) {
        (Some(a), f) if a.to_lowercase() == "dsd" => {
            let rate = f
                .as_ref()
                .and_then(|s| s.split(':').next())
                .and_then(|r| r.parse::<u32>().ok())
                .unwrap_or(0);
            let mult = rate / 44100;
            if mult >= 2 {
                format!("DSD{}", mult)
            } else {
                "DSD".into()
            }
        }
        (_, Some(f)) => {
            let p: Vec<&str> = f.split(':').collect();
            if p.len() >= 2 {
                let sr = p[0].parse::<f64>().unwrap_or(0.0) / 1000.0;
                let sr_s = if sr.fract() == 0.0 {
                    format!("{}", sr as u32)
                } else {
                    format!("{:.1}", sr)
                };
                format!("{}/{}", p[1], sr_s)
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}
