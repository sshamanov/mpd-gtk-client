# Story 19.1: Auto-Start Support

Status: done

## Story

As a user,
I want the application to start automatically when I log into my desktop,
so that my music client is always ready without manual launch.

## Acceptance Criteria

1. **Given** the user enables auto-start in Settings
   **When** the setting is saved
   **Then** an XDG autostart `.desktop` file is created at `~/.config/autostart/mpd-client.desktop`
   **And** the file contains the correct `Exec=` path pointing to the installed binary
   **And** the file contains `X-GNOME-Autostart-enabled=true`

2. **Given** the user disables auto-start in Settings
   **When** the setting is saved
   **Then** the autostart `.desktop` file is removed
   **And** the application does not start automatically on next login

3. **Given** the auto-start file already exists
   **When** the application is launched by the desktop environment at login
   **Then** the application starts normally with full functionality (no special auto-start mode needed)

4. **Given** the installed binary path changes (update/reinstall)
   **When** auto-start is already enabled
   **Then** the `Exec=` path in the autostart file is updated on next settings save

5. **Given** the `~/.config/autostart/` directory does not exist
   **When** auto-start is enabled
   **Then** the directory is created automatically

## Tasks / Subtasks

- [ ] 1. Add auto_start: bool field to Config struct (AC: 1, 2)
  - [ ] 1.1 Default to false
  - [ ] 1.2 Serialize/deserialize in config load/save
- [ ] 2. Implement XDG autostart file management (AC: 1, 2, 5)
  - [ ] 2.1 Create `~/.config/autostart/` directory if needed
  - [ ] 2.2 Generate .desktop file with correct Exec path
  - [ ] 2.3 Remove .desktop file when auto-start disabled
- [ ] 3. Resolve installed binary path (AC: 1, 4)
  - [ ] 3.1 Use `std::env::current_exe()` for Exec path
  - [ ] 3.2 Update on each settings save (handles path changes)
- [ ] 4. Add Settings toggle UI (AC: 1, 2)
  - [ ] 4.1 Auto-start checkbox in Settings (General section)
  - [ ] 4.2 Wire toggle to config save + autostart file create/remove
- [ ] 5. Verify normal startup (AC: 3)
  - [ ] 5.1 Application works normally when launched via autostart
  - [ ] 5.2 No special auto-start code path needed

## Dev Notes

- **XDG Autostart Specification:** File at `$XDG_CONFIG_HOME/autostart/` (default `~/.config/autostart/`). Format is standard `.desktop` file:
  ```ini
  [Desktop Entry]
  Type=Application
  Name=mpd-client
  Exec=/usr/bin/mpd-client
  X-GNOME-Autostart-enabled=true
  ```
- **Binary path:** Use `std::env::current_exe()` to get the current executable path. This handles the common case where the binary is in PATH.
- **No startup delay:** Set `X-GNOME-AutostartDelay=0` — the app loads quick enough.
- **On uninstall:** The `.desktop` file remains (standard behavior). Not handled by the app.
- **Config integration:** Add `auto_start: bool` (default false) to `Config` in `src/config/mod.rs`. No schema migration needed (new field with default = false).

### Project Structure Notes

- `src/config/mod.rs` — Add auto_start field
- `src/ui/mod.rs` — Settings dialog toggle
- No new source files needed — autostart logic can live in `src/config/mod.rs` or a new utility function in an existing module

### References

- [Source: _bmad-output/planning-artifacts/prd.md §194] — "Auto-start capability when configured by user"
- [Source: _bmad-output/planning-artifacts/prd.md NFR-O2] — "Auto-start capability"
- [Source: _bmad-output/planning-artifacts/architecture.md §589-599] — Configuration Management ADR
- [Source: _bmad-output/planning-artifacts/epics.md §Epic 19] — Epic definition

## Dev Agent Record

### Agent Model Used

TBD

### Debug Log References

### Completion Notes List

### File List
- `src/config/mod.rs` — Add auto_start field, save/load
- `src/ui/mod.rs` — Settings toggle, wire to autostart logic
