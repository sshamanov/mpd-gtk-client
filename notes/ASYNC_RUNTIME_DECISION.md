# Async Runtime Decision

## Context

The MPD client needs concurrent execution for:
1. **MPD protocol handling** — Single TCP connection, text-based protocol (send command, read response)
2. **Compute worker thread** — Background I/O (cover art downloads, search indexing)

## Candidates Compared

### `std::thread` (selected by architecture)

| Metric | Value |
|--------|-------|
| Binary size (simple MPD client) | 4.2 MB |
| Est. full-app binary | ~5–6 MB (with gtk4, ureq, image) |
| Extra dependencies | 0 (uses std library) |
| Build time (cold, MPD-only) | < 0.1s (no deps beyond std) |
| Complexity | Trivial — `thread::spawn`, channels for cross-thread comms |
| MPD protocol fit | Natural — single connection, blocking reads with `BufRead` |

### tokio

| Metric | Value |
|--------|-------|
| Binary size (simple MPD client) | 15 MB |
| Est. full-app binary | ~17–18 MB |
| Extra dependencies | 14 new crates (mio, bytes, socket2, tokio-macros, etc.) |
| Build time (cold, MPD-only) | ~6.5s on top of gtk4 build |
| Complexity | `#[tokio::main]`, `async`/`await`, compatible IO traits |
| MPD protocol fit | Overkill — tokio excels at many concurrent connections, not one |

## Recommendation

**Use `std::thread`** — confirmed by spike results:

1. **No async runtime needed.** The GTK4 main loop is its own event loop. Background work (MPD polling, cover art I/O) is a perfect fit for dedicated `std::thread` workers. Adding tokio would mean running a second event loop alongside GTK's — more complexity, more failure modes.

2. **Binary size.** tokio adds 10.8 MB (+260%) to a trivial binary. In a full app with gtk4, this overhead compounds to ~3x increase vs `std::thread`.

3. **Dependency footprint.** tokio pulls in 14 crate dependencies including `mio` (platform I/O), `bytes` (buffer management), and `socket2` (low-level networking). `std::thread` uses zero extra crates.

4. **MPD protocol is inherently synchronous.** Send a command, read one or more lines, wait for "OK". There is no benefit from async I/O for a single connection with request–response semantics.

## Conclusion

Decision confirmed: **`std::thread` for background workers, GTK main loop for UI, no tokio/async-std.**
