# Story 29.3: Now-Playing Consolidation

Status: ready-for-dev

## Story

As a developer,
I want all now-playing metadata to flow through a single centralized handler that writes SharedState first, updates UI widgets second, and forwards to MPRIS third,
so that there is one canonical source of truth, format parsing is unified, and CurrentSong merges with cached playback status instead of sending partial data.

## Acceptance Criteria

1. **Single entry point `handle_now_playing(PlaybackUpdate)`**
   - Given a `StateChanged` or `CurrentSong` event produces a `PlaybackUpdate`
   - When `handle_now_playing()` is called
   - Then it writes `SharedState.current` first, reads back into a `PlaybackDisplay` view model, updates all GTK now-playing widgets, and forwards the update to the MPRIS emitter
   - And the inlined SharedState population and MPRIS forwarding at the call site (~lines 2045-2077) are removed

2. **Unified audio format parser `parse_mpd_audio_format()`**
   - Given MPD status and currentsong responses
   - When `parse_mpd_audio_format(songs_audio, songs_format, status_audio)` is called
   - Then it checks fields in priority order: `currentsong.Audio` (file format, immutable) → `currentsong.Format` (file sample spec) → `status.audio` (DAC output, may be resampled)
   - And returns `Option<AudioFormat>` with file format winning over DAC format (DSD file shows "DSD64" even if MPD converts to PCM)
   - And `AudioFormat::display_text()` produces the badge text (e.g., "24/96 · FLAC", "DSD128")

3. **`AudioFormat` struct added to `PlaybackUpdate`**
   - Given a `PlaybackUpdate` is constructed from MPD data
   - When the update is sent
   - Then `PlaybackUpdate` includes an `audio_format: Option<AudioFormat>` field
   - And `Track.format` in `SharedState.current.track` is populated from `AudioFormat::display_text()` instead of always `None`

4. **`CurrentSong` merges with cached status**
   - Given a `CurrentSong(PlaybackUpdate)` event arrives (track change initiated by external MPD client)
   - When the handler processes it
   - Then it merges the fresh song metadata with cached status fields (state, elapsed, duration, volume, playlist_version) from the last `Status` response
   - And sends the merged `PlaybackUpdate` through `handle_now_playing()` (same path as `StateChanged`)
   - And the separate `parse_song_update()` path that resets elapsed to `--:--` is removed

5. **Cover art remains independent**
   - Given a `CoverPaths` or `CoverRefreshed` event arrives
   - When the now-playing cover widget needs updating
   - Then the cover widget is updated directly via the existing CoverPaths/CoverRefreshed handler
   - And cover arrival does NOT re-trigger `handle_now_playing()`
   - And `SharedState.current.album.cover_path` is NOT populated from now-playing cover events

6. **Backward compatibility**
   - Given the now-playing section is displayed
   - When the consolidation is applied
   - Then title, artist, album, year, position, duration, seekbar, format badge, bitrate, play/pause, and status dot all display identically
   - And the bottom panel now-playing mini-display is unchanged
   - And MPRIS PropertiesChanged emission is unchanged
   - And cover art loading from disk (now-playing section) works identically

## Technical Requirements

### Background

Two ADRs in architecture.md define the now-playing consolidation:

**ADR 1** (architecture.md §650-664): Establish `update_now_playing(PlaybackUpdate)` as the single entry point. Cover independent through CoverPaths/CoverRefreshed. Year normalized and displayed inline with artist ("Artist · 2024").

**ADR 2** (architecture.md §795-818): Go further — `handle_now_playing()` writes SharedState first (canonical), reads back to UI via `PlaybackDisplay` view model, forwards to MPRIS. Unified `parse_mpd_audio_format()` with prioritized MPD field sources. Structured `AudioFormat` populates `Track.format`. CurrentSong merges with cached status.

The `update_now_playing()` function exists (lines 2730-2810) and updates GTK widgets directly, but:
- SharedState population is inlined at the call site (lines 2045-2069) — not inside the function
- MPRIS forwarding is separate
- No `PlaybackDisplay` view model
- `Track.format` is always `None`
- `CurrentSong` handler sends unmerged partial data
- No `parse_mpd_audio_format()` helper

### New Types

