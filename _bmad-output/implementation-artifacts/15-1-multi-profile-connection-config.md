# Story 15.1: Multi-Profile Connection Config

Status: done

## Story

As a user with multiple MPD instances (local, NAS, work),
I want to define named connection profiles in the config,
so that I can switch between MPD servers without re-entering connection details.

## Acceptance Criteria

1. **Config format supports multiple profiles**
   - **Given** the config file contains multiple profile sections
   - **When** the application reads the config
   - **Then** all profiles are parsed and available for selection
   - **And** the `default_profile` key determines the initial connection
   - **And** the `last_profile` key is updated on each manual profile switch

2. **Profile-based connection**
   - **Given** a profile defines a Unix socket path
   - **When** connecting via that profile
   - **Then** the adapter uses the Unix socket path directly (skips TCP auto-detection)
   - **Given** a profile defines a TCP host:port
   - **When** connecting via that profile
   - **Then** the adapter connects via TCP to the specified host:port

3. **Backward compatibility**
   - **Given** no profiles section is defined in config (existing config format)
   - **When** the application starts
   - **Then** the existing single-host behavior is preserved
   - **And** the legacy `mpd_host`/`mpd_port` fields are used as before

4. **Auto-save discovered connection**
   - **Given** Unix socket auto-detection succeeds on first connect
   - **When** no `default_profile` is set
   - **Then** the discovered connection is saved as the "default" profile
   - **And** the config is written with the new profiles section

## Tasks / Subtasks

- [ ] (AC: 1) Update `Config` struct to support `profiles` section with named profile entries
- [ ] (AC: 1) Add `default_profile` and `last_profile` to `[general]` section
- [ ] (AC: 2) Update `MpdAdapter::connect()` to accept profile parameters (socket path or host:port)
- [ ] (AC: 3) Maintain backward compatibility — `mpd_host`/`mpd_port` used when no profiles exist
- [ ] (AC: 4) Auto-save discovered connection as "default" profile
- [ ] (AC: 1) Wire profile selection through `MpdCommand::Reconnect` to use new profile params

## Dev Notes

- **Config schema:**
  ```toml
  [profiles.local]
  host = "/run/mpd/socket"

  [profiles.nas]
  host = "192.168.1.100"
  port = 6600

  [general]
  default_profile = "local"
  last_profile = "local"
  ```
- **MpdAdapter::connect update:** Accept either `(host: &str, port: u16)` for TCP or `(socket_path: &Path)` for Unix socket. Currently supports auto-detection — add direct socket path option.
- **Profile not found:** If `default_profile` references a profile that doesn't exist, fall back to auto-detection and log a warning.
- **No profile editing UI in v1:** Profiles are hand-edited in TOML. The settings dialog only offers selection from existing profiles.
- **Config migration:** If migrating from legacy config (no profiles), the existing `mpd_host`/`mpd_port` are treated as an implicit profile.

### Source Files to Touch
- `src/config/mod.rs` — Add profile data structures, parsing, migration
- `src/mpd/mod.rs` — Update connect to accept profile params, add socket path connect
- `src/mpd/state_machine.rs` — Update reconnect to carry profile info
- `src/main.rs` — Pass profile selection through startup chain

### Testing
- Unit test for parsing profiles from TOML
- Unit test for backward compatibility with legacy config
- Integration test for profile-based connection
- Edge cases: missing profile, invalid profile host, migration from legacy

## References

- [Source: architecture.md §448] Multi-Profile Connections — Profile config format and behavior
- [Source: epics.md §15] Epic 15: Connection Profiles

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- Backward compatible: existing configs continue to work without changes
- Profile editing UI is deferred — hand-edit TOML only in v1

### File List

- `src/config/mod.rs`
- `src/mpd/mod.rs`
- `src/mpd/state_machine.rs`
- `src/main.rs`
