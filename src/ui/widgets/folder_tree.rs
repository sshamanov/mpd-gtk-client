//! Folder tree — breadcrumb navigation, file playback, format badges, keyboard nav. Thread: UI (GTK main loop).

use crate::mpd::DirEntry;
use crate::mpd::state_machine::MpdCommand;
use gtk4::prelude::*;
use gtk4::{Box, EventControllerKey, Label, ListBox, Orientation, ScrolledWindow};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;

pub struct FolderBrowser {
    pub container: Box,
    list: ListBox,
    breadcrumb: Box,
    pub shared_path: Rc<RefCell<String>>,
    #[allow(dead_code)]
    cmd_tx: mpsc::Sender<MpdCommand>,
}

impl FolderBrowser {
    pub fn new(cmd_tx: mpsc::Sender<MpdCommand>) -> Self {
        let container = Box::new(Orientation::Vertical, 0);

        let breadcrumb = Box::new(Orientation::Horizontal, 0);
        breadcrumb.set_margin_start(8);
        breadcrumb.set_margin_bottom(4);
        breadcrumb.set_widget_name("folder-breadcrumb");
        container.append(&breadcrumb);

        let list = ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::Single);
        let scroll = ScrolledWindow::new();
        scroll.set_child(Some(&list));
        container.append(&scroll);

        let shared_path: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

        // Row activation: navigate directories / play files
        let tx = cmd_tx.clone();
        let sp = shared_path.clone();
        list.connect_row_activated(move |_list, row| {
            if let Some(w) = row.child() {
                let css = w.css_classes();
                let name = w.widget_name();
                // File rows have widget_name starting with "file:"
                if name.starts_with("file:") {
                    let filepath = name.strip_prefix("file:").unwrap_or("");
                    let _ = tx.send(MpdCommand::PlayFile(filepath.to_string()));
                } else if name == ".." {
                    let cur = sp.borrow().clone();
                    let parent = Path::new(&cur).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    let _ = tx.send(MpdCommand::ListDirectory(parent));
                } else if css.contains(&"dir-entry".into()) {
                    let cur = sp.borrow().clone();
                    let n = name.to_string();
                    let new_path = if n.is_empty() { cur } else if cur.is_empty() { n } else { format!("{}/{}", cur, n) };
                    let _ = tx.send(MpdCommand::ListDirectory(new_path));
                }
            }
        });

