//! Toast notification overlay — bottom-right transient messages. Thread: UI (GTK main loop).

use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation, Revealer};

pub struct ToastOverlay {
    pub container: Box,
    revealer: Revealer,
}

impl Default for ToastOverlay {
    fn default() -> Self { Self::new() }
}

impl ToastOverlay {
    pub fn new() -> Self {
        let container = Box::new(Orientation::Vertical, 2);
        container.set_halign(gtk4::Align::End);
        container.set_valign(gtk4::Align::End);
        container.set_margin_bottom(8);
        container.set_margin_end(8);
        container.set_widget_name("toast-container");

        let revealer = Revealer::new();
        revealer.set_child(Some(&container));
        revealer.set_transition_type(gtk4::RevealerTransitionType::SlideUp);
        revealer.set_reveal_child(false);
        revealer.set_can_target(false);

        Self { container, revealer }
    }

    pub fn widget(&self) -> &Revealer { &self.revealer }

    pub fn show_toast(&self, msg: &str) {
        // Clear old toasts
        while let Some(child) = self.container.first_child() {
            self.container.remove(&child);
        }

        let label = Label::new(Some(msg));
        label.set_widget_name("toast-label");
        label.set_css_classes(&["toast-message"]);
        label.set_margin_start(8);
        label.set_margin_end(8);
        label.set_margin_top(4);
        label.set_margin_bottom(4);

        // Click to dismiss
        let revealer = self.revealer.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |_g, _n, _x, _y| {
            revealer.set_reveal_child(false);
        });
        label.add_controller(gesture);

        self.container.append(&label);
        self.revealer.set_can_target(true);
        self.revealer.set_reveal_child(true);

        // Auto-dismiss after 3 seconds
        let r = self.revealer.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(3), move || {
            r.set_reveal_child(false);
            r.set_can_target(false);
        });
    }
}
