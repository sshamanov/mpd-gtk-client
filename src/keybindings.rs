//! Centralized keybinding service — single source of truth for all keyboard shortcuts.
//!
//! Defines a keybinding table with context-based dispatch and compile-time conflict
//! detection. GTK accelerators and event controllers reference this table rather than
//! defining shortcuts inline.
//!
//! Thread: UI (keybindings are defined once at startup and referenced thereafter).

use gtk4::gdk::Key;
use gtk4::gdk::ModifierType;

/// Keyboard context — determines which bindings are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeybindingContext {
    /// Always active regardless of mode.
    Global,
    /// Active when Album Mode browsing (album grid focused).
    AlbumGrid,
    /// Active when Folder Mode browsing (folder tree focused).
    FolderTree,
    /// Active when the search field is focused.
    Search,
    /// Active when the queue is focused.
    Queue,
}

/// All invocable actions, shared by keyboard, menu, hover buttons, and IPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    PlayPause,
    NextTrack,
    PreviousTrack,
    Stop,
    ToggleMode,
    FocusSearch,
    OpenSettings,
    OpenShortcuts,
    Quit,
    QueueMoveUp,
    QueueMoveDown,
    QueueRemoveSelected,
    ActivateSelection,
    DeleteSelected,
    Escape,
    FolderCollapse,
    AlbumPlay,
}

/// A single keybinding entry.
#[derive(Debug, Clone)]
pub struct Binding {
    pub key: Key,
    pub mods: ModifierType,
    pub context: KeybindingContext,
    pub action: Action,
}

/// The global keybinding table.
///
/// All shortcuts are defined here in a single location. The table is checked for
/// conflicts at test time (no two entries with the same `(key, mods, context)`).
pub fn keybindings() -> Vec<Binding> {
    vec![
        // Global shortcuts
        Binding { key: Key::space, mods: ModifierType::empty(), context: KeybindingContext::Global, action: Action::PlayPause },
        Binding { key: Key::f, mods: ModifierType::CONTROL_MASK, context: KeybindingContext::Global, action: Action::FocusSearch },
        Binding { key: Key::comma, mods: ModifierType::CONTROL_MASK, context: KeybindingContext::Global, action: Action::OpenSettings },
        Binding { key: Key::question, mods: ModifierType::CONTROL_MASK, context: KeybindingContext::Global, action: Action::OpenShortcuts },
        Binding { key: Key::q, mods: ModifierType::CONTROL_MASK, context: KeybindingContext::Global, action: Action::Quit },
        Binding { key: Key::Escape, mods: ModifierType::empty(), context: KeybindingContext::Global, action: Action::Escape },

        // Album grid shortcuts
        Binding { key: Key::Return, mods: ModifierType::empty(), context: KeybindingContext::AlbumGrid, action: Action::AlbumPlay },
        Binding { key: Key::KP_Enter, mods: ModifierType::empty(), context: KeybindingContext::AlbumGrid, action: Action::AlbumPlay },

        // Folder tree shortcuts
        Binding { key: Key::Left, mods: ModifierType::empty(), context: KeybindingContext::FolderTree, action: Action::FolderCollapse },
        Binding { key: Key::BackSpace, mods: ModifierType::empty(), context: KeybindingContext::FolderTree, action: Action::FolderCollapse },

        // Queue shortcuts
        Binding { key: Key::Up, mods: ModifierType::SHIFT_MASK, context: KeybindingContext::Queue, action: Action::QueueMoveUp },
        Binding { key: Key::Down, mods: ModifierType::SHIFT_MASK, context: KeybindingContext::Queue, action: Action::QueueMoveDown },
        Binding { key: Key::Delete, mods: ModifierType::empty(), context: KeybindingContext::Queue, action: Action::QueueRemoveSelected },
    ]
}

/// Resolve a key event to an action, given the active context.
pub fn resolve(key: Key, mods: ModifierType, active_context: &[KeybindingContext]) -> Option<Action> {
    // Global bindings are always checked first, then context-specific ones.
    for binding in keybindings() {
        if binding.key == key && binding.mods == mods {
            if binding.context == KeybindingContext::Global || active_context.contains(&binding.context) {
                return Some(binding.action);
            }
        }
    }
    None
}

/// Map an `Action` to its display description for the help dialog.
pub fn action_description(action: &Action) -> &'static str {
    match action {
        Action::PlayPause => "Play / Pause",
        Action::NextTrack => "Next track",
        Action::PreviousTrack => "Previous track",
        Action::Stop => "Stop",
        Action::ToggleMode => "Toggle Album / Folder mode",
        Action::FocusSearch => "Focus search bar",
        Action::OpenSettings => "Open settings",
        Action::OpenShortcuts => "Show keyboard shortcuts",
        Action::Quit => "Quit",
        Action::QueueMoveUp => "Move item up in queue",
        Action::QueueMoveDown => "Move item down in queue",
        Action::QueueRemoveSelected => "Remove from queue",
        Action::ActivateSelection => "Activate selected item",
        Action::DeleteSelected => "Delete selected",
        Action::Escape => "Cancel / Deselect",
        Action::FolderCollapse => "Collapse folder",
        Action::AlbumPlay => "Play album",
    }
}

/// Return the keybinding as a human-readable string.
pub fn binding_label(key: Key, mods: ModifierType) -> String {
    let mut parts = Vec::new();
    if mods.contains(ModifierType::CONTROL_MASK) { parts.push("Ctrl"); }
    if mods.contains(ModifierType::SHIFT_MASK) { parts.push("Shift"); }
    if mods.contains(ModifierType::ALT_MASK) { parts.push("Alt"); }
    let key_name = match key {
        Key::space => "Space".to_string(),
        Key::Return | Key::KP_Enter => "Enter".to_string(),
        Key::Escape => "Esc".to_string(),
        Key::Left => "←".to_string(),
        Key::Right => "→".to_string(),
        Key::Up => "↑".to_string(),
        Key::Down => "↓".to_string(),
        Key::Delete => "Del".to_string(),
        Key::comma => ",".to_string(),
        Key::question => "?".to_string(),
        _ => key.name().unwrap_or_default().to_string(),
    };
    parts.push(&key_name);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Verify no duplicate keybindings within the same context.
    #[test]
    fn test_no_duplicate_bindings() {
        let mut seen: HashSet<(Key, ModifierType, KeybindingContext)> = HashSet::new();
        for binding in keybindings() {
            let key = (binding.key, binding.mods, binding.context);
            assert!(
                seen.insert(key),
                "Duplicate keybinding: {:?} + {:?} in {:?}",
                binding.key.name(), binding.mods, binding.context
            );
        }
    }

    #[test]
    fn test_resolve_global() {
        let result = resolve(Key::space, ModifierType::empty(), &[]);
        assert_eq!(result, Some(Action::PlayPause));
    }

    #[test]
    fn test_resolve_context_specific() {
        // Return without active context should not match
        let result = resolve(Key::Return, ModifierType::empty(), &[]);
        assert_eq!(result, None);
        // With AlbumGrid context should match
        let result = resolve(Key::Return, ModifierType::empty(), &[KeybindingContext::AlbumGrid]);
        assert_eq!(result, Some(Action::AlbumPlay));
    }

    #[test]
    fn test_resolve_no_match() {
        let result = resolve(Key::a, ModifierType::empty(), &[KeybindingContext::Global]);
        assert_eq!(result, None);
    }
}
