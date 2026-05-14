//! Presenter layer — stateless pure projection functions.
//!
//! Transforms application state (MPD data, queue, browsing state) into display-ready
//! view models with zero GTK dependencies. Widgets import presenters, never perform projection.
//!
//! Rules (from architecture.md §2453-2459, §2710-2715):
//! - No `gtk4`, `gdk4`, or `gdk_pixbuf` imports
//! - Stateless pure functions — no mutable state
//! - Presenters compute *what* to display; widgets decide *how*

pub mod browse;
pub mod folder_norm;
pub mod format;
pub mod types;
