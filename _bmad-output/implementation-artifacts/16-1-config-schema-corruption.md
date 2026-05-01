# Story 16.1: Config Schema Version and Corruption Recovery

Status: done

## Story

As a developer,
I want the config file to have a schema version with automatic corruption recovery,
so that future config changes are migratable and corrupt configs don't break the application.

## Acceptance Criteria

1. **Schema version field**
   - **Given** the config file has a `schema_version` field that is lower than the current version
   - **When** the config is loaded
   - **Then** a migration function is applied to update the config to the current version
   - **And** the migrated config is written back to disk

2. **Corruption recovery**
   - **Given** the config file is corrupt (invalid TOML, truncated, or unreadable)
   - **When** the config loader attempts to parse it
   - **Then** the corrupt file is backed up to `config.toml.bad`
   - **And** a fresh default config is created
   - **And** a warning is logged with the backup path
   - **And** the application starts normally with defaults

3. **Unversioned migration**
   - **Given** the config file is valid but missing the `schema_version` field
   - **When** the config is loaded
   - **Then** the schema version defaults to 0
   - **And** all migrations from version 0 to current are applied sequentially

4. **Backup restoration note**
   - **Given** a corrupt config was backed up
   - **When** the application logs the warning
   - **Then** the message includes the path to the backup file
   - **And** advises the user to restore values manually

## Tasks / Subtasks

- [ ] (AC: 1) Add `schema_version: u32` field to `Config` struct (default 0 for unversioned)
- [ ] (AC: 1) Define migration system: `Vec<fn(&mut Config)>` indexed by version
- [ ] (AC: 1, 3) Implement migration runner that applies migrations sequentially from current to target
- [ ] (AC: 2) Implement corruption detection when TOML parsing fails
- [ ] (AC: 2) Implement backup: copy corrupt file to `<path>.bad`
- [ ] (AC: 2) Implement fallback: create and save default config
- [ ] (AC: 1) Write migrated config back to disk after successful migration

## Dev Notes

- **Current version starts at 1.** Version 0 means "unversioned" (migration target). Version 1 is the first versioned schema.
- **Migration functions** are added as new versions are introduced. Each function takes a mutable `Config` reference and transitions it from version N to N+1.
- **Backup path:** Same directory as config, with `.bad` extension. File is renamed, not copied (atomic, preserves original for recovery).
- **Configuration integrity is non-critical** — the config is only preferences. The app must always start, even with corrupted config. This is the guiding principle for fallback behavior.
- **Log warning example:** `"Config file corrupt, backed up to ~/.config/mpd-client/config.toml.bad. Using defaults."`

### Source Files to Touch
- `src/config/mod.rs` — Add schema version, migration system, backup on corruption

### Testing
- Unit test: parse versioned config
- Unit test: migration from version 0 to current
- Unit test: corrupt TOML triggers backup + fallback
- Unit test: corrupt file is properly backed up
- Edge cases: empty file, binary garbage, partial truncation

## References

- [Source: architecture.md §589] Configuration Management — Schema version and corruption recovery
- [Source: epics.md §16] Epic 16: Infrastructure & Code Quality

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- Migration system uses function vector indexed by version
- Current schema version: 1 (first versioned schema, same as current config structure)

### File List

- `src/config/mod.rs`