```rust
/// Parsed audio format with sample rate, bit depth, and codec.
struct AudioFormat {
    sample_rate: u32,     // e.g., 96000
    bit_depth: u16,       // e.g., 24
    codec: String,        // e.g., "FLAC", "DSD128"
    is_dsd: bool,
}

impl AudioFormat {
    /// Produce the badge text: "24/96 · FLAC" or "DSD128"
    fn display_text(&self) -> String { ... }
}

/// View model for now-playing display — derived from SharedState.current.
struct PlaybackDisplay {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    year: Option<String>,
    state: String,
    elapsed: Option<f64>,
    duration: Option<f64>,
    format_badge: Option<String>,
    bitrate: Option<String>,
    volume: i16,
    song_id: Option<u32>,
}
```

### Changed Types

```rust
// PlaybackUpdate gains:
pub struct PlaybackUpdate {
    // ... existing fields ...
    pub audio_format: Option<AudioFormat>,  // NEW
}
```

### `parse_mpd_audio_format()` Algorithm

```rust
fn parse_mpd_audio_format(
    songs_audio: Option<&str>,   // MPD currentsong "Audio" — file format, immutable
    songs_format: Option<&str>,  // MPD currentsong "Format" — file sample spec
    status_audio: Option<&str>,  // MPD status "audio" — DAC output, may be resampled
) -> Option<AudioFormat> {
    // Priority: songs_audio > songs_format > status_audio
    // File format wins over output format
    // Parse format strings like "44100:24:2", "dsd64", "dsd128:2"
    // Return None only when all three sources are None or unparseable
}
```

### `handle_now_playing()` Flow

```
fn handle_now_playing(
    state: &SharedState,
    update: PlaybackUpdate,
    npw: &NowPlayingWidgets,
    mpris_tx: &std::sync::mpsc::Sender<PlaybackUpdate>,
) {
    // 1. WRITE SharedState.current (canonical)
    state.write().current = build_current_context(&update);

    // 2. READ back into PlaybackDisplay view model
    let display = PlaybackDisplay::from_state(&state.read().current, &update);

    // 3. UPDATE all GTK now-playing widgets from display
    update_now_playing(npw, &display);

    // 4. FORWARD to MPRIS emitter
    let _ = mpris_tx.send(update);
}
```

### SharedState Population

```rust
// Track.format is now populated:
app_state.current.track = Some(Track {
    // ... existing fields ...
    format: update.audio_format.as_ref().map(|f| f.display_text()),
});
```

### CurrentSong Merge

```rust
MpdEvent::CurrentSong(song_update) => {
    // Merge fresh song metadata with cached status
    let merged = merge_with_cached_status(song_update, &cached_status);
    handle_now_playing(&state, merged, &npw, &mpris_tx);
}
```

Where `merge_with_cached_status` takes the fresh song's title/artist/album and the last `status` response's state/elapsed/duration/volume/playlist_version.

### Integration Points

| File | Change |
|------|--------|
| `src/ui/mod.rs` | Add `handle_now_playing()`, `PlaybackDisplay`, `parse_mpd_audio_format()`; refactor `update_now_playing()` to take `PlaybackDisplay`; move SharedState write into handler; add CurrentSong merge logic; remove inlined SharedState population at call site |
| `src/mpd/state_machine.rs` | Add `AudioFormat` struct; add `audio_format: Option<AudioFormat>` to `PlaybackUpdate`; add `parse_mpd_audio_format()` or parse at event construction site |
| `src/state/mod.rs` | Potentially add `format` field population in `Track` (already has `format: Option<String>`) |

### Key Rules

- **Cover is independent** — now-playing cover updates flow through CoverPaths/CoverRefreshed directly to `np_cover` widget, not through `handle_now_playing()`
- **Write-then-read pattern** — `handle_now_playing()` writes SharedState first, then reads back. This ensures SharedState is canonical and any other reader sees consistent state.
- **Year normalization** — `normalize_year()` splits on `-` to extract year from full dates like "2024-03-15"
- **Year inline display** — "Artist · 2024" with middle-dot separator in the now-playing artist label (already partially implemented)
- **Format badge** — center text in the info row: "24/96 · FLAC" or "DSD128"
- **Do NOT touch** — the bottom panel now-playing mini-display (lines 2037-2042), MPRIS emitter wiring, CoverPaths/CoverRefreshed handlers for the grid

### What Gets Consolidated

| Before | After |
|--------|-------|
| `update_now_playing()` receives raw `PlaybackUpdate` | Receives `PlaybackDisplay` view model derived from SharedState |
| SharedState populated at call site (lines 2045-2077) | SharedState written inside `handle_now_playing()` |
| `Track.format` always `None` | Populated from `AudioFormat::display_text()` |
| CurrentSong sends partial `PlaybackUpdate` | CurrentSong merges with cached status, goes through `handle_now_playing()` |
| MPRIS forward is separate from widget update | Both happen in `handle_now_playing()` |
| Format string parsed ad-hoc in `update_now_playing()` | `parse_mpd_audio_format()` is the single parser |

