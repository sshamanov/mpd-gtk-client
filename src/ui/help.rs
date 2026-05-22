//! Keyboard shortcuts help dialog.

use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation};

/// Show the keyboard shortcuts modal dialog.
pub(crate) fn show(parent: &impl gtk4::prelude::IsA<gtk4::Window>) {
    let d = gtk4::Window::new();
    d.set_title(Some(crate::strings::SHORTCUTS_TITLE));
    d.set_transient_for(Some(parent));
    d.set_modal(true);
    let content = Box::new(Orientation::Vertical, 4);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    let shortcuts = crate::strings::shortcut_list();
    for (key, desc) in shortcuts {
        let row = Box::new(Orientation::Horizontal, 16);
        let key_lbl = Label::new(Some(key));
        key_lbl.set_halign(gtk4::Align::Start);
        key_lbl.set_css_classes(&["shortcut-key"]);
        key_lbl.set_size_request(180, -1);
        let desc_lbl = Label::new(Some(desc));
        desc_lbl.set_halign(gtk4::Align::Start);
        row.append(&key_lbl);
        row.append(&desc_lbl);
        content.append(&row);
    }
    let close_btn = Button::with_label(crate::strings::CLOSE);
    close_btn.set_margin_top(8);
    close_btn.set_halign(gtk4::Align::End);
    let dw = d.clone();
    close_btn.connect_clicked(move |_| {
        dw.close();
    });
    content.append(&close_btn);
    d.set_child(Some(&content));
    d.present();
}
