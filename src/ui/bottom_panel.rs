//! Bottom transport bar for narrow mode — track info, transport controls, queue toggle.

use crate::mpd::state_machine::CommandSender;
use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation};

pub(crate) struct BottomPanel {
    pub panel: Box,
    pub title: Label,
    pub play: Button,
    pub queue_btn: Button,
}

/// Build the bottom transport panel for narrow-mode layout.
pub(crate) fn build(cmd_tx: &CommandSender) -> BottomPanel {
    let panel = Box::new(Orientation::Horizontal, 8);
    panel.set_margin_start(8);
    panel.set_margin_end(8);
    panel.set_margin_top(4);
    panel.set_margin_bottom(4);
    panel.set_size_request(-1, 44);
    panel.set_vexpand(false);
    panel.set_css_classes(&["bottom-panel"]);
    panel.set_visible(false);

    let title = Label::new(Some(""));
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.set_max_width_chars(24);
    title.set_halign(gtk4::Align::Start);
    title.set_valign(gtk4::Align::Center);
    title.set_css_classes(&["caption"]);

    let prev = Button::from_icon_name("media-seek-backward-symbolic");
    prev.set_valign(gtk4::Align::Center);
    let play = Button::new();
    play.set_child(Some(&gtk4::Image::from_icon_name("media-playback-start-symbolic")));
    play.set_valign(gtk4::Align::Center);
    let next = Button::from_icon_name("media-seek-forward-symbolic");
    next.set_valign(gtk4::Align::Center);

    let transport = Box::new(Orientation::Horizontal, 2);
    transport.set_halign(gtk4::Align::Center);
    transport.set_hexpand(true);
    transport.append(&prev);
    transport.append(&play);
    transport.append(&next);

    let queue_btn = Button::from_icon_name("view-grid-symbolic");
    queue_btn.set_valign(gtk4::Align::Center);
    queue_btn.set_tooltip_text(Some(crate::strings::TOOLTIP_QUEUE));

    panel.append(&title);
    panel.append(&transport);
    panel.append(&queue_btn);

    let tx = cmd_tx.clone();
    prev.connect_clicked(move |_| {
        let _ = tx.send(crate::mpd::state_machine::MpdCommand::Previous);
    });
    let tx = cmd_tx.clone();
    play.connect_clicked(move |_| {
        let _ = tx.send(crate::mpd::state_machine::MpdCommand::Pause);
    });
    let tx = cmd_tx.clone();
    next.connect_clicked(move |_| {
        let _ = tx.send(crate::mpd::state_machine::MpdCommand::Next);
    });

    BottomPanel { panel, title, play, queue_btn }
}