## Tasks/Subtasks

- [ ] 1. Add `AudioFormat` struct and `parse_mpd_audio_format()` helper in `src/mpd/state_machine.rs` (or `src/mpd/mod.rs`)
  - [ ] 1.1 Define `AudioFormat` with `sample_rate`, `bit_depth`, `codec`, `is_dsd` fields
  - [ ] 1.2 Implement `AudioFormat::display_text()` — "24/96 · FLAC", "DSD128", etc.
  - [ ] 1.3 Implement `parse_mpd_audio_format(songs_audio, songs_format, status_audio) -> Option<AudioFormat>`
  - [ ] 1.4 Add unit tests for format parser (DSD64, DSD128, 24/96 FLAC, 16/44.1, unparseable, all None)
- [ ] 2. Add `audio_format: Option<AudioFormat>` field to `PlaybackUpdate`
  - [ ] 2.1 Populate `audio_format` where `PlaybackUpdate` is constructed from MPD status/currentsong responses
  - [ ] 2.2 Pass the three MPD fields to `parse_mpd_audio_format()` during construction
- [ ] 3. Add `PlaybackDisplay` view model struct in `src/ui/mod.rs`
  - [ ] 3.1 Derive fields from `SharedState.current` + `PlaybackUpdate` state/elapsed/duration/volume
- [ ] 4. Implement `handle_now_playing()` as single entry point
  - [ ] 4.1 Write SharedState.current (track + album, with format populated)
  - [ ] 4.2 Build PlaybackDisplay from SharedState
  - [ ] 4.3 Call update_now_playing() with PlaybackDisplay
  - [ ] 4.4 Forward to MPRIS emitter
  - [ ] 4.5 Remove inlined SharedState population at call site (lines ~2045-2069)
- [ ] 5. Refactor `update_now_playing()` to take `PlaybackDisplay` instead of raw `PlaybackUpdate`
  - [ ] 5.1 Remove direct `update.field` access — use `display.field` instead
- [ ] 6. Implement CurrentSong merge with cached status
  - [ ] 6.1 Cache last `Status` fields (state, elapsed, duration, volume, playlist_version)
  - [ ] 6.2 On `CurrentSong(PlaybackUpdate)`, merge fresh song metadata with cached status fields
  - [ ] 6.3 Route merged update through `handle_now_playing()`
- [ ] 7. Build and run full test suite — verify zero regressions, zero warnings

## Dev Agent Record

### Implementation Plan
(To be filled by dev agent)

### Completion Notes
(To be filled by dev agent)

### Change Log
(To be filled by dev agent)

## References
- [Source: architecture.md §650-664] ADR 1 — Now-Playing Consolidation (single entry point, cover independent, year normalization)
- [Source: architecture.md §795-818] ADR 2 — Now-Playing Consolidation (SharedState canonical, AudioFormat, parse_mpd_audio_format, CurrentSong merge, PlaybackDisplay)
- [Source: src/ui/mod.rs:2730-2810] `update_now_playing()` — current widget update function
- [Source: src/ui/mod.rs:2713-2728] `NowPlayingWidgets` — widget reference struct
- [Source: src/ui/mod.rs:2005-2077] Call site — StateChanged handler with inlined SharedState population
- [Source: src/ui/mod.rs:2037-2042] Bottom panel now-playing mini-display — DO NOT TOUCH
- [Source: src/mpd/state_machine.rs:102-116] `PlaybackUpdate` struct — add `audio_format` field
- [Source: src/mpd/state_machine.rs:22-61] `MpdCommand` enum — CurrentSong, Status, StateChanged flow
- [Source: src/state/mod.rs:24-27] `CurrentContext` — track and album fields in SharedState

## File List
- `src/mpd/state_machine.rs` — Add `AudioFormat` struct, `parse_mpd_audio_format()`, `audio_format` field on `PlaybackUpdate`, CurrentSong merge logic
- `src/ui/mod.rs` — Add `PlaybackDisplay`, `handle_now_playing()`, merge logic; refactor `update_now_playing()`; remove inlined SharedState population
- `src/state/mod.rs` — Verify `Track.format: Option<String>` is suitable for `AudioFormat::display_text()` output
