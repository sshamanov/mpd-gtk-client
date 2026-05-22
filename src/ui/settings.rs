//! Settings dialog — connection parameters, layout profile, theme, cache management.

use crate::config::Config;
use crate::mpd::state_machine::{CommandSender, MpdCommand};
use crate::mpd::ConnectionTarget;
use gtk4::prelude::*;
use gtk4::{Box, Button, CheckButton, DropDown, Entry, Label, Orientation, SpinButton};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Show the settings modal dialog.
pub(crate) fn show(
    parent: &impl gtk4::prelude::IsA<gtk4::Window>,
    conn_params: &Arc<Mutex<ConnectionTarget>>,
    cmd_tx: &CommandSender,
    rail_width_min: &Rc<Cell<u32>>,
    rail_width_max: &Rc<Cell<u32>>,
    split_ratio: &Rc<Cell<f64>>,
) {
    let scfg = Config::load();
    let d = gtk4::Window::new();
    d.set_title(Some(crate::strings::SETTINGS_TITLE));
    d.set_transient_for(Some(parent));
    d.set_modal(true);
    let content = Box::new(Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    let host_entry = Entry::new();
    host_entry.set_text(&scfg.mpd_host);
    let port_entry = Entry::new();
    port_entry.set_text(&scfg.mpd_port.to_string());
    content.append(&Label::new(Some(crate::strings::SETTINGS_MPD_HOST)));
    content.append(&host_entry);
    content.append(&Label::new(Some(crate::strings::SETTINGS_MPD_PORT)));
    content.append(&port_entry);
    let port_error = Label::new(Some(crate::strings::SETTINGS_INVALID_PORT));
    port_error.set_css_classes(&["error-label"]);
    port_error.set_visible(false);
    content.append(&port_error);

    // Profile selector (hidden when ≤1 profile)
    let profile_names: Vec<String> = scfg
        .profiles
        .as_ref()
        .map_or_else(Vec::new, |p| {
            let mut names: Vec<String> = p.keys().cloned().collect();
            names.sort();
            names
        });
    let profile_dropdown = DropDown::from_strings(
        &profile_names
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>(),
    );
    profile_dropdown.set_visible(profile_names.len() > 1);
    if profile_names.len() > 1 {
        content.append(&Label::new(Some(crate::strings::SETTINGS_PROFILE)));
        content.append(&profile_dropdown);
    }
    // Pre-select current profile
    if let Some(ref cur) = scfg.default_profile.or(scfg.last_profile) {
        if let Some(pos) = profile_names.iter().position(|n| n == cur) {
            profile_dropdown.set_selected(pos as u32);
        }
    }

    // High contrast toggle
    let hc_check = CheckButton::with_label(crate::strings::SETTINGS_HIGH_CONTRAST);
    hc_check.set_active(scfg.high_contrast);
    hc_check.set_margin_top(8);
    content.append(&hc_check);

    let auto_start_check = CheckButton::with_label(crate::strings::SETTINGS_AUTO_START);
    auto_start_check.set_active(scfg.auto_start);
    auto_start_check.set_margin_top(4);
    content.append(&auto_start_check);

    let artist_check = CheckButton::with_label(crate::strings::SETTINGS_GROUP_ARTIST);
    artist_check.set_active(scfg.use_album_artist);
    artist_check.set_margin_top(4);
    artist_check.set_tooltip_text(Some(crate::strings::SETTINGS_GROUP_ARTIST_TOOLTIP));
    content.append(&artist_check);

    // Network cap section
    let cap_label = Label::new(Some(crate::strings::SETTINGS_COVER_CAP));
    cap_label.set_halign(gtk4::Align::Start);
    cap_label.set_margin_top(12);
    content.append(&cap_label);
    let cap_spin = SpinButton::new(
        Some(&gtk4::Adjustment::new(
            scfg.cover_cache.monthly_data_cap_mb as f64,
            0.0,
            10000.0,
            100.0,
            500.0,
            0.0,
        )),
        1.0,
        0,
    );
    cap_spin.set_margin_top(2);
    content.append(&cap_spin);

    // Current month-to-date usage
    let usage_mb = scfg.cover_cache.monthly_bytes_downloaded as f64 / (1024.0 * 1024.0);
    let usage_text = crate::strings::month_usage_mb(usage_mb);
    let usage_label = Label::new(Some(&usage_text));
    usage_label.set_halign(gtk4::Align::Start);
    usage_label.set_margin_top(2);
    content.append(&usage_label);

    // Memory warning threshold
    let mem_label = Label::new(Some(crate::strings::SETTINGS_MEMORY_LABEL));
    mem_label.set_halign(gtk4::Align::Start);
    mem_label.set_margin_top(12);
    content.append(&mem_label);
    let mem_spin = SpinButton::new(
        Some(&gtk4::Adjustment::new(
            scfg.memory.warning_threshold_mb as f64,
            100.0,
            4000.0,
            100.0,
            500.0,
            0.0,
        )),
        1.0,
        0,
    );
    mem_spin.set_margin_top(2);
    content.append(&mem_spin);

    // Layout profile export/import
    let layout_label = Label::new(Some(crate::strings::SETTINGS_LAYOUT_LABEL));
    layout_label.set_halign(gtk4::Align::Start);
    layout_label.set_margin_top(12);
    content.append(&layout_label);
    let layout_btn_box = Box::new(Orientation::Horizontal, 8);
    layout_btn_box.set_margin_top(4);
    let export_btn = Button::with_label(crate::strings::SETTINGS_EXPORT_BTN);
    let import_btn = Button::with_label(crate::strings::SETTINGS_IMPORT_BTN);
    let layout_status = Label::new(None);
    layout_status.set_halign(gtk4::Align::Start);
    layout_status.set_margin_top(2);
    layout_btn_box.append(&export_btn);
    layout_btn_box.append(&import_btn);
    content.append(&layout_btn_box);
    content.append(&layout_status);

    let dw_layout = d.clone();
    let dw_layout2 = dw_layout.clone();
    let ls_export = layout_status.clone();
    export_btn.connect_clicked(move |_| {
        let cfg = Config::load();
        let json = match cfg.export_layout_profile() {
            Ok(j) => j,
            Err(e) => {
                ls_export.set_text(&format!("{}{e}", crate::strings::EXPORT_ERROR_PREFIX));
                return;
            }
        };
        let dialog = gtk4::FileDialog::new();
        dialog.set_title(crate::strings::SETTINGS_EXPORT_TITLE);
        dialog.set_accept_label(Some(crate::strings::SETTINGS_SAVE));
        let filters = {
            let filter = gtk4::FileFilter::new();
            filter.add_pattern("*.json");
            filter.set_name(Some(crate::strings::SETTINGS_JSON_FILTER));
            let store = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
            store.append(&filter);
            store
        };
        dialog.set_filters(Some(&filters));
        let dir = gtk4::gio::File::for_path(
            dirs::document_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
        );
        dialog.set_initial_folder(Some(&dir));
        let ls = ls_export.clone();
        dialog.save(
            Some(&dw_layout),
            gtk4::gio::Cancellable::NONE,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        if let Err(e) = std::fs::write(&path, &json) {
                            ls.set_text(&format!("{}{e}", crate::strings::EXPORT_FAILED_PREFIX));
                        } else {
                            ls.set_text(crate::strings::EXPORT_SUCCESS);
                        }
                    }
                }
            },
        );
    });

    let ls_import = layout_status.clone();
    let rwmin = rail_width_min.clone();
    let rwmax = rail_width_max.clone();
    let sratio = split_ratio.clone();
    import_btn.connect_clicked(move |_| {
        let dialog = gtk4::FileDialog::new();
        dialog.set_title(crate::strings::SETTINGS_IMPORT_TITLE);
        dialog.set_accept_label(Some(crate::strings::SETTINGS_OPEN));
        let filters = {
            let filter = gtk4::FileFilter::new();
            filter.add_pattern("*.json");
            filter.set_name(Some(crate::strings::SETTINGS_JSON_FILTER));
            let store = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
            store.append(&filter);
            store
        };
        dialog.set_filters(Some(&filters));
        let dir = gtk4::gio::File::for_path(
            dirs::document_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
        );
        dialog.set_initial_folder(Some(&dir));
        let ls = ls_import.clone();
        let sw = dw_layout2.clone();
        let irwmin = rwmin.clone();
        let irwmax = rwmax.clone();
        let isratio = sratio.clone();
        dialog.open(
            Some(&sw),
            gtk4::gio::Cancellable::NONE,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        match std::fs::read_to_string(&path) {
                            Ok(json) => {
                                let mut cfg = Config::load();
                                match cfg.import_layout_profile(&json) {
                                    Ok(()) => {
                                        if let Some(ref lp) = cfg.layout_profile {
                                            irwmin.set(lp.layout.rail_width_min);
                                            irwmax.set(lp.layout.rail_width_max);
                                            isratio.set(lp.layout.split_ratio);
                                        }
                                        ls.set_text(crate::strings::IMPORT_SUCCESS);
                                    }
                                    Err(e) => {
                                        ls.set_text(&format!(
                                            "{}{e}",
                                            crate::strings::IMPORT_FAILED_PREFIX
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                ls.set_text(&format!(
                                    "{}{e}",
                                    crate::strings::IMPORT_READ_ERROR_PREFIX
                                ));
                            }
                        }
                    }
                }
            },
        );
    });

    let btn_box = Box::new(Orientation::Horizontal, 8);
    btn_box.set_margin_top(8);
    let save_btn = Button::with_label(crate::strings::SETTINGS_SAVE);
    let cancel_btn = Button::with_label(crate::strings::SETTINGS_CANCEL);
    let dw = d.clone();
    let he = host_entry.clone();
    let pe = port_entry.clone();
    let perr = port_error.clone();
    let cp = conn_params.clone();
    let tx = cmd_tx.clone();
    let pd_profile = profile_dropdown.clone();
    let pd_names = profile_names.clone();
    let hc_checkbox = hc_check.clone();
    let auto_start_box = auto_start_check.clone();
    let artist_checkbox = artist_check.clone();
    let cap_spin_clone = cap_spin.clone();
    let mem_spin_clone = mem_spin.clone();
    save_btn.connect_clicked(move |_| {
        let mut c = Config::load();
        c.high_contrast = hc_checkbox.is_active();
        c.use_album_artist = artist_checkbox.is_active();
        c.cover_cache.monthly_data_cap_mb = cap_spin_clone.value() as u64;
        c.memory.warning_threshold_mb = mem_spin_clone.value() as u64;
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
                        if let Ok(mut params) = cp.lock() {
                            *params = if host.starts_with('/') || host.starts_with('~') {
                                ConnectionTarget::Unix(host)
                            } else {
                                ConnectionTarget::Tcp(host, profile.port)
                            };
                        }
                    }
                }
            }
        } else {
            c.mpd_host = he.text().to_string();
            match pe.text().parse::<u16>() {
                Ok(p) if p > 0 => {
                    c.mpd_port = p;
                    perr.set_visible(false);
                }
                _ => {
                    perr.set_text(crate::strings::SETTINGS_INVALID_PORT_RANGE);
                    perr.set_visible(true);
                    return;
                }
            }
            if let Ok(mut params) = cp.lock() {
                let host = c.mpd_host.trim().to_string();
                *params = if host.starts_with('/') || host.starts_with('~') {
                    ConnectionTarget::Unix(host)
                } else if host.is_empty() || host == "auto" {
                    ConnectionTarget::Auto
                } else {
                    ConnectionTarget::Tcp(host, c.mpd_port)
                };
            }
        }

        let _ = c.save();
        let _ = tx.send(MpdCommand::Reconnect);
        dw.close();
    });
    let dw2 = d.clone();
    cancel_btn.connect_clicked(move |_| {
        dw2.close();
    });
    btn_box.append(&save_btn);
    btn_box.append(&cancel_btn);
    content.append(&btn_box);
    d.set_child(Some(&content));
    d.present();
}
