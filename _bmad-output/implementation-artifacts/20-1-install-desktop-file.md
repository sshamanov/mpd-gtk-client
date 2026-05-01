# Story 20.1: Install Desktop File with Proper Categories

Status: done

## Story

As a user,
I want the application to appear in the desktop application menu with a proper icon and category,
so that I can launch it from my desktop environment's app launcher.

## Acceptance Criteria

1. **Desktop file created on first run**
   - **Given** the application is launched for the first time
   - **When** startup completes
   - **Then** a `.desktop` file is installed at `~/.local/share/applications/mpd-client.desktop`
   - **And** the file contains `Categories=Audio;Music;Player;`
   - **And** the `Exec=` path points to the installed binary (via `std::env::current_exe()`)
   - **And** `Name=mpd-client`, `Type=Application`, `Terminal=false`

2. **Icon reference**
   - **Given** the desktop file is installed
   - **When** the desktop environment reads it
   - **Then** `Icon=mpd-client` is set (icon must be installed separately or omitted)

3. **MIME type registration**
   - **Given** the desktop file is installed
   - **When** the user opens an audio file in the file manager
   - **Then** "Open with mpd-client" appears in the context menu
   - **And** the `MimeType=` field lists common audio MIME types

4. **Update on binary path change**
   - **Given** the binary path changes after an update
   - **When** the application starts
   - **Then** the `Exec=` path in the desktop file is updated

## Dev Notes

- **Install path:** `~/.local/share/applications/mpd-client.desktop` (per-user, follows XDG spec).
- **Different from autostart:** This is a system menu entry, not the autostart file in `~/.config/autostart/` (story 19-1). Both are needed for full desktop integration.
- **MIME types:** `audio/flac;audio/mpeg;audio/ogg;audio/wav;audio/x-flac;audio/flac;`
- **Categories:** Use `Audio;Music;Player;` per freedesktop.org spec.
- **No icon file:** In v1, omit `Icon=` or point to a bundled icon. Icon installation is a separate concern.
- **Install on startup:** Check if the desktop file exists at startup. If not, create it. This handles first-run and update cases.

## References
- [Source: PRD §194] Desktop integration — desktop entry with proper categories
- [Source: PRD NFR-O2] Auto-start capability
- [Source: architecture.md §724] Build & Packaging Architecture — desktop integration requirements
- [XDG Desktop Entry Spec] https://specifications.freedesktop.org/desktop-entry-spec/latest/

## File List
- `src/config/mod.rs` (new `install_desktop_file()` method or alongside autostart)
- `src/main.rs` (call at startup if not present)
