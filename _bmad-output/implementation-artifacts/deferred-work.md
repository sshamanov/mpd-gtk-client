## Scoped Work

Items planned for the release — may be in-progress or pending.

### Epics (in sprint backlog)

| Epic | Story | What |
|------|-------|------|
| epic 25 | 25-1-mpd-idle-protocol | Replace 500ms polling with MPD `idle`/`noidle` protocol + `TcpStream::try_clone()` |
| epic 26 | 26-1-metadata-caching | Pull album metadata into local cache for fast sorting/grouping without MPD round-trips |
| epic 27 | 27-1-responsive-right-rail | MultiLayoutView + BottomSheet for responsive right rail on narrow windows |
| epic 20 | 20-1-install-desktop-file | `.desktop` file installation |
| epic 21 | 21-1-performance-profiling | Profiling tool integration for large libraries |

### Architecture — designed, not yet implemented

- **MPD idle protocol** (same as epic 25)
- **Centralized KeybindingService** — compile-time conflict detection; currently ad-hoc GTK accelerators
- **Undo stack** — queue mutation undo
- **Runtime theme switcher** — user-facing theme toggle (dark/light)
- **CoverProvider + ActualRead pipeline refinement** — two-layer architecture (CoverProvider index + ActualRead fetch queue) is implemented; potential enhancements: multi-source cover aggregation, configurable cache eviction

### Bugs & Gaps

- **Queue key handler stale `item_ids`** — race window on rapid Shift+Up/Down
- **`batch_populate` runs synchronously on GTK thread** — large libraries may freeze UI
- **`search_albums` stale artist across tracks** — partial fix in commit 68f3593, edge cases remain
- **`MpdAdapter` has no `Drop`** — no clean MPD close on shutdown
- **SIGINT cleanup via `process::exit(0)`** — skips graceful MPD disconnect
- **D-Bus session bus reconnection** — MPRIS silently stops if D-Bus restarts

## Deferred from: code review of 40-1-layout-profile-export-import (2026-05-16)

- `LayoutProfile` fields (`split_ratio`, `rail_width`, proportions) are never consumed by layout code — import saves to config but doesn't apply changes to running UI (AC 3); no existing layout update path to wire into

## Deferred from: code review of 28-4-notification-router (2026-05-13)

- `event.clone()` on every MpdEvent wastes memory — clones large variants (CoverRefreshed JPEG, AlbumTracks) that the router ignores; only Toast/Connected/Disconnected needed
- Router thread never joined on shutdown — `notif_stop` signaled but no `JoinHandle` available for `join()`
- Toast channel (256) sizing undocumented vs event channel (1024) and search channel (64); rationale unclear
- Toast timeout values (3s/5s) hardcoded in match arm in ui/mod.rs, not shared as constants with ToastLevel docs
- `reduce()` referenced in notifications/mod.rs doc comment but doesn't exist in codebase — architecture-speak leak

## Deferred from: code review of 28-3-search-worker-thread (2026-05-13)

- Unconditional MPD fallback alongside local search worker — doubled MPD traffic per keystroke, result race between local and MPD SearchResults
- Race window between Reset and BuildIndex on reconnect — user search between Connected/Albums events returns empty
- 500ms recv_timeout command latency — follows Cover Proc pattern, debounce masks it
- Event channel saturation from SearchResults — try_send drops when channel full (pre-existing pattern)
- No generation counter on SearchResults — stale results from slow queries can overwrite fresh ones

## Deferred from: code review of 28-2-cover-proc-worker (2026-05-13)

- Multiple Cover Proc workers during reconnection window — index.json read-modify-write not atomic across threads, can lose cache entries
- MPD Cover thread blocking send can stall permanently if Cover Proc panics — no timeout on send, no panic detection
- Non-JPEG embedded cover art may enter delete-recycle loop via CoverProvider::is_valid_jpeg rejecting non-JPEG files cached as .jpg
- Orphaned {md5}.jpg files accumulate when cover art changes — no GC or LRU eviction
- update_index_json called on failed fs::write — transient cache index inconsistency
- No size guard on JPEG decode — large images can cause OOM in Cover Proc via intermediate RGBA buffer before resize
- CoverProvider RwLock poison silently disables cache I/O — if let Ok pattern skips silently with no recovery
- try_send event drops invisible to caller — CoverPaths/CoverRefreshed can be silently dropped when channel full
- Corrupt index.json silently resets entire cache — unwrap_or_default() replaces all entries with empty map
- **Config loaded twice during startup** — I/O waste on TOML file
- **BackSpace at root sends `ListDirectory("")`** — harmless but wrong; should be no-op

## Deferred from: code review of 29-1-image-crate-migration (2026-05-13)

- `image::open` decodes full image before resize, unlike `Pixbuf::from_file_at_size` which decoded at target resolution — can cause OOM for very large cached cover files (pre-existing concern, already tracked as "No size guard on JPEG decode")
- `image::open` decode failures silently swallowed by `if let Ok` — no diagnostic log; same as old Pixbuf code but missed opportunity to add `log::warn!`

---

## Out of Scope (Historical)

Items explicitly excluded from the release, kept for reference.

### Design Decisions

- **Three-tier session persistence** — only last active mode persists; full session restore/window geometry out of scope
- **User-configurable keybindings** — compile-time mapping only; runtime customization out of scope
- **Flatpak/CI/.deb/.rpm packaging** — build via `cargo build`; no packaging pipeline
- **Plugin/extension system** — all functionality compiled in; no dynamic loading
- **i18n/l10n** — English-only UI; no gettext/fluent framework
- **RTL language support** — layout testing for Arabic/Hebrew not planned
- **Touchscreen hover controls** — CSS hover overlays don't work on touch
- **Workspace sub-crates** — single crate; may split when module boundaries proven
- **System tray icon** — inconsistent across Linux DEs; MPRIS provides equivalent

### Resolved (fixed in code)

| Item | Resolution | Date |
|------|------------|------|
| Dead MPD connection never triggers reconnect | 3 consecutive failures → reconnect | 2026-04-29 |
| `MpdEvent::Reconnected` dead code | Variant removed | 2026-04-29 |
| Settings changes require app restart | Live reconnect via `MpdCommand::Reconnect` | 2026-04-29 |
| Search index not rebuilt on library update | Index rebuild lifecycle defined | 2026-04-29 |
| Cover art `set_cover_path` dead code | Full pipeline + widget registry designed | 2026-04-29 |
| Toast auto-dismiss race | Generation counter fix | 2026-04-29 |
| `search_albums` misses `AlbumArtist` tag | Commit 68f3593 | 2026-04-29 |
| Grouped-mode double-click plays wrong album | Cannot reproduce, fixed | 2026-05-07 |
| Cue/DSD rows clickable no-ops | Fixed (PlayUris, AddUris, Play Next for CUE) | 2026-05-07 |
| 30ms timer starves GTK main loop | Replaced with frame clock callback | 2026-05-07 |
| `list_albums_grouped` loses artist for Date/Genre | Cannot reproduce, fixed | 2026-05-07 |
| `InsertNext` uses stale `current_pos` | Cannot reproduce, fixed | 2026-05-07 |
| MPRIS PropertiesChanged signal emission | Emitter exists (ui/mod.rs:318, 2089) | 2026-05-07 |
| SharedState playback fields never populated | Now populated (ui/mod.rs:2061-2077) | 2026-05-07 |

### Skipped (negligible impact)

| Item | Reason |
|------|--------|
| `ExponentialBackoff` derive(Clone) is misleading | Never actually cloned; maintenance risk only |
| Unicode symbols in GTK labels may not render | All modern Linux systems render them |
