//! Album grid cell with square cover + labels + hover buttons.
//!
//! AlbumCoverCell uses GtkBoxLayout (NOT manual size_allocate) following the
//! plattenalbum pattern of GtkBox as ListItem child container.
//!
//! SquareCover is an internal HEIGHT_FOR_WIDTH widget that forces 1:1 aspect
//! ratio — pattern from plattenalbum's AlbumCover.do_measure returning
//! (for_size, for_size, -1, -1).

use crate::mpd::state_machine::{CommandSender, MpdCommand};
use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::RefCell;

mod square_cover_imp {
    use super::*;

    #[derive(Default)]
    pub struct SquareCover {
        pub overlay: RefCell<Option<gtk4::Overlay>>,
        pub picture: RefCell<Option<gtk4::Picture>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SquareCover {
        const NAME: &'static str = "MpdSquareCover";
        type Type = super::SquareCover;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for SquareCover {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_hexpand(true);

            let overlay = gtk4::Overlay::new();

            let picture = gtk4::Picture::new();
            picture.set_widget_name("cover-image");
            picture.set_content_fit(gtk4::ContentFit::Cover);
            picture.set_can_shrink(false);
            picture.set_halign(gtk4::Align::Fill);
            picture.set_valign(gtk4::Align::Fill);
            overlay.set_child(Some(&picture));
            self.picture.replace(Some(picture));

            overlay.set_parent(obj.upcast_ref::<gtk4::Widget>());
            self.overlay.replace(Some(overlay));
        }

        fn dispose(&self) {
            if let Some(ref child) = *self.overlay.borrow() {
                child.unparent();
            }
            self.overlay.take();
            self.picture.take();
        }
    }

    impl WidgetImpl for SquareCover {
        fn request_mode(&self) -> gtk4::SizeRequestMode {
            gtk4::SizeRequestMode::HeightForWidth
        }

