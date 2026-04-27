//! Presentation layer — pure functions projecting state into ViewModels. Thread: UI (no blocking I/O).

pub mod browse;
pub mod album_queue;
pub mod track_queue;

pub struct AlbumQueuePresenter;
pub struct TrackQueuePresenter;
