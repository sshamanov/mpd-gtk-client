# Build Notes — MPD Client Spike

## System Requirements

- **OS:** Arch Linux (testing ground; build commands vary by distro)
- **GTK4 runtime:** 4.22.2 (installed via `pacman`)
- **Rust:** 1.93.0 (MSRV: 1.85)
- **Edition:** 2024

## System Dependency Installation

### Arch Linux
```bash
sudo pacman -S gtk4 pkgconf
```
GTK4 4.14.2+ is required (runtime). Build host runs 4.22.2 — code targeting only `v4_14` APIs is portable to any ≥4.14 runtime.

### Other Distros
| Distribution | Command |
|-------------|---------|
| Fedora | `sudo dnf install gtk4-devel pkgconfig` |
| Debian/Ubuntu | `sudo apt install libgtk-4-dev pkg-config` |
| openSUSE | `sudo zypper install gtk4-devel pkg-config` |

No OpenSSL required — `ureq` uses `rustls` (pure Rust TLS).

## Project Creation

```bash
cargo new mpd-client-spike
cd mpd-client-spike
# Edition "2024" is auto-selected by cargo new with Rust 1.93
```

No `build.rs` needed — `gtk4-sys` discovers system libraries via pkg-config automatically.

## Dependencies

```toml
[dependencies]
gtk4 = { version = "0.11", features = ["v4_14"] }
glib = "0.20"
gdk-pixbuf = "0.20"
ureq = { version = "3", default-features = false, features = ["rustls", "gzip"] }
rustls = "0.23"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1.1"
dirs = "6"
log = "0.4"
env_logger = "0.11"
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
unicode-normalization = "0.1"
thiserror = "2"
```

## Build Times

| Build type | Time |
|-----------|------|
| Cold cache (full) | **51 s** |
| Incremental (no change) | **0.04 s** |
| Incremental (add deps) | **6–10 s** |

Cold cache fetched and compiled 79 crate dependencies from scratch.

Target directory size after full build: **761 MB** (includes debug symbols; release build would be smaller).

## Gotchas

### gtk4-rs 0.8 → 0.11 API Changes
| Change | Old API | New API |
|--------|---------|---------|
| `set_title` signature | `set_title("title")` | `set_title(Some("title"))` — now takes `Option<&str>` |

### ureq Version
The architecture draft specified `ureq 4` but **ureq 4 has not been released**. Using ureq 3.3.0 instead. Feature name changed from `tls` to `rustls`. SPIKE VERIFICATION NOTE: Also needed `gzip` feature for compressed API responses — enabled in reference manifest.

### Unused Dependencies
- `gdk-pixbuf` compiles but may be optional if GTK4's internal pixbuf loader is sufficient
- `env_logger` requires explicit initialization (`env_logger::init()`) — silent otherwise

### Binary Size Impact (debug mode)

| Binary | Size | Dependencies |
|--------|------|-------------|
| GTK4 window only | 4.2 MB | gtk4 + glib |
| `std::thread` MPD client | 4.2 MB | std only |
| tokio MPD client | 15 MB | 13 extra crates |

The tokio test confirmed the architecture's `std::thread` decision is correct — tokio adds 3.6× binary size with no benefit for a single-connection text protocol.

## Spiked Files

```
/tmp/mpd-client-spike/
├── Cargo.toml                   # Working manifest (model for real project)
├── Cargo.lock                   # Resolved dependency tree
├── src/
│   ├── main.rs                  # GTK4 window spike
│   └── bin/
│       └── mpd-std-thread.rs    # MPD connection spike (std::thread)
├── ASYNC_RUNTIME_DECISION.md    # Runtime comparison and decision
├── BUILD_NOTES.md               # This file
└── target/                      # Build artifacts (761 MB)
```
