# Story 39.2: Memory Footprint Monitoring with Threshold Warning

Status: done

## Story

As a user,
I want the application to warn me when memory usage exceeds a safe threshold,
So that I can take action (close other applications, reduce library size) before the system becomes unresponsive.

## Acceptance Criteria

1. **Given** the application is running
   **When** memory usage exceeds 500MB RSS (Resident Set Size)
   **Then** a warning toast notification is shown: "Memory usage high (XXX MB) — consider closing other applications"
   **And** the warning is shown at most once every 60 seconds (rate-limited)
   **And** memory usage is checked at most once every 30 seconds (polling interval)

2. **Given** memory usage returns below 500MB after a warning
   **When** the next check runs
   **Then** no warning is shown (threshold no longer exceeded)
   **And** no "memory usage recovered" notification is needed

3. **Given** memory monitoring is implemented
   **When** a memory check runs
   **Then** the check reads `/proc/self/status` for `VmRSS` on Linux (the only target platform)
   **And** if the proc file is unreadable (permissions, container), the check is silently skipped with a debug log
   **And** the monitoring overhead is minimal (<1ms per check)

## Tasks / Subtasks

- [ ] Implement memory reading utility function (AC: 3)
  - [ ] Parse `VmRSS:` from `/proc/self/status`
  - [ ] Convert kB to MB
  - [ ] Return `Option<u64>` — None on unreadable proc file
- [ ] Add memory monitoring timer to GTK main loop (AC: 1)
  - [ ] Use `glib::timeout_add_local` with 30-second interval
  - [ ] Read current memory via the utility function
  - [ ] Compare against threshold (default 500MB)
- [ ] Implement rate-limited warning emission (AC: 1, 2)
  - [ ] Track `last_warning_time` in application state
  - [ ] Suppress warning if <60s since last warning
- [ ] Add config field `[memory] warning_threshold_mb = 500` (AC: 1)
  - [ ] Config serialization in `src/config/mod.rs`
  - [ ] Read by monitoring timer at runtime
- [ ] Add settings UI for memory warning threshold (AC: 1)
  - [ ] SpinButton in Settings dialog (General section)
  - [ ] Range: 100-4000 MB, step 100

## Dev Notes

- Linux-only: read `VmRSS:` from `/proc/self/status` — provides resident memory in kB
- Integration: check runs on the GTK main loop (fast proc read, no blocking)
- Memory monitoring is NOT part of the profiling instrumentation (epic 21) — it's a user-facing safety feature with toast notifications, not a developer tool
- The warning toast uses `ToastLevel::Warn` with 5s duration
- No action is taken beyond the warning — the application does not auto-throttle or reduce memory usage
- Use `std::fs::read_to_string("/proc/self/status")` and parse `VmRSS:` line

### References

- Source: `_bmad-output/planning-artifacts/prd.md` §843 (NFR-O3)
- Source: `_bmad-output/planning-artifacts/epics.md` Epic 39, Story 39.2
- Source: `src/profiling.rs` — existing profiling instrumentation (separate concern)
- Source: `/proc/[pid]/status` — Linux procfs documentation
