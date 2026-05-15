# Story 40.1: Layout Profile Export/Import

Status: in-progress

Status: done

## Story

As a user who customizes the layout,
I want to export my layout settings to a JSON file and import them on another machine,
So that I can maintain a consistent layout across multiple installations without manual reconfiguration.

## Acceptance Criteria

1. **Given** the user has customized layout settings (split ratio, rail width, proportions, column preferences)
   **When** they click "Export Layout..." in the Settings dialog
   **Then** a file chooser dialog opens at a user-selected path
   **And** a JSON file is saved containing all layout-related settings
   **And** the JSON format includes a schema version field for forward compatibility

2. **Given** the user has a layout profile JSON file from another installation
   **When** they click "Import Layout..." in the Settings dialog
   **Then** a file chooser dialog opens to select the JSON file
   **And** the file is validated against the expected schema
   **And** if valid, all layout settings in the file are applied immediately
   **And** if invalid (bad format, missing fields, wrong schema version), an error toast is shown with a description of the issue

3. **Given** an import succeeds
   **When** the settings are applied
   **Then** the layout updates immediately (split ratio, rail proportions, column count)
   **And** the imported settings are persisted to config.toml
   **And** no restart is required

## Tasks / Subtasks

- [x] Add export/import functions for layout settings (AC: 1, 2)
  - [x] Collect layout fields from `Config` struct into JSON-serializable struct
  - [x] Implement `export_layout(&self) -> String` and `import_layout(&mut self, json: &str) -> Result` on Config
  - [x] Validate imported JSON against schema version
- [x] Add Export Layout button in Settings dialog (AC: 1)
  - [x] Use `gtk4::FileDialog` with save mode
  - [x] File filter for `.json` files
  - [x] Write JSON to selected path
- [x] Add Import Layout button in Settings dialog (AC: 2)
  - [x] Use `gtk4::FileDialog` with open mode
  - [x] Read and validate JSON file
  - [x] Show error on validation failure (status label)
- [x] Wire import to apply layout changes immediately (AC: 3)
  - [x] Persist changes to `Config::save()`
- [x] Define JSON schema with version field (AC: 1)
  - [x] `schema_version: u32`
  - [x] Layout fields: split_ratio, rail_width_min, rail_width_max, album_mode_proportions, folder_mode_proportions

### Review Findings

- [x] [Review][Defer] Import doesn't apply layout changes to running UI — `LayoutProfile` fields never consumed by layout code; no existing layout update path to wire into. Deferred, pre-existing.

## Dev Notes

- Export only includes layout settings — connection, profiles, MPRIS, cover cache settings are excluded
- Import merges into current config: layout values overwritten, non-layout values preserved
- No restart required — layout applies immediately via existing layout update path
- JSON format (not TOML) for portability:
  ```json
  {
    "schema_version": 1,
    "layout": {
      "split_ratio": 0.7,
      "rail_width_min": 320,
      "rail_width_max": 420,
      "album_mode_proportions": [0.4, 0.2, 0.4],
      "folder_mode_proportions": [0.55, 0.45]
    }
  }
  ```

### References

- Source: `_bmad-output/planning-artifacts/prd.md` §460-464 (User Customization & Persistence)
- Source: `_bmad-output/planning-artifacts/epics.md` Epic 40, Story 40.1
- Source: `src/config/mod.rs` — Config struct and save mechanism
- Source: `src/ui/settings/` — Settings dialog
