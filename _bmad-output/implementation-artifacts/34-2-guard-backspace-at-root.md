# Story 34.2: Guard BackSpace at Root in Folder Tree

Status: done

## Story

As a developer,
I want pressing BackSpace at the root of the folder tree to be a no-op,
so that an empty string is never sent as a `ListDirectory` command.

## Acceptance Criteria

1. **BackSpace at root is a no-op**
   - Given the folder tree is at the root level (no parent directory, breadcrumb shows root)
   - When the user presses BackSpace or Left arrow
   - Then the key press is consumed without sending any MPD command
   - And a debug log notes that the user is at root

2. **BackSpace at non-root works normally**
   - Given the folder tree is at a non-root level (has a parent directory)
   - When the user presses BackSpace or Left arrow
   - Then the existing behavior is preserved — parent directory is navigated to via `ListDirectory(parent)`

## Technical Requirements

- In `src/ui/widgets/folder_tree.rs`, the BackSpace/Left handler at line 115 unconditionally computes a parent path and sends `ListDirectory(parent)`.
- When at root, the `parent` computation on lines 110-111 produces an empty string `""`.
- The `MpdCommand::ListDirectory("")` handler in `state_machine.rs` (line 856) processes this as an empty path, which is harmless but semantically wrong.
- Fix: Add a guard in the folder tree keyboard handler. Check if `dir_path` (the current root) is empty or is `/`. If so, return without sending any command. The parent path computation only makes sense when there is an actual parent.
- The `current_breadcrumb` or equivalent state indicates whether root is showing — check this before computing parent.

## References
- [Source: epics.md] Epic 34: General Code Quality — Story 34.2
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — BackSpace at root sends ListDirectory("")
- [Source: src/ui/widgets/folder_tree.rs:108-115] BackSpace/Left key handler
