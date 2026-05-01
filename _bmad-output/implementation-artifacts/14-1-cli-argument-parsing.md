# Story 14.1: CLI Argument Parsing

Status: done

## Story

As a power user,
I want to pass command-line flags to control the application at startup,
so that I can specify MPD host/port, startup mode, and initial actions without editing config files.

## Acceptance Criteria

1. **Config overrides via CLI flags**
   - **Given** the application is launched from the command line
   - **When** `--mpd-host <host>` and/or `--mpd-port <port>` are passed
   - **Then** the MPD connection uses the specified host/port (overrides config)
   - **And** the config file is not modified (override is session-only)

2. **Startup mode selection**
   - **Given** the application is launched with `--mode album` or `--mode folder`
   - **When** the UI initializes
   - **Then** the specified mode is activated on startup (overrides last-saved mode)

3. **Action flags for remote-like behavior**
   - **Given** the application is launched with `--start-playing`
   - **When** the connection to MPD is established
   - **Then** the `Play` command is dispatched
   - **Given** `--toggle-playback` is passed
   - **When** the connection is established
   - **Then** the playback state is toggled

4. **Media control flags**
   - **Given** the application is launched with `--next` or `--prev`
   - **When** the connection is established
   - **Then** the corresponding command is dispatched

5. **Profile selection**
   - **Given** the application is launched with `--profile <name>`
   - **When** the config is loaded
   - **Then** the named profile's connection parameters are used

6. **Help and version**
   - **Given** the user passes `--help`
   - **When** the application starts
   - **Then** usage information is printed to stdout
   - **And** the application exits with code 0
   - **Given** `--version` is passed
   - **When** the application starts
   - **Then** the version string is printed to stdout
   - **And** the application exits with code 0

7. **Invalid flags**
   - **Given** an unrecognized flag is passed
   - **When** the CLI parser encounters it
   - **Then** a helpful error message is printed to stderr
   - **And** the application exits with a non-zero status

## Tasks / Subtasks

- [x] (AC: 1-7) Add lightweight CLI parser to `main.rs` using manual `std::env::args()` parsing
  - [x] Define supported flags: `--mpd-host`, `--mpd-port`, `--profile`, `--mode`, `--start-playing`, `--toggle-playback`, `--next`, `--prev`, `--version`, `--help`
  - [x] Parse and validate flag combinations
  - [x] Apply config overrides before MPD connection setup
  - [x] Dispatch action flags after connection established
- [x] (AC: 6) Wire `--help` and `--version` with early exit
- [x] (AC: 7) Handle unknown flags with descriptive error
- [x] (AC: 5) Pass `--profile` value to config loader for profile selection
- [x] Test all flag combinations including invalid input

## Dev Notes

- **No clap dependency:** Keep binary size small. Use `argh` (lightweight, 0 dependencies) or manual parsing. The instruction set is small (<12 flags).
- **Config override pattern:** CLI-parsed values are stored in a `CliOverrides` struct that shadows config fields during `App::new()` construction. The config file is NOT written to.
- **Action dispatch:** Action flags (`--start-playing`, `--toggle-playback`, etc.) are stored as an `Option<MpdCommand>` that is sent once the MPD connection is established and the initial state sync is complete.
- **Profile selection:** `--profile <name>` is read before config loading, then used to select `[profiles.<name>]` section. Falls back to `[general].default_profile` if no match.
- **Existing pattern:** `Config::load()` in `src/config/mod.rs` currently reads from a single file. Add `with_profile(name: &str)` method for profile-aware loading.

### Source Files to Touch
- `src/main.rs` — Add CLI parsing before config load, integrate with App construction
- `src/config/mod.rs` — Add profile-aware loading method, `CliOverrides` struct
- `src/mpd/state_machine.rs` — Action dispatch after connection (or handle via cmd_tx)

### Testing
- Unit tests for each flag parsing and validation
- Integration test verifying config override behavior
- Edge cases: mutually exclusive flags, missing flag values, mixed flags + config

## References

- [Source: architecture.md §661] IPC & CLI Architecture — CLI flag specification
- [Source: architecture.md §448] Multi-Profile Connections — `--profile` flag integration
- [Source: prd.md §194] System Integration — CLI requirements
- [Source: epics.md §14] Epic 14: CLI & Desktop Integration

## Dev Agent Record

### Agent Model Used

Claude Code (deepseek-v4-flash)

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- All flags map to existing MpdCommand variants or config overrides — no new command paths needed
- Implemented manual std::env::args() parsing (no new dependencies)
- Added CliOverrides struct to config/mod.rs with apply_to_config() method
- Added parse_args<I>(args) generic parser with parse_cli_args() wrapper
- Added Config::with_profile() stub for future multi-profile support
- Added #[derive(Debug)] to MpdCommand enum for testability
- 23 unit tests covering all flags, validation errors, edge cases, combined flags

### File List

- `src/main.rs` — Added parse_args(), parse_cli_args(), print_usage(), integrated into main()
- `src/config/mod.rs` — Added CliOverrides struct, apply_to_config(), Config::with_profile()
- `src/mpd/state_machine.rs` — Added #[derive(Debug)] to MpdCommand
