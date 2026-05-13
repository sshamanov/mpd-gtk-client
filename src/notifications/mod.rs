//! Desktop notification integration and routing.
//!
//! Two paths for toast events after `reduce()`:
//! 1. GTK thread — in-app toast overlay (always active, no D-Bus)
//! 2. NotificationRouter thread — desktop notifications via `org.freedesktop.Notifications` D-Bus
//!
//! The NotificationRouter is spawned unconditionally. When `[notifications] mode`
//! is `"toast"` or the `mpris` feature is not compiled, it silently no-ops.
//!
//! Architecture: architecture.md §2342-2400 (Toast & Notification Consolidation)

pub mod router;
