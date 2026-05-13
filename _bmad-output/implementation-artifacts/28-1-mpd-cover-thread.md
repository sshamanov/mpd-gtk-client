# Story 28.1: MPD Cover Thread

Status: ready-for-dev

## Story

As a user,
I want cover art fetching to never block MPD command/status operations,
so that the UI stays responsive even when MPD is sending large album art binaries.

## Acceptance Criteria

1. **Separate MPD connection for cover binary data**
   - Given the app is connected to MPD
   - When cover art needs fetching
   - Then a separate TCP connection is opened (not the command/status socket)
   - And the MPD IO thread's socket is never touched for `albumart`/`readpicture`

2. **Channel-based forwarding from MPD IO → MPD Cover**
   - Given the MPD IO thread processes `FetchCovers` command
   - When covers are enqueued
   - Then tuples `(artist, album, uri)` are sent to the MPD Cover thread via channel
   - And the MPD IO thread returns immediately (non-blocking)

3. **On-demand thread lifecycle with 30s idle timeout**
   - Given the MPD Cover thread has been idle for 30 seconds
   - When the idle timeout expires
   - Then the thread closes its MPD connection and terminates
   - And a new connection is created when the next `FetchCovers` arrives

4. **Connection drop on MPD reconnect**
   - Given the main MPD IO connection reconnects (new epoch)
   - When the reconnect happens
   - Then the MPD Cover thread's connection is also dropped
   - And a fresh connection is created for subsequent cover requests

5. **Protocol fallback: albumart → readpicture**
   - Given `albumart <uri> 0` returns error 50 (no embedded art)
   - When the error is received
   - Then `readpicture <uri>` is attempted as fallback
   - And raw JPEG/PNG bytes are forwarded downstream

6. **Backward compatibility**
   - Given existing cover art cache and UI
   - When raw bytes arrive from the MPD Cover thread
   - Then they are delivered to the existing CoverRefreshed event path unchanged
   - And no changes are needed in coverart/provider.rs or ui/widgets/

## Technical Requirements

### Thread Architecture

Per `architecture.md` §2185-2284, the 6-thread model:

```
MPD IO ──channel──→ MPD Cover ──channel──→ Cover Proc
  ↑                    ↑
  │ FetchCovers        │ (artist, album, uri)
  │ command            │ albumart/readpicture
  │                    │ raw JPEG bytes out
```

### MPD Cover Thread Design

- **Location**: `src/mpd/cover.rs` (new module)
- **Thread type**: On-demand (0-1 threads), created when cover work arrives
- **Channel**: `mpsc::sync_channel` with small bound (backpressure on MPD IO if cover queue is full)
- **Job type**: `(String, String, String)` — `(artist, album, uri)` tuples
- **Output**: `AppEvent::CoverRefreshed { album_id, data }` via existing event channel
- **Idle timeout**: 30s — uses `recv_timeout` on the job channel

### MPD Connection

- Opens `TcpStream::connect()` to the same MPD host:port as the command connection
- Reads MPD greeting, sends no idle command
- For each job:
  1. Send `albumart "<uri>" 0\n`
  2. Parse binary size header, collect chunks in offset-loop
  3. If error 50: send `readpicture "<uri>" 0\n`, collect response
  4. Forward raw bytes as `CoverRefreshed` event
- Detect broken connection (read error): reconnect before processing next job

### Integration Points

| File | Change |
|------|--------|
| `src/mpd/cover.rs` | **NEW** — `MpdCoverThread::spawn()` returning `MpdCoverSender` |
| `src/mpd/mod.rs` | Register `pub mod cover;` |
| `src/mpd/state_machine.rs` | Change `FetchCovers` handler: forward to cover thread instead of calling `ActualRead` |
| `src/coverart/actual_read.rs` | Keep but reduce scope — `process_batch()` no longer calls `adapter.albumart_by_uri()` |

### Key Rules

- The MPD IO thread **never** sends `albumart`/`readpicture` on its own socket
- The MPD Cover thread has **no** idle loop, **no** state tracking, **no** queue knowledge
- The MPD Cover thread **never** decodes images (that's Cover Proc's job — story 28-2)
- The `MpdAdapter` methods `albumart()` / `albumart_by_uri()` / `readpicture()` are **moved** to an internal helper in `cover.rs`, not kept on the adapter

## Tasks/Subtasks

- [ ] 1. Create `src/mpd/cover.rs` — `MpdCoverThread` struct with `spawn()` returning `MpdCoverSender`
- [ ] 2. Implement MPD TCP connect + binary protocol (albumart offset-loop, readpicture fallback)
- [ ] 3. Implement 30s idle timeout via `recv_timeout` on job channel
- [ ] 4. Implement reconnection on broken socket detection
- [ ] 5. Add `MpdCommand::CoverThreadShutdown` or use `AtomicBool` for reconnect signal
- [ ] 6. Update `src/mpd/state_machine.rs`: replace inline `ActualRead` calls with channel forwarding
- [ ] 7. Register `pub mod cover;` in `src/mpd/mod.rs`
- [ ] 8. Build and run full test suite — verify zero regressions

## References
- [Source: architecture.md §2185-2284] 6-thread model, MPD Cover thread spec
- [Source: architecture.md §630-648] MPD Cover Fetch Optimizations ADR
- [Source: src/mpd/mod.rs:629-680] Existing albumart/readpicture protocol implementation
- [Source: src/coverart/actual_read.rs] ActualRead — current inline cover fetch

## File List
- `src/mpd/cover.rs` (NEW) — MPD Cover thread: connection, albumart/readpicture, idle timeout
- `src/mpd/mod.rs` — Add `pub mod cover;`
- `src/mpd/state_machine.rs` — Forward FetchCovers to cover thread, drop/shutdown coordination
