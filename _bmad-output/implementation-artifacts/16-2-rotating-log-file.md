# Story 16.2: Rotating Log File

Status: done

## Story

As a developer debugging issues,
I want the application to write logs to a rotating file in addition to stderr,
so that I can review past sessions' logs even after the terminal is closed.

## Acceptance Criteria

1. **Log file output**
   - **Given** the application is running with `RUST_LOG=debug`
   - **When** log messages are emitted
   - **Then** they are written to both stderr (at configured level) and a log file
   - **And** the log file is at `~/.local/share/mpd-client/log/mpd-client.log`

2. **Log rotation**
   - **Given** the log file exceeds 5MB
   - **When** new log entries are written
   - **Then** the log is rotated: current file is archived, a new file is started
   - **And** a maximum of 3 rotated files are kept
   - **And** the oldest rotated file is deleted when the limit is exceeded

3. **Auto-create log directory**
   - **Given** the log directory does not exist
   - **When** the application starts
   - **Then** the directory is created automatically

4. **Graceful failure on write error**
   - **Given** the log file cannot be written (permissions, disk full)
   - **When** a log write attempt fails
   - **Then** the error is silently ignored (logging failure is non-fatal)
   - **And** the application continues with stderr-only logging

5. **No performance impact**
   - **Given** the application is running normally
   - **When** log writes occur at info level (normal operation)
   - **Then** log writes take <1ms
   - **And** no UI stutter is observed during log writes

## Tasks / Subtasks

- [ ] (AC: 1) Add file appender to logging setup in `main.rs`
- [ ] (AC: 1) Configure log file path: `~/.local/share/mpd-client/log/mpd-client.log`
- [ ] (AC: 2) Implement file rotation: max 5MB, keep 3 files
- [ ] (AC: 3) Auto-create log directory on startup
- [ ] (AC: 4) Handle write errors gracefully (log to stderr only)
- [ ] (AC: 5) Verify no performance regression

## Dev Notes

- **Best approach:** Use `log4rs` with a simple file appender configuration, OR implement a custom `log::Log` that wraps env_logger and adds file output. `log4rs` handles rotation natively.
- **Alternative (lighter):** Implement a custom `log::Log` that wraps env_logger's output and writes to a file with manual rotation. This avoids adding `log4rs` as a dependency.
- **Log path:** Use `dirs::data_dir()` (expands to `~/.local/share/` on Linux) + `/mpd-client/log/mpd-client.log`. Consistent with XDG base directory spec.
- **Rotation naming:** `mpd-client.log` (current), `mpd-client.1.log`, `mpd-client.2.log`, `mpd-client.3.log` (oldest).
- **Trace-level to file:** File output uses `RUST_LOG` level. Info to stderr by default, but file always gets whatever level is configured.
- **Do not use `log4rs` polling** — it adds unnecessary overhead. File rotation on write, not on timer.
- **Current logging:** `env_logger::init()` in `main.rs`. The file appender is registered alongside env_logger via a custom logger composition.

### Source Files to Touch
- `src/main.rs` — Add file logger initialization
- `src/lib.rs` or new `src/logging.rs` — Logger setup and rotation logic

### Testing
- Unit test for log file creation
- Unit test for file rotation (write >5MB of test data, verify 3 rotated files)
- Unit test for log directory creation
- Edge cases: read-only log directory, disk full, concurrent log writes

## References

- [Source: architecture.md §621] Logging & Observability — Rotating file output specification
- [Source: epics.md §16] Epic 16: Infrastructure & Code Quality

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- File logging is additive to stderr logging — no existing behavior changes
- Rotation prevents unbounded disk usage

### File List

- `src/main.rs`
- `src/logging.rs` (new)
