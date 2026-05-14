# Story 30.2: Readpicture Version Check

Status: done

## Story

As a developer,
I want the MPD version check for `readpicture` to use the correct minimum version (0.22, not 0.24),
so that MPD 0.22 and 0.23 servers can serve embedded cover art via readpicture.

## Acceptance Criteria

1. **Correct minimum version**
   - Given an MPD server running version 0.22 or 0.23
   - When `MpdCapabilities::from_version()` is called
   - Then `supports_readpicture()` returns `true`
   - And `readpicture` fallback is available for these versions

2. **Backward compatibility**
   - Given all existing tests pass
   - When the fix is verified
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Background

From architecture.md §642-644: `readpicture` was added in MPD 0.22, not 0.24. The version check `self.minor >= 24` incorrectly disabled readpicture for MPD 0.22 and 0.23.

### Fix (Already Implemented)

Applied in commit `71ceda65` (2026-05-12). `src/mpd/mod.rs:256`:

```rust
fn supports_readpicture(&self) -> bool {
    self.major >= 1 || (self.major == 0 && self.minor >= 22)
}
```

Previously was `self.minor >= 24`.

### Key Files

| File | Change |
|------|--------|
| `src/mpd/mod.rs:256` | Changed `>= 24` to `>= 22` |

## Dev Agent Record

### Implementation Plan

Verification-only: confirm fix via git blame.

### Completion Notes

- Fix applied in commit `71ceda65` (2026-05-12)
- `supports_readpicture()` correctly uses `minor >= 22`
- No code changes needed
- All 94 tests pass

## References
- [Source: architecture.md §642-644] Readpicture version check ADR
- [Source: src/mpd/mod.rs:255-257] `supports_readpicture()` implementation
- [Source: git 71ceda65] Commit that applied the fix
