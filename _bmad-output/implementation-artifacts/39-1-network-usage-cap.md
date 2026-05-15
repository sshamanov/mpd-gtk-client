# Story 39.1: Configurable Network Usage Cap with Monitoring

Status: done

## Story

As a privacy-conscious user,
I want to set a monthly network usage cap for online cover lookups,
So that the application does not exceed my data budget when fetching artwork from the internet.

## Acceptance Criteria

1. **Given** the online-cover-art feature is enabled
   **When** the application fetches covers from online sources (MusicBrainz, Cover Art Archive)
   **Then** total bytes downloaded is tracked per session
   **And** a running total is compared against the configured monthly cap (default: 500MB)
   **And** if the cap is exceeded, online cover lookups are suspended for the remainder of the month
   **And** a toast notification is shown: "Online cover lookup paused — monthly data cap reached"

2. **Given** the network usage cap is configurable
   **When** the user opens Settings
   **Then** they can set the monthly cap in megabytes (0 = unlimited)
   **And** the current month-to-date usage is displayed in the settings dialog
   **And** the usage counter resets at the start of each calendar month

3. **Given** online cover lookups are disabled (default)
   **When** the application runs
   **Then** no network usage tracking is needed (no HTTP requests are made)

## Tasks / Subtasks

- [ ] Add `monthly_bytes_downloaded` and `last_reset_month` fields to Config struct (AC: 1, 2)
  - [ ] Config serialization/deserialization in `src/config/mod.rs`
  - [ ] Default values: bytes=0, month=current month number
- [ ] Implement network usage tracking in online cover lookup path (AC: 1)
  - [ ] Wrap HTTP fetch in `src/coverart/online.rs` to count bytes
  - [ ] Check cap before making HTTP request
  - [ ] Persist updated byte count to config after each fetch
- [ ] Implement monthly cap reset logic (AC: 2)
  - [ ] Compare `last_reset_month` against current month on startup and before each fetch
  - [ ] Zero counter and update `last_reset_month` on month boundary
- [ ] Add cap exceeded notification (AC: 1)
  - [ ] Emit `Toast { level: Warn, message: "..." }` when cap is first exceeded
- [ ] Add settings UI for monthly cap (AC: 2)
  - [ ] SpinButton or Entry for cap value (megabytes, 0=unlimited)
  - [ ] Read-only display of current month-to-date usage
  - [ ] Place in Connection or Cover Cache section of Settings dialog
- [ ] Add config field `[cover_cache] monthly_data_cap_mb = 500` (AC: 2)

## Dev Notes

- Network usage tracking applies only to the `online-cover-art` feature (HTTP requests to MusicBrainz/Cover Art Archive)
- MPD protocol traffic (local network, typically <1MB/hour) is NOT tracked — this cap is for external data only
- The cap is a soft limit — lookups are suspended with a toast, not blocked at the system level
- Low priority — most users keep online lookups disabled (opt-in by default)
- Relevant source: `src/coverart/online.rs` for HTTP fetch path, `src/config/mod.rs` for config persistence, `src/ui/settings/` for settings dialog

### References

- Source: `_bmad-output/planning-artifacts/prd.md` §833 (NFR-S3)
- Source: `_bmad-output/planning-artifacts/epics.md` Epic 39, Story 39.1
- Source: `src/coverart/online.rs` — existing online lookup HTTP code
- Source: `src/config/mod.rs` — config struct and serialization
