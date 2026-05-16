# Story 40.2: Layout Dimension Reset to Defaults

Status: out-of-scope

## Story

As a user who has customized layout settings,
I want to reset individual layout dimensions or all layout settings to their defaults,
So that I can recover from changes that don't work well for me without manually undoing each setting.

## Acceptance Criteria

1. **Given** the Settings dialog is open with layout customization options
   **When** the user clicks "Reset All Layout Settings"
   **Then** all layout values revert to defaults (split ratio 0.7, rail 320-420px, album 40/20/40, folder 55/45)
   **And** the layout updates immediately to reflect defaults
   **And** a confirmation toast is shown: "Layout settings reset to defaults"

2. **Given** individual layout dimensions have reset buttons (split ratio reset, proportions reset)
   **When** the user clicks a single dimension's reset button
   **Then** only that dimension reverts to its default value
   **And** other layout settings are preserved unchanged
   **And** the layout updates immediately

3. **Given** the user resets all or individual layout settings
   **When** the reset completes
   **Then** the updated settings are persisted to config.toml
   **And** the changes are applied immediately without requiring a restart

## Tasks / Subtasks

- [ ] Define default layout constants (AC: 1)
  - [ ] Create `LayoutDefaults` struct or named constants in `src/config/mod.rs`
  - [ ] Split ratio default: 0.7
  - [ ] Rail width min: 320, max: 420
  - [ ] Album mode proportions: 40/20/40
  - [ ] Folder mode proportions: 55/45
- [ ] Add per-dimension reset buttons in Settings dialog (AC: 2)
  - [ ] Split ratio reset button
  - [ ] Rail width min/max reset button
  - [ ] Album mode proportions reset button
  - [ ] Folder mode proportions reset button
  - [ ] Each button resets only its dimension to default
- [ ] Add "Reset All Layout Settings" button in Settings dialog (AC: 1)
  - [ ] Confirmation dialog: "This will reset all layout settings to defaults. Continue?"
  - [ ] On confirm, reset all layout dimensions to defaults
  - [ ] Show confirmation toast
- [ ] Wire reset to apply layout changes immediately (AC: 3)
  - [ ] Call existing layout update path after each reset
  - [ ] Persist changes to `Config::save()`
- [ ] Add individual reset buttons per dimension (AC: 2)
  - [ ] Place reset button next to each layout spinbutton/slider
  - [ ] Use a small "Reset" label or icon button

## Dev Notes

- Individual reset buttons per layout dimension: split ratio, rail min width, rail max width, album mode proportions, folder mode proportions
- "Reset All" gets a confirmation dialog (not individual resets)
- The reset modifies the in-memory config and triggers the layout service to recompute positions
- Existing `Config::save()` mechanism persists the changes
- Default values must be defined as constants, not hardcoded, to keep a single source of truth

### References

- Source: `_bmad-output/planning-artifacts/prd.md` §460-464 (User Customization & Persistence)
- Source: `_bmad-output/planning-artifacts/epics.md` Epic 40, Story 40.2
- Source: `src/config/mod.rs` — Config struct, save mechanism
- Source: `src/ui/settings/` — Settings dialog with layout controls
