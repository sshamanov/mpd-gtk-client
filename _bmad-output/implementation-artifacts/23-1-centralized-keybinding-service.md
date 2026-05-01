# Story 23.1: Centralized Keybinding Service

Status: backlog

## Story

As a developer,
I want all keyboard shortcuts managed through a single `KeybindingService`,
so that shortcuts are auditable, conflict-free, and consistently dispatched.

## Acceptance Criteria

1. **Single keybinding table**
   - **Given** the application runs
   - **When** any keyboard shortcut is pressed
   - **Then** the key event is routed through a centralized `(GdkKey, GdkModifierType, KeybindingContext) -> Action` mapping
   - **And** the mapping is defined in a single `keybindings.rs` module

2. **Context-based dispatch**
   - **Given** the user is in Album Mode
   - **When** mode-specific keys are pressed
   - **Then** only the `AlbumGrid` context bindings are active
   - **Given** the user switches to Folder Mode
   - **Then** `AlbumGrid` bindings are disabled and `FolderTree` bindings are enabled

3. **Compile-time conflict detection**
   - **Given** the keybinding table is compiled
   - **When** two entries share the same `(key, mods, context)` tuple
   - **Then** a compile-time assertion or `#[cfg(test)]` unit test fails

4. **Action enum unification**
   - **Given** an action can be triggered by keyboard, menu, hover button, or IPC
   - **When** any of those trigger paths fires
   - **Then** they all dispatch the same `Action` enum variant
   - **And** the MPD command mapping is centralized

## References
- [Source: architecture.md §893] Keybinding Architecture ADR
- Existing: `src/ui/mod.rs` has ad-hoc `EventControllerKey` and GTK accelerators

## File List
- `src/keybindings.rs` (new) — Keybinding table, Action enum, context types
- `src/ui/mod.rs` — Remove ad-hoc key handlers, delegate to KeybindingService
