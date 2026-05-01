# Story 18.1: High Contrast Mode Support

Status: done

## Story

As a user with visual impairment,
I want the application to support a high contrast theme variant,
so that all text and UI elements meet WCAG 2.1 AA contrast ratios (4.5:1 text, 3:1 UI elements).

## Acceptance Criteria

1. **Given** the application is running in default dark theme
   **When** the user enables high contrast mode in Settings
   **Then** a high-contrast CSS variant is applied with WCAG 2.1 AA contrast ratios
   **And** all text elements meet minimum 4.5:1 contrast against their background
   **And** all UI controls (buttons, sliders, indicators) meet minimum 3:1 contrast
   **And** cover art placeholders are replaced with solid high-contrast colors when enabled

2. **Given** the system accessibility preference signals high contrast
   **When** the application starts
   **Then** the high contrast theme is loaded automatically (respects system contrast settings)

3. **Given** high contrast mode is active
   **When** the user disables it in Settings
   **Then** the normal theme is restored
   **And** the preference is persisted in config

4. **Given** the user switches between normal and high contrast mode
   **When** the theme changes
   **Then** no widgets are left unstyled
   **And** the transition is instant (no animation delay)

## Tasks / Subtasks

- [ ] 1. Add high_contrast: bool field to Config struct (AC: 3)
  - [ ] 1.1 Default to false
  - [ ] 1.2 Serialize/deserialize in config load/save
- [ ] 2. Create high-contrast CSS variant (AC: 1)
  - [ ] 2.1 Override GTK4 CSS @define-color variables for WCAG 2.1 AA
  - [ ] 2.2 Cover art placeholder fallback colors with sufficient contrast
- [ ] 3. Implement CSS provider switching (AC: 1, 4)
  - [ ] 3.1 Load high-contrast CSS as additional provider on toggle
  - [ ] 3.2 Remove normal CSS when HC active; restore on disable
  - [ ] 3.3 No widget recreation — CSS provider swap only
- [ ] 4. Add Settings toggle UI (AC: 1, 3)
  - [ ] 4.1 High contrast checkbox in Settings dialog (General or Appearance section)
  - [ ] 4.2 Wire toggle to CSS provider swap + config save
- [ ] 5. Auto-detect system contrast preference (AC: 2)
  - [ ] 5.1 Read GtkSettings gtk-enable-animations or contrast preference
  - [ ] 5.2 Apply HC mode on startup if system preference detected
- [ ] 6. Integration testing (AC: 1-4)
  - [ ] 6.1 Test toggle on/off works without crashes
  - [ ] 6.2 Test config persist + restore
  - [ ] 6.3 Manual WCAG 2.1 AA contrast ratio verification

## Dev Notes

- **GTK4 CSS theming:** Use `gtk4::CssProvider` loaded from a separate `style-hc.css` file. On toggle, add/remove the provider from `GtkStyleManager`. CSS variables (`@define-color`) override the base theme colors.
- **Config field:** Add `high_contrast: bool` (default false) to `Config` struct in `src/config/mod.rs`. No schema migration needed (new field with default = false).
- **Auto-detect:** GTK4 exposes `GtkSettings:gtk-theme-name` and system color scheme; check for `Adwaita-dark` or system high-contrast setting. For non-GNOME desktops, respect `GTK_THEME` env var.
- **Cover art placeholders:** The hash-derived color placeholder in `src/ui/widgets/album_cover.rs` may need adjustment. In HC mode, use only the saturated hue channel with sufficient lightness for background contrast.
- **Seekbar/folder tree colors:** These use `@theme_bg_color` and `@theme_fg_color` — ensure the HC palette overrides these with sufficient ratio pairs.
- **Existing CSS:** Current styles are in `src/ui/style.css` (or inline CSS in `src/ui/mod.rs`). Check for `@define-color` usage to understand which variables to override.

### Project Structure Notes

- `src/config/mod.rs` — Config struct, add `high_contrast` field
- `src/ui/mod.rs` — CSS provider setup, Settings dialog toggle
- `src/ui/style.css` or inline CSS — Base stylesheet; create `style-hc.css` for HC overrides
- `src/ui/widgets/album_cover.rs` — Placeholder colors, may need HC-aware fallback
- `_bmad-output/implementation-artifacts/sprint-status.yaml` — Already updated

### References

- [Source: _bmad-output/planning-artifacts/prd.md §197] — "optional high-contrast mode for accessibility"
- [Source: _bmad-output/planning-artifacts/architecture.md §641-657] — Theming & UI Architecture ADR
- [Source: _bmad-output/planning-artifacts/epics.md §Epic 18] — Epic definition with full AC

## Dev Agent Record

### Agent Model Used

TBD

### Debug Log References

### Completion Notes List

### File List
- `src/config/mod.rs` — Add high_contrast field
- `src/ui/style-hc.css` — New high-contrast CSS variant
- `src/ui/mod.rs` — CSS provider toggling, Settings UI
- `src/ui/widgets/album_cover.rs` — HC-aware placeholder