        // Keyboard: Left=parent
        let tx_kb = cmd_tx.clone();
        let sp_kb = shared_path.clone();
        let key_ctrl = EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_ctrl, key, _code, _mods| {
            if key == gtk4::gdk::Key::Left || key == gtk4::gdk::Key::BackSpace {
                let cur = sp_kb.borrow().clone();
                let parent = Path::new(&cur).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                let _ = tx_kb.send(MpdCommand::ListDirectory(parent));
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        list.add_controller(key_ctrl);

        Self { container, list, breadcrumb, shared_path, cmd_tx }
    }

    pub fn set_entries(&mut self, path: &str, entries: Vec<DirEntry>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        *self.shared_path.borrow_mut() = path.to_string();

        // Rebuild breadcrumb
        while let Some(c) = self.breadcrumb.first_child() {
            self.breadcrumb.remove(&c);
        }
        let tx = self.cmd_tx.clone();
        if path.is_empty() {
            let lbl = Label::new(Some("~"));
            lbl.set_css_classes(&["breadcrumb-root"]);
            let tx_r = tx.clone();
            let gesture_r = gtk4::GestureClick::new();
            gesture_r.connect_pressed(move |_g, _n, _x, _y| {
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
                    let gesture = gtk4::GestureClick::new();
                    gesture.connect_pressed(move |_gest, _n, _x, _y| {
                        let _ = seg_tx.send(MpdCommand::ListDirectory(seg_path.clone()));
                    });
                    lbl.add_controller(gesture);
                }
                self.breadcrumb.append(&lbl);
            }
        }

        // Up-navigation
        if !path.is_empty() {
            self.list.append(&dir_row(".."));
        }

        // Normalization: detect cue/DSD folders (case-insensitive extension matching)
        let ends_with_ci = |s: &str, ext: &str| -> bool {
            s.len() >= ext.len() && s[s.len()-ext.len()..].to_lowercase() == ext
        };
        let has_cue = entries.iter().any(|e| matches!(e, DirEntry::File { name, .. } if ends_with_ci(name, ".cue")));
        let dsd_count = entries.iter().filter(|e| matches!(e, DirEntry::File { name, .. } if ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff"))).count();
        let is_dsd_folder = dsd_count >= 1;
        let dsd_format = entries.iter().find_map(|e| {
            if let DirEntry::File { name, format, audio, .. } = e {
                if ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff") {
                    Some(format_badge(format, audio))
                } else { None }
            } else { None }
        }).unwrap_or_default();
        let mut has_visible = false;

        for entry in entries {
            match entry {
                DirEntry::Directory { name, .. } => {
                    has_visible = true;
                    self.list.append(&dir_row(&name));
                }
                DirEntry::File { path, name, duration, format, audio, .. } => {
                    // Hide .cue files when cue normalization is active
                    if has_cue && ends_with_ci(&name, ".cue") { continue; }
                    // Skip individual audio files grouped under a cue sheet
                    if has_cue && (ends_with_ci(&name, ".flac") || ends_with_ci(&name, ".wav")
                        || ends_with_ci(&name, ".ape") || ends_with_ci(&name, ".ogg")
                        || ends_with_ci(&name, ".mp3") || ends_with_ci(&name, ".m4a")
                        || ends_with_ci(&name, ".aiff") || ends_with_ci(&name, ".aif")
                        || ends_with_ci(&name, ".wv") || ends_with_ci(&name, ".wma")
                        || ends_with_ci(&name, ".opus") || ends_with_ci(&name, ".aac"))
                    {
                        continue;
                    }
                    // Collapse DSD folder into single entry
                    if is_dsd_folder && (ends_with_ci(&name, ".dsf") || ends_with_ci(&name, ".dff")) {
                        continue;
                    }
                    has_visible = true;
                    let badge = format_badge(&format, &audio);
                    let dur = duration.map(format_duration).unwrap_or_default();
                    self.list.append(&file_row(&path, &name, &dur, &badge));
                }
                DirEntry::Playlist { name, .. } => {
                    has_visible = true;
                    self.list.append(&dir_row(&name));
                }
            }
        }

        // Show normalized entries
        if has_cue {
            let cue_row = gtk4::ListBoxRow::new();
            let hbox = Box::new(Orientation::Horizontal, 4);
            let lbl = Label::new(Some("[CUE] Cue Sheet Album"));
            let info = Label::new(Some("click to browse"));
            info.set_css_classes(&["format-badge"]);
            hbox.set_css_classes(&["dir-entry"]);
            hbox.set_widget_name("");
            hbox.append(&lbl);
            hbox.append(&info);
            cue_row.set_child(Some(&hbox));
            self.list.append(&cue_row);
        }
        if is_dsd_folder && !has_cue {
            let dsd_row = gtk4::ListBoxRow::new();
            let hbox = Box::new(Orientation::Horizontal, 4);
            let lbl = Label::new(Some(&format!("[DSD] DSD Album ({} tracks)", dsd_count)));
            let badge = Label::new(Some(&dsd_format));
            badge.set_css_classes(&["format-badge"]);
            hbox.set_css_classes(&["dir-entry"]);
            hbox.set_widget_name("");
            hbox.append(&lbl);
            hbox.append(&badge);
            dsd_row.set_child(Some(&hbox));
            self.list.append(&dsd_row);
        }

        // Empty state
        if !has_visible && !has_cue && !is_dsd_folder {
            let empty_row = gtk4::ListBoxRow::new();
            let lbl = Label::new(Some("(empty directory)"));
            lbl.set_css_classes(&["album-grid-status"]);
            empty_row.set_child(Some(&lbl));
            self.list.append(&empty_row);
        }
    }
}

fn dir_row(name: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let hbox = Box::new(Orientation::Horizontal, 4);
    hbox.set_margin_start(8);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);
    hbox.set_widget_name(name);
    hbox.set_css_classes(&["dir-entry"]);
    let icon = Label::new(Some("▶"));
    let text = Label::new(Some(name));
    text.set_halign(gtk4::Align::Start);
    hbox.append(&icon);
    hbox.append(&text);
    row.set_child(Some(&hbox));
    row
}

fn file_row(full_path: &str, _name: &str, duration: &str, format_badge: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let hbox = Box::new(Orientation::Horizontal, 8);
    hbox.set_margin_start(12);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);
    hbox.set_widget_name(&format!("file:{}", full_path));

    let icon = Label::new(Some("♪"));
    hbox.append(&icon);

    let name_label = Label::new(Some(_name));
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

fn format_duration(secs: f64) -> String {
    if secs < 0.0 { return "0:00".into(); }
    let total = secs as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn format_badge(format: &Option<String>, audio: &Option<String>) -> String {
    match (audio, format) {
        (Some(a), f) if a.to_lowercase() == "dsd" => {
            let rate = f.as_ref().and_then(|s| s.split(':').next())
                .and_then(|r| r.parse::<u32>().ok()).unwrap_or(0);
            let mult = rate / 44100;
            if mult >= 2 { format!("DSD{}", mult) } else { "DSD".into() }
        }
        (_, Some(f)) => {
            let p: Vec<&str> = f.split(':').collect();
            if p.len() >= 2 {
                let sr = p[0].parse::<f64>().unwrap_or(0.0) / 1000.0;
                let sr_s = if sr.fract() == 0.0 { format!("{}", sr as u32) } else { format!("{:.1}", sr) };
                format!("{}/{}", p[1], sr_s)
            } else { String::new() }
        }
        _ => String::new(),
    }
}
