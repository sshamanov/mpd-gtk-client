//! Now-playing display — view model, widgets, and update logic.

use crate::mpd::state_machine::PlaybackUpdate;
use gtk4::prelude::*;
use gtk4::{Box, Label, Picture};

pub(crate) struct NowPlayingWidgets<'a> {
    pub title: &'a gtk4::Button,
    pub artist: &'a Label,
    pub album: &'a Label,
    pub year: &'a Label,
    pub status_dot: &'a Box,
    pub play_pause_btn: &'a gtk4::Button,
    pub pos_label: &'a Label,
    pub len_label: &'a Label,
    pub seekbar: &'a gtk4::Scale,
    pub fmt_label: &'a Label,
    pub bitrate_label: &'a Label,
    pub cover: &'a Picture,
    pub cover_stack: &'a gtk4::Stack,
    pub cover_paths: &'a std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Option<String>>>>,
}

/// View model for now-playing display — derived from PlaybackUpdate.
#[allow(dead_code)]
pub(crate) struct PlaybackDisplay {
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
    pub(crate) fn from_update(update: &PlaybackUpdate) -> Self {
        let mut format_badge = update.format.clone();
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

/// Find the first track position of the previous (forward=false) or next (forward=true)
/// album in the queue, relative to the currently playing track's album.
pub(crate) fn find_album_boundary(queue: &[crate::mpd::QueueEntry], current_position: Option<i32>, forward: bool) -> Option<i32> {
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

/// Update now-playing widgets and forward to MPRIS after state has been written by `reduce()`.
pub(crate) fn handle_now_playing(
    update: &PlaybackUpdate,
    w: NowPlayingWidgets,
    mpris_tx: &std::sync::mpsc::Sender<PlaybackUpdate>,
) {
    let display = PlaybackDisplay::from_update(update);
    update_now_playing(w, &display);
    let _ = mpris_tx.send(update.clone());
}

pub(crate) fn update_now_playing(
    w: NowPlayingWidgets,
    display: &PlaybackDisplay,
) {
    let has_track = display.artist.is_some() || display.album.is_some();

    if !display.title.is_empty() {
        w.title.set_label(&display.title);
    } else {
        w.title.set_label(if has_track { "" } else { crate::strings::NO_TRACK_PLAYING });
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
