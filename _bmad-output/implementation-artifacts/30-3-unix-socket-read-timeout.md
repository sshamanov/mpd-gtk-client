# Story 30.3: Unix Socket Read Timeout

Status: done

## Story

As a developer,
I want Unix socket MPD connections to have read timeouts set for binary cover art reads,
so that hung connections on Unix sockets don't block the MPD cover thread indefinitely.

## Acceptance Criteria

1. **Unix sockets get read timeout**
   - Given an MPD connection via Unix socket
   - When `read_albumart_response()` is called
   - Then `set_read_timeout(Some(Duration::from_secs(5)))` is applied before the read loop
   - And the timeout is restored to 10s after the response completes

2. **TCP behavior unchanged**
   - Given an MPD connection via TCP
   - When `set_read_timeout` is called
   - Then TCP timeout behavior is identical to before

3. **Backward compatibility**
   - Given all existing tests pass
   - When the fix is verified
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Background

From architecture.md: The `MpdStream::set_read_timeout()` implementation for Unix sockets was a no-op (`Ok(())`), meaning binary reads on Unix socket connections could hang indefinitely. TCP connections already had proper timeout support.

### Fix (Already Implemented)

Applied in commit `71ceda65` (2026-05-12). `src/mpd/mod.rs:52-57`:

```rust
fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
    match self {
        MpdStream::Tcp(s) => s.set_read_timeout(timeout),
        MpdStream::Unix(s) => s.set_read_timeout(timeout),  // was: Ok(())
    }
}
```

### Key Files

| File | Change |
|------|--------|
| `src/mpd/mod.rs:56` | Changed `Ok(())` to `s.set_read_timeout(timeout)` for Unix variant |

## Dev Agent Record

### Completion Notes

- Fix applied in commit `71ceda65` (2026-05-12)
- Unix sockets now properly set `SO_RCVTIMEO` before binary reads
- `read_albumart_response()` sets 5s timeout, restores 10s after
- No code changes needed
- All 94 tests pass

## References
- [Source: src/mpd/mod.rs:52-57] `MpdStream::set_read_timeout()` implementation
- [Source: src/mpd/mod.rs:583-594] `read_albumart_response()` timeout usage
- [Source: git 71ceda65] Commit that applied the fix
