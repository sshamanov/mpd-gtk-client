# Story 15.2: Profile Selector in Settings Dialog

Status: done

## Story

As a user,
I want to switch between connection profiles from the settings dialog,
so that I can change MPD servers without editing config files.

## Acceptance Criteria

1. **Profile dropdown in settings**
   - **Given** the settings dialog is open and multiple profiles are defined
   - **When** the user navigates to the Connection section
   - **Then** a profile dropdown shows all defined profiles from the config
   - **And** the current profile is pre-selected

2. **Profile switch triggers reconnect**
   - **Given** the user selects a different profile and clicks Save
   - **When** the settings are applied
   - **Then** the current MPD connection is gracefully closed
   - **And** a new connection is established using the selected profile's host/port/socket
   - **And** the `last_profile` key is updated in config

3. **Single profile — no dropdown**
   - **Given** only one profile is defined in config
   - **When** the settings dialog opens
   - **Then** the profile dropdown is hidden (no selection needed)

4. **No profile editing in v1**
   - **Given** the settings dialog is open
   - **When** the user looks at the Connection section
   - **Then** there is no "Add Profile" or "Edit Profile" button
   - **And** the profile dropdown is the only profile-related UI

## Tasks / Subtasks

- [ ] (AC: 1) Add `gtk4::DropDown` widget to settings dialog populated from profile names
- [ ] (AC: 2) Wire profile selection change to trigger `MpdCommand::Reconnect` with new profile params
- [ ] (AC: 2) Update config's `last_profile` on profile switch
- [ ] (AC: 3) Conditionally hide dropdown when only 1 profile exists
- [ ] (AC: 1) Pre-select current profile in dropdown

## Dev Notes

- **Using existing reconnect mechanism:** Profile switch uses the same `MpdCommand::Reconnect` mechanism as host/port changes in the current settings dialog. The command needs to carry the profile name or resolved connection params.
- **UI placement:** Add profile selector below the current host/port fields. When a profile is selected, the host/port fields are populated from the profile and become read-only (or are hidden).
- **gtk4::DropDown:** Use `gtk4::DropDown` with `gtk4::StringList` for the profile names. Bind the selected item to the profile switch action.
- **Backward compatibility:** If config has no profiles (legacy format), the dropdown is hidden (falls under the "single profile" rule).

### Source Files to Touch
- `src/ui/mod.rs` — Add dropdown to settings dialog, wire reconnect
- `src/config/mod.rs` — Expose profile list and getter

### Testing
- UI test may not be feasible — manual QA
- Test config `last_profile` update on selection
- Test reconnect with profile params

## References

- [Source: architecture.md §448] Multi-Profile Connections — Profile selector
- [Source: epics.md §15] Epic 15: Connection Profiles

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- No profile editing UI in v1 — selection only
- Reuses existing reconnect mechanism from settings dialog

### File List

- `src/ui/mod.rs`
- `src/config/mod.rs`
