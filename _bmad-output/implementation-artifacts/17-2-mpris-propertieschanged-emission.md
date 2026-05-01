# Story 17.2: Emit MPRIS PropertiesChanged Signal on Playback State Changes

Status: done

## Story

As a user with lock screen media controls,
I want the MPRIS interface to emit `PropertiesChanged` signals on playback state transitions,
so that `playerctl status` and lock screen controls auto-update without manual polling.

## Acceptance Criteria

1. **PropertiesChanged signal on state transitions**
   - **Given** the application is running with MPRIS enabled
   - **When** playback state changes (playing to paused, paused to playing, stop, track change)
   - **Then** `org.freedesktop.DBus.Properties.PropertiesChanged` is emitted for `org.mpris.MediaPlayer2.Player`
   - **And** the signal carries the changed properties dict: `PlaybackStatus`, `Metadata`, `Position`
   - **And** the `InvalidatedProperties` array is empty (all changes via changed properties)

2. **playerctl --follow receives updates**
   - **Given** `playerctl --follow` is monitoring the MPRIS interface
   - **When** a track change occurs
   - **Then** `playerctl --follow` outputs the updated metadata within 1 second of the change

3. **No duplicate emissions**
   - **Given** playback state remains unchanged (same track, same play/pause state)
   - **When** no MPD event triggers a state transition
   - **Then** no `PropertiesChanged` signal is emitted

4. **Graceful channel drop**
   - **Given** the MPRIS module is disabled or the channel is dropped
   - **When** a `PlaybackUpdate` is sent on the channel
   - **Then** the send is best-effort (no crash if receiver dropped)
   - **And** the application continues without MPRIS (no panic)

5. **Thread-safe communication**
   - **Given** the StateChanged handler runs on the GTK main loop thread
   - **When** it sends a `PlaybackUpdate` to the MPRIS module
   - **Then** the send does not block the GTK thread (bounded channel with try_send)
   - **And** the zbus IO thread processes the emission asynchronously

## Tasks / Subtasks

- [ ] (AC: 1-2) Add `mpsc::Sender<PlaybackUpdate>` channel from UI event handler to MPRIS module
  - [ ] Add channel creation in `main.rs` alongside MPRIS init
  - [ ] Pass sender to the `StateChanged` handler closure in `ui/mod.rs`
  - [ ] Pass receiver to `mpris::init()` or create a new `MprisUpdateForwarder` thread
  - [ ] In the receiver thread: on update, emit `PropertiesChanged` via zbus `ObjectServer`
- [ ] (AC: 3) Debounce/throttle identical state updates
  - [ ] Track last-emitted `(state, song_pos, title, album)` tuple
  - [ ] Skip emission if the tuple matches the last emission
- [ ] (AC: 4) Use `try_send` instead of `send` for non-blocking delivery
- [ ] (AC: 5) Verify thread safety
  - [ ] Confirm zbus ObjectServer API is `Send` across threads
  - [ ] Ensure no RwLock contention between GTK writer and zbus reader

## Dev Notes

### Architecture

Currently, MPRIS property getters in `src/mpris.rs` read `SharedState` directly. This works for poll-based clients (`playerctl status`, `playerctl metadata`) because each call triggers a fresh read. However, signal-based clients (lock screen, GNOME Shell media controls, KDE Plasma) wait for `PropertiesChanged` signals and never see state updates without them.

### Communication Channel

The `StateChanged` handler in `src/ui/mod.rs` already has access to:
- The `PlaybackUpdate` struct (from `MpdEvent::StateChanged(update)`)
- Various `cmd_tx` channels for MPD commands

A new channel is needed:
```rust
let (mpris_update_tx, mpris_update_rx) = std::sync::mpsc::channel::<PlaybackUpdate>();
```

