# Story 22.1: libadwaita Widget Integration

Status: backlog

## Story

As a developer,
I want to replace custom composite widgets with libadwaita (adw) equivalents,
so that the UI follows GNOME HIG conventions and reduces custom widget maintenance.

## Acceptance Criteria

1. **Toast overlay replaced with `Adw.ToastOverlay`**
   - **Given** the application shows toast notifications
   - **When** the toast infrastructure is migrated
   - **Then** the custom toast widget is replaced with `Adw.ToastOverlay` wrapping the main content
   - **And** toast messages use `Adw.Toast` with standard timeout and styling

2. **Mode switcher replaced with `Adw.ViewSwitcher`**
   - **Given** the user can switch between Album and Folder modes
   - **When** the mode switcher is migrated
   - **Then** the custom group/toggle button bar is replaced with `Adw.ViewSwitcher`
   - **And** mode switching still dispatches the same underlying commands

3. **Navigation stack replaced with `Adw.NavigationView`**
   - **Given** the user navigates between views
   - **When** the navigation infrastructure is migrated
   - **Then** `Adw.NavigationView` manages the view stack instead of manual visibility toggling

4. **Responsive sidebar uses `Adw.MultiLayoutView`**
   - **Given** the window is resized across breakpoints
   - **When** the responsive layout is migrated
   - **Then** `Adw.MultiLayoutView` + `Adw.BottomSheet` handle the single-column → split-view transition

5. **Feature-gated behind `libadwaita` feature**
   - **Given** `libadwaita` feature is not enabled
   - **When** the application is compiled
   - **Then** existing custom widgets are used (backward compatible)
   - **Given** the feature is enabled
   - **When** the application runs
   - **Then** `libadwaita` >= 1.6 is required at runtime

## References
- [Source: architecture.md §474] libadwaita Integration ADR
- [adw crate docs] https://docs.rs/adw/latest/adw/

## File List
- `Cargo.toml` — Add `adw` dependency (optional, feature-gated)
- `src/ui/mod.rs` — Widget replacements
- `src/ui/widgets/toast.rs` — Remove or gate behind !feature
