# Story 14.3: Second-Instance Detection

Status: ready-for-dev

## Story

As a user launching the application a second time,
I want the second instance to detect the running instance and forward CLI actions,
so that I don't get duplicate windows and my intended action still happens.

## Acceptance Criteria

1. **Lock file prevents duplicate instances**
   - **Given** an instance of the application is already running
   - **When** a second instance is launched
   - **Then** the second instance detects the running instance via a lock file at `~/.cache/mpd-client/lock`
   - **And** the second instance does not create a window

2. **CLI action forwarding via Unix socket**
   - **Given** a second instance is launched with `--toggle-playback` (or other action flags)
   - **When** it detects the running instance via the lock file
   - **Then** the action is forwarded to the running instance via a Unix socket
   - **And** the running instance dispatches the action as an internal command
   - **And** the second instance exits with code 0

3. **Lock file creation**
   - **Given** no instance is running
   - **When** the application starts
   - **Then** a lock file is created at `~/.cache/mpd-client/lock`
   - **And** a Unix socket listener is started at `~/.cache/mpd-client/socket`

4. **Clean shutdown**
   - **Given** the application exits normally
   - **When** the shutdown sequence runs
   - **Then** the lock file is deleted
   - **And** the Unix socket is cleaned up
   - **Given** the application crashes
   - **When** it restarts
   - **Then** the stale lock file is detected and replaced (via `gio`-managed socket auto-cleanup or PID check)

5. **No lock file = normal startup**
   - **Given** no lock file exists
   - **When** the application starts
   - **Then** it proceeds with normal initialization (window, MPD connection, etc.)

## Tasks / Subtasks

- [ ] (AC: 1, 3) Implement lock file at `~/.cache/mpd-client/lock` on startup
  - [ ] Write PID to lock file for stale detection
  - [ ] Check lock file on startup; if exists and PID is alive, enter second-instance mode
- [ ] (AC: 2) Implement Unix socket listener in first instance
  - [ ] Listener on `~/.cache/mpd-client/socket` using `gio` socket API on GTK main loop
  - [ ] Parse incoming action strings and dispatch via `cmd_tx`
- [ ] (AC: 2) Implement Unix socket client in second instance
  - [ ] Connect to running instance's socket, send action string, wait for ack, exit
- [ ] (AC: 4) Clean up lock file and socket on shutdown
- [ ] (AC: 4) Handle stale lock files (PID no longer running)
- [ ] (AC: 5) Normal startup path when no lock file exists

## Dev Notes

- **Protocol:** Simple text-based protocol over Unix socket. Second instance sends action as plain text (e.g., `toggle-playback\n`), first instance responds with `OK\n` or `ERR\n`.
- **Action to command mapping:** Same action strings as CLI flag names (without `--` prefix): `toggle-playback`, `next`, `prev`, `start-playing`.
- **No separate IPC thread:** Use GTK main loop's `gio` socket API (`g_io_add_watch` equivalent) for the Unix socket listener. This avoids threading issues.
- **Stale lock detection:** Read PID from lock file. If `/proc/<pid>/status` doesn't exist or the process name doesn't match, the lock is stale.
- **Lock file path:** `dirs::cache_dir() + "/mpd-client/lock"` using the existing `dirs` crate dependency.
- **Integration with CLI args:** The second-instance detection happens AFTER CLI parsing (so action flags are extracted) but BEFORE window creation.

### Source Files to Touch
- `src/main.rs` — Add lock file check before window creation, conditionally skip to second-instance forwarding
- New file: `src/ipc/mod.rs` — Shared socket/listener implementation (alongside MPRIS code)
- `src/config/mod.rs` — Add cache path resolution (or use existing dirs calls)

### Testing
- Unit test for lock file write/read and PID parsing
- Integration test for Unix socket send/receive
- Stale lock file detection test (manipulate PID)
- Edge cases: permission errors on lock file directory, racing instances

## References

- [Source: architecture.md §661] IPC & CLI Architecture — Second-instance detection via lock file
- [Source: epics.md §14] Epic 14: CLI & Desktop Integration

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- Lock file and socket paths use `dirs::cache_dir()` consistent with cover art cache location
- Socket listener runs on GTK main thread via gio socket API, not a separate thread

### File List

- `src/main.rs`
- `src/ipc/mod.rs` (new)