The sender is cloned into the `StateChanged` handler closure. The receiver is passed to the MPRIS module which spawns a receiver thread (or uses zbus's existing IO thread).

### zbus PropertiesChanged Emission

zbus 5.x provides `ObjectServer` with property emission:

```rust
use zbus::interface;
use zbus::object_server::Interface;

// After getting the ObjectServer from the connection:
let server = connection.object_server();
let iface = server.interface::<_, MprisPlayer>(path).unwrap();
iface.set_properties(changed_properties).unwrap();
```

Or alternatively, emit via the connection's `object_server()` directly:

```rust
// The ObjectServer::emit_properties_changed method
connection.object_server().emit_properties_changed(
    &ObjectPath::from_str("/org/mpris/MediaPlayer2").unwrap(),
    &["PlaybackStatus", "Metadata", "Position"],
).ok();
```

The `changed_properties` is a `HashMap<String, Value<'_>>` with the new values for each property.

### Changed Properties on Each Event

| Transition | Changed Properties |
|------------|-------------------|
| State change (play/pause/stop) | `PlaybackStatus` |
| Track change | `Metadata`, `Position` |
| Seek | `Position` |

Always include: `PlaybackStatus`, `Metadata`, `Position` — it's safe to over-emit (clients ignore unchanged properties).

### Thread Model

- The receiver thread is a simple `loop { rx.recv() }` on a dedicated thread
- It calls `connection.object_server().emit_properties_changed(...)` which is thread-safe (zbus manages its own locking)
- Use `try_send` from the GTK thread to avoid blocking on a full channel
- Use `recv` (blocking) on the receiver thread — it's a dedicated thread, blocking is fine

### Existing Code Context

The `mpris::init()` function signature:
```rust
pub fn init(cmd_tx: mpsc::Sender<MpdCommand>, state: SharedState, enabled: bool) -> Option<Connection>
```

After this story, it should also accept:
```rust
pub fn init(cmd_tx: ..., state: ..., enabled: bool, update_rx: mpsc::Receiver<PlaybackUpdate>) -> Option<Connection>
```

Or the receiver thread could be spawned separately in `main.rs` after `mpris::init()`.

### MPRIS Track ID Requirement

When emitting `Metadata`, the `mpris:trackid` field must be a valid D-Bus `ObjectPath`. The MPRIS spec requires this to be a unique, persistent identifier for the track. Use `format!("/org/mpris/MediaPlayer2/Track/{}", song_pos)` where `song_pos` comes from `PlaybackUpdate.song`.

### Testing

- `cargo build` with `--features mpris` passes
- `cargo test` passes (zero regressions)
- Manual: `playerctl --player=mpdclient --follow` shows updates on track change
- Manual: `playerctl --player=mpdclient status` shows state transitions
- Manual (advanced): `dbus-monitor "interface=org.freedesktop.DBus.Properties"` to watch PropertiesChanged signals
- No automated D-Bus tests in v1 (requires session bus)

### References

- [Source: epics.md §17] Epic 17: MPRIS & SharedState Integration (Story 17.2)
- [Source: src/mpris.rs §333-360] mpris::init() function — receives cmd_tx, state, enabled
- [Source: src/ui/mod.rs §1768-1792] StateChanged handler — send PlaybackUpdate to MPRIS channel
- [Source: src/mpd/state_machine.rs §69-80] PlaybackUpdate struct
- [Source: architecture.md §681-699] ADR: IPC & CLI Architecture
- [MPRIS Specification] https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html#Method:Player.PropertiesChanged

### Dev Agent Record

#### Agent Model Used

Claude Code (deepseek-v4-flash)

#### Debug Log References

- Deferred from code review of story 14-2 (2026-05-01)
- Review finding: "No PropertiesChanged signal emission — Known deferral. MPRIS clients relying on signal-based reactivity (lock screen, GNOME Shell media) will not auto-update."

#### Completion Notes List

#### File List

- `src/mpris.rs` (edit — add PropertiesChanged emission)
- `src/ui/mod.rs` (edit — add mpris_update_tx channel send in StateChanged handler)
- `src/main.rs` (edit — create mpris_update channel, pass to both sides)
