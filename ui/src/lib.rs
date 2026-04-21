pub mod widgets;

use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box, Orientation, Paned, ScrolledWindow};
use state::SharedState;

pub struct App {
    state: SharedState,
}

impl App {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    pub fn run(&self) {
        let app = Application::builder()
            .application_id("com.github.schaman.mpd-client")
            .build();

        app.connect_activate(|app| {
            let window = ApplicationWindow::builder()
                .application(app)
                .default_width(1200)
                .default_height(800)
                .title("MPD Client")
                .build();

            // Create main split pane (70/30)
            let paned = Paned::new(Orientation::Horizontal);
            paned.set_position(70); // 70% left, 30% right

            // Left pane: browsing area
            let left_pane = ScrolledWindow::new();
            // TODO: add album grid or folder tree
            paned.set_start_child(Some(&left_pane));

            // Right pane: persistent rail
            let right_pane = Box::new(Orientation::Vertical, 0);
            // TODO: add now playing, queue, etc.
            paned.set_end_child(Some(&right_pane));

            window.set_child(Some(&paned));
            window.show();
        });

        app.run();
    }
}