# Story 21.1: Performance Profiling Instrumentation

Status: done

## Story

As a developer,
I want basic performance profiling instrumentation built into the application,
so that I can identify slow operations and measure UI responsiveness.

## Acceptance Criteria

1. **Timing macros for critical paths**
   - **Given** the application is running with `RUST_LOG=debug`
   - **When** a critical operation completes (MPD command, grid repopulation, cover decode, search query)
   - **Then** a log message at debug level includes the elapsed time in milliseconds
   - **And** operations exceeding 100ms are logged at warn level

2. **Frame rate monitoring**
   - **Given** the application is running
   - **When** the GTK frame clock fires
   - **Then** elapsed time since the previous tick is measured
   - **And** ticks exceeding 50ms (below 20 FPS) are logged at debug level with a warning

3. **Memory usage snapshot**
   - **Given** a library load or cover cache rebuild completes
   - **When** the operation finishes
   - **Then** approximate memory usage is logged at debug level (resident set size via `/proc/self/status`)

4. **No overhead in release builds**
   - **Given** the application is compiled in release mode
   - **When** any profiling path would execute
   - **Then** the timing code is compiled out (gated behind `debug_assertions` or a `profiling` feature flag)

## Dev Notes

- **No new dependencies:** Use `std::time::Instant` for timing, `/proc/self/status` for memory, existing `log` crate.
- **Pattern:** `let _start = Instant::now(); ... log::debug!("operation took {:?}", _start.elapsed());`
- **Gate behind `cfg!(debug_assertions)`** so it's zero-cost in release builds.
- **Key paths to instrument:** `connected_loop` iterations, `batch_populate`, cover decode, search queries, MPD command round-trips, frame clock tick duration.
- **Overhead:** `Instant::now()` is ~20ns — negligible. String formatting only happens when the log level is active (log crate's filtering).

## References
- [Source: PRD NFR-P1] Library load time <2s for 50k tracks
- [Source: PRD NFR-P2] UI responsiveness <100ms
- [Source: PRD NFR-O5] Performance profiling support for large libraries
- [Source: architecture.md §621] Logging & Observability — timing macros at debug level

## File List
- `src/main.rs` (or new `src/profiling.rs`) — timing helpers
- `src/ui/mod.rs` — frame clock measurement
- `src/mpd/state_machine.rs` — command timing
- `src/coverart/` — decode timing
- `src/search/mod.rs` — query timing