        fn measure(
            &self,
            _orientation: gtk4::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            if for_size > 0 {
                (for_size, for_size, -1, -1)
            } else {
                (-1, -1, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            if let Some(ref overlay) = *self.overlay.borrow() {
                overlay.allocate(width, height, -1, None);
            }
        }

    }
}

glib::wrapper! {
    pub struct SquareCover(ObjectSubclass<square_cover_imp::SquareCover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl SquareCover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn picture(&self) -> gtk4::Picture {
        self.imp().picture.borrow().clone().unwrap()
    }

    pub fn overlay(&self) -> gtk4::Overlay {
        self.imp().overlay.borrow().clone().unwrap()
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct AlbumCoverCell {
        pub cmd_tx: RefCell<Option<CommandSender>>,
        pub album_key: RefCell<String>,
        pub album_section: RefCell<Option<gtk4::Box>>,
        pub cover_picture: RefCell<Option<gtk4::Picture>>,
        pub title_label: RefCell<Option<gtk4::Label>>,
        pub artist_label: RefCell<Option<gtk4::Label>>,
        pub btn_add: RefCell<Option<gtk4::Button>>,
        pub btn_next: RefCell<Option<gtk4::Button>>,
        pub btn_play: RefCell<Option<gtk4::Button>>,
        pub context_popover: RefCell<Option<gtk4::Popover>>,
        pub group_caption: RefCell<Option<gtk4::Box>>,
        pub year_badge: RefCell<Option<gtk4::Label>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumCoverCell {
        const NAME: &'static str = "MpdAlbumCoverCell";
        type Type = super::AlbumCoverCell;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for AlbumCoverCell {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let layout = gtk4::BoxLayout::new(gtk4::Orientation::Vertical);
            layout.set_spacing(0);
            obj.set_layout_manager(Some(layout));

            obj.set_hexpand(true);
            obj.set_size_request(200, 250);
            obj.set_css_classes(&["album-cover-cell"]);

            // Album section — vertical Box containing cover + labels.
            let album_section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            album_section.set_hexpand(true);
            album_section.set_css_classes(&["album-cover-section"]);

            // Square cover widget — forces 1:1 aspect ratio
            let square_cover = SquareCover::new();
            self.cover_picture.replace(Some(square_cover.picture()));

            // --- Hover buttons — bottom-right of cover ---

            let btn_add = gtk4::Button::with_label(crate::strings::BTN_ADD_LABEL);
            btn_add.set_css_classes(&["album-cover-hover-btn"]);
            btn_add.set_tooltip_text(Some(crate::strings::TOOLTIP_ADD_TO_QUEUE));
            self.btn_add.replace(Some(btn_add.clone()));

            let btn_next = gtk4::Button::with_label(crate::strings::BTN_NEXT_LABEL);
            btn_next.set_css_classes(&["album-cover-hover-btn"]);
            btn_next.set_tooltip_text(Some(crate::strings::TOOLTIP_PLAY_NEXT));
            self.btn_next.replace(Some(btn_next.clone()));

            let btn_play = gtk4::Button::with_label(crate::strings::BTN_PLAY_LABEL);
            btn_play.set_css_classes(&["album-cover-hover-btn"]);
            btn_play.set_tooltip_text(Some(crate::strings::TOOLTIP_CLEAR_AND_PLAY));
            self.btn_play.replace(Some(btn_play.clone()));

            let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            btn_box.set_halign(gtk4::Align::End);
            btn_box.set_valign(gtk4::Align::End);
            btn_box.set_margin_bottom(4);
            btn_box.set_margin_end(4);
            btn_box.append(&btn_add);
            btn_box.append(&btn_next);
            btn_box.append(&btn_play);
            square_cover.overlay().add_overlay(&btn_box);
            // CSS-only hover: buttons are always sensitive, opacity controlled by CSS

            // Group captions — stacked labels at top-left of cover, max 6
            let group_caption = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
            group_caption.set_halign(gtk4::Align::Start);
            group_caption.set_valign(gtk4::Align::Start);
            group_caption.set_margin_start(4);
            group_caption.set_margin_top(4);
            group_caption.set_visible(false);
            square_cover.overlay().add_overlay(&group_caption);
            self.group_caption.replace(Some(group_caption));

            // Year badge — last 2 digits at top-right of cover
            let year_badge = gtk4::Label::new(None);
            year_badge.set_halign(gtk4::Align::End);
            year_badge.set_valign(gtk4::Align::Start);
            year_badge.set_margin_end(4);
            year_badge.set_margin_top(4);
            year_badge.set_css_classes(&["year-badge"]);
            year_badge.set_visible(false);
            square_cover.overlay().add_overlay(&year_badge);
            self.year_badge.replace(Some(year_badge));

            album_section.append(&square_cover);

            // Title label (album name) — fills available cell width
            let title_label = gtk4::Label::new(None);
            title_label.set_halign(gtk4::Align::Start);
            title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            title_label.set_lines(1);
            title_label.set_hexpand(true);
            album_section.append(&title_label);
            self.title_label.replace(Some(title_label));

            // Artist label (bold) — year combined inline: "Artist · 2024"
            let artist_label = gtk4::Label::new(None);
            artist_label.set_halign(gtk4::Align::Start);
            artist_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            artist_label.set_lines(1);
            artist_label.set_hexpand(true);
            artist_label.set_css_classes(&["album-cell-artist"]);
            album_section.append(&artist_label);
            self.artist_label.replace(Some(artist_label));

            album_section.set_parent(obj.upcast_ref::<gtk4::Widget>());
            self.album_section.replace(Some(album_section));
        }

        fn dispose(&self) {
            if let Some(ref child) = *self.album_section.borrow() {
                child.unparent();
            }
            self.album_section.take();
            self.cover_picture.take();
            self.title_label.take();
            self.artist_label.take();
            self.btn_add.take();
            self.btn_next.take();
            self.btn_play.take();
            self.context_popover.take();
            self.group_caption.take();
            self.year_badge.take();
        }
    }

    impl WidgetImpl for AlbumCoverCell {
        fn measure(&self, orient: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orient {
                gtk4::Orientation::Horizontal => (200, 200, -1, -1),
                gtk4::Orientation::Vertical => (250, 250, -1, -1),
                _ => (-1, -1, -1, -1),
            }
        }
    }
}

glib::wrapper! {
    pub struct AlbumCoverCell(ObjectSubclass<imp::AlbumCoverCell>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl AlbumCoverCell {
    pub fn new(cmd_tx: CommandSender) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().cmd_tx.replace(Some(cmd_tx));
        obj.wire_buttons();
        obj.wire_context_menu();
        obj.wire_drag_source();
        obj.wire_activation();
        obj
    }

    fn wire_activation(&self) {
        let cmd = self.imp().cmd_tx.borrow().clone().expect("cmd_tx set before wire_activation");
        let cell = self.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(1);
        gesture.connect_pressed(move |_gest, n_clicks, _x, _y| {
            if n_clicks == 2 {
                let key = cell.album_key();
                if !key.is_empty() {
                    let _ = cmd.send(MpdCommand::PlayAlbum(key));
                }
            }
        });
        self.add_controller(gesture);
    }

    fn wire_drag_source(&self) {
        let drag = gtk4::DragSource::new();
        drag.set_actions(gtk4::gdk::DragAction::COPY);
        let cell = self.clone();
        drag.connect_prepare(move |_source, _x, _y| {
            let key = cell.album_key();
            if key.is_empty() {
                return None;
            }
            Some(gtk4::gdk::ContentProvider::for_value(&glib::Value::from(&key)))
        });
        self.add_controller(drag);
    }

    fn wire_buttons(&self) {
        let imp = self.imp();
        let cmd_tx = imp
            .cmd_tx
            .borrow()
            .clone()
            .expect("cmd_tx set before wire_buttons");

        if let Some(ref btn_add) = *imp.btn_add.borrow() {
            let tx = cmd_tx.clone();
            let obj = self.clone();
            btn_add.connect_clicked(move |_| {
                let key = obj.imp().album_key.borrow().clone();
                if !key.is_empty() {
                    let _ = tx.send(MpdCommand::Add(key));
                }
            });
        }

        if let Some(ref btn_next) = *imp.btn_next.borrow() {
            let tx = cmd_tx.clone();
            let obj = self.clone();
            btn_next.connect_clicked(move |_| {
                let key = obj.imp().album_key.borrow().clone();
                if !key.is_empty() {
                    let _ = tx.send(MpdCommand::InsertNext(key));
                }
            });
        }

        if let Some(ref btn_play) = *imp.btn_play.borrow() {
            let obj = self.clone();
            btn_play.connect_clicked(move |_| {
                let key = obj.imp().album_key.borrow().clone();
                if !key.is_empty() {
                    let _ = cmd_tx.send(MpdCommand::PlayAlbum(key));
                }
            });
        }
    }

    /// Add right-click context menu gesture directly on the cell widget.
    /// Uses Capture phase to see events before child widgets (labels, buttons, cover).
    /// set_button(3) restricts to right-click only.
    fn wire_context_menu(&self) {
        let imp = self.imp();
        let cmd_tx = imp
            .cmd_tx
            .borrow()
            .clone()
            .expect("cmd_tx set before wire_context_menu");
        let cell = self.clone();

        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);
        gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
        gesture.connect_pressed(move |_gest, _n, x, y| {
            log::debug!("AlbumCoverCell context menu fired, key={}", cell.album_key());
            let key = cell.album_key();
            if key.is_empty() {
                return;
            }

            // Dismiss previous popover
            if let Some(ref old) = *cell.imp().context_popover.borrow() {
                old.popdown();
            }

            let pop = gtk4::Popover::new();
            let popbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            let btn_play = gtk4::Button::with_label(crate::strings::PLAY_NOW);
            let btn_next = gtk4::Button::with_label(crate::strings::PLAY_NEXT);
            let btn_add = gtk4::Button::with_label(crate::strings::ADD_TO_QUEUE);
            let pop_close = cell.clone();

            let tp = cmd_tx.clone();
            let k_play = key.clone();
            let pc = pop_close.clone();
            btn_play.connect_clicked(move |_| {
                let _ = tp.send(MpdCommand::PlayAlbum(k_play.clone()));
                if let Some(ref p) = *pc.imp().context_popover.borrow() { p.popdown(); }
            });
            let tn = cmd_tx.clone();
            let k_next = key.clone();
            let pc = pop_close.clone();
            btn_next.connect_clicked(move |_| {
                let _ = tn.send(MpdCommand::InsertNext(k_next.clone()));
                if let Some(ref p) = *pc.imp().context_popover.borrow() { p.popdown(); }
            });
            let ta = cmd_tx.clone();
            let k_add = key.clone();
            let pc = pop_close.clone();
            btn_add.connect_clicked(move |_| {
                let _ = ta.send(MpdCommand::Add(k_add.clone()));
                if let Some(ref p) = *pc.imp().context_popover.borrow() { p.popdown(); }
            });

            popbox.append(&btn_play);
            popbox.append(&btn_next);
            popbox.append(&btn_add);
            pop.set_child(Some(&popbox));
            pop.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
                x as i32, y as i32, 1, 1,
            )));
            pop.set_parent(cell.upcast_ref::<gtk4::Widget>());
            pop.popup();
            *cell.imp().context_popover.borrow_mut() = Some(pop);
        });
        self.add_controller(gesture);
    }

    pub fn set_album(
        &self,
        title: &str,
        artist: &str,
        year: Option<&str>,
        mpd_lookup_name: &str,
    ) {
        let imp = self.imp();
        imp.album_key.replace(mpd_lookup_name.to_string());

        if let Some(ref a) = *imp.album_section.borrow() {
            a.set_visible(true);
        }
        if let Some(ref t) = *imp.title_label.borrow() {
            t.set_text(title);
        }
        if let Some(ref ar) = *imp.artist_label.borrow() {
            let artist_text = if artist.is_empty() {
                crate::strings::UNKNOWN_ARTIST
            } else {
                artist
            };
            if let Some(y) = year {
                ar.set_text(&format!("{} · {}", artist_text, y));
            } else {
                ar.set_text(artist_text);
            }
        }
    }

    pub fn set_cover_texture(&self, texture: &impl IsA<gdk4::Paintable>) {
        if let Some(ref pic) = *self.imp().cover_picture.borrow() {
            pic.set_paintable(Some(texture));
        }
    }

    /// Load cover from a file path via GTK's async image loading.
    /// This does NOT decode JPEG on the calling thread — GDK handles
    /// loading and scaling asynchronously in the compositor.
    pub fn set_cover_filename(&self, path: &str) {
        if let Some(ref pic) = *self.imp().cover_picture.borrow() {
            pic.set_filename(Some(path));
        }
    }

    pub fn cover_picture(&self) -> Option<gtk4::Picture> {
        self.imp().cover_picture.borrow().clone()
    }

    pub fn album_key(&self) -> String {
        self.imp().album_key.borrow().clone()
    }

    /// Show or hide stacked group caption labels on the cover overlay (max 5).
    pub fn set_group_captions(&self, captions: &[String]) {
        if let Some(ref cap_box) = *self.imp().group_caption.borrow() {
            // Remove old labels one by one to avoid mutation-during-iteration
            while let Some(child) = cap_box.first_child() {
                child.unparent();
            }
            if captions.is_empty() {
                cap_box.set_visible(false);
            } else {
                for text in captions.iter().take(6) {
                    let lbl = gtk4::Label::new(Some(text));
                    lbl.set_halign(gtk4::Align::Start);
                    lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    lbl.set_max_width_chars(14);
                    lbl.set_css_classes(&["group-caption"]);
                    cap_box.append(&lbl);
                }
                cap_box.set_visible(true);
            }
        }
    }

    /// Show or hide the year badge (last 2 digits) at top-right of cover.
    pub fn set_year_badge(&self, text: Option<&str>) {
        if let Some(ref badge) = *self.imp().year_badge.borrow() {
            if let Some(t) = text {
                badge.set_text(t);
                badge.set_visible(true);
            } else {
                badge.set_visible(false);
            }
        }
    }
}
