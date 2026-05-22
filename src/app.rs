//! Application wiring — GTK app creation and action registration. Thread: UI (startup).
//! Thin wiring only — no business logic.

use gtk4::gio::SimpleAction;
use gtk4::prelude::*;
use gtk4::Application;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::mpd::state_machine::{CommandSender, MpdEvent, PlaybackUpdate};
use crate::search::SearchCommandSender;
use crate::state::SharedState;

pub fn run(
    state: SharedState,
    event_rx: mpsc::Receiver<MpdEvent>,
    cmd_tx: CommandSender,
    conn_params: Arc<Mutex<crate::mpd::ConnectionTarget>>,
    mpris_update_tx: mpsc::Sender<PlaybackUpdate>,
    metadata_cache: std::sync::Arc<crate::metadata::MetadataCache>,
    search_cmd_tx: SearchCommandSender,
    toast_tx: mpsc::SyncSender<MpdEvent>,
) {
    let application = Application::builder()
        .application_id("com.mpdclient.app")
        .build();

    // Register Ctrl+Q quit action
    let quit_action = SimpleAction::new("quit", None);
    let app_handle = application.clone();
    quit_action.connect_activate(move |_, _| {
        app_handle.quit();
    });
    application.add_action(&quit_action);
    application.set_accels_for_action("app.quit", &["<Ctrl>Q"]);

    // Register remaining actions — handlers connected in build_ui where widgets exist
    for (name, accels) in [
        ("album-mode", &["<Ctrl>1"][..]),
        ("folder-mode", &["<Ctrl>2"][..]),
        ("search", &["<Ctrl>F"][..]),
        ("settings", &["<Ctrl>comma"][..]),
        ("shortcuts", &["<Ctrl>question"][..]),
        ("toggle-sidebar", &["<Ctrl>B"][..]),
    ] {
        let action = SimpleAction::new(name, None);
        application.add_action(&action);
        application.set_accels_for_action(&format!("app.{name}"), accels);
    }

    let event_rx = Arc::new(Mutex::new(event_rx));

    application.connect_activate(move |app| {
        crate::ui::build_ui(
            app,
            state.clone(),
            cmd_tx.clone(),
            event_rx.clone(),
            conn_params.clone(),
            mpris_update_tx.clone(),
            metadata_cache.clone(),
            search_cmd_tx.clone(),
            toast_tx.clone(),
        );
    });

    application.run();
}
