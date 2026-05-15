//! Desktop notification integration and routing.
//!
//! Toast events arrive from the GTK thread's event loop via the `toast_tx` sync_channel
//! and take one of two paths depending on the `[notifications] mode` config setting:
//! 1. GTK thread — in-app toast overlay via `adw::ToastOverlay` (always active)
//! 2. NotificationRouter thread — desktop notifications via `org.freedesktop.Notifications` D-Bus
//!
//! The NotificationRouter is spawned unconditionally but when mode is `"toast"`
//! the channel receiver is dropped, making sends no-ops.
//!
//! Architecture: architecture.md §2342-2400 (Toast & Notification Consolidation)

pub mod router;
