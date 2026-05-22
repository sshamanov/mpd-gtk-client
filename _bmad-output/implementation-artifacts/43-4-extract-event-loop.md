# Story 43-4: Extract Event Loop from Tick Callback

## Status: done

## Context

The GTK tick callback in `ui/mod.rs` (lines 2126-2861) is a 735-line giant match block processing all `MpdEvent` variants. It captures ~80 local variables from the `connect_activate` closure. Extract into `src/ui/event_loop.rs` using a `UiHandles` struct pattern.

See `.claude/plans/glittery-wiggling-puzzle.md` Step 12.

## Tasks

### Task 1: Define `UiHandles` struct in `src/ui/event_loop.rs`
Create a struct holding all widget references the event dispatch loop needs:
- Now-playing widgets (labels, buttons, cover, seekbar, bottom panel widgets)
- Queue widgets (listbox, mini-fixed, mini-cells, item-ids, queue-entries)
- Grid state (album_cells, album_layout, cover_paths, texture_cache, left_stack, empty_label)
- Folder state (folder_browser, folder_search_list, folder_cap_note)
- Layout state (prev_mode, multi_view, bottom_panel, bottom_sheet, wide_right, queue_stack)
- Channels (cmd_tx, search_cmd_tx, mpris_update_tx, toast_tx)
- State refs (state: SharedState, metadata_cache, counters, generation trackers)
- Other: toast_overlay, shutdown, cover_paths_np, popover data, track_listbox, active_group, track_revealer

### Task 2: Extract `process_events()` function
Move the match block from the tick callback into:
```rust
pub fn process_events(
    handles: &UiHandles,
    event_rx: &Arc<Mutex<mpsc::Receiver<MpdEvent>>>,
    toast_tx: &mpsc::SyncSender<MpdEvent>,
) -> glib::ControlFlow
```

### Task 3: Shrink tick callback in `ui/mod.rs`
The remaining callback handles only:
1. Layout management (narrow/wide mode detection, rail width)
2. Grid resize + reposition trigger
3. Call `event_loop::process_events(&handles, &event_rx, &toast_tx)`

~40 lines instead of ~735.

### Task 4: Register `pub mod event_loop;` in `src/ui/mod.rs`

## Acceptance Criteria
1. `src/ui/event_loop.rs` contains `UiHandles` struct and `process_events()`
2. Tick callback in `ui/mod.rs` is ≤60 lines
3. All MpdEvent variants processed correctly
4. `cargo build` passes
5. Manual test: window opens, albums load, search works, queue updates, now-playing works, toasts appear
