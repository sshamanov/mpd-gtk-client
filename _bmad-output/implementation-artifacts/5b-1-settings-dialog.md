# Story 5b.1: Settings Dialog

Status: ready-for-dev

## Story

As a user,
I want to configure MPD connection and save my preferences,
so that the app works with my specific setup.

## Acceptance Criteria

1. **Settings gear icon** — Top-right corner icon (⊙) that opens settings:
   - Placed in the right rail header area
   - Keyboard shortcut: `Ctrl+,`
   - Opens a settings dialog

2. **Settings dialog** — Modal window with:
   - MPD host text entry (default "127.0.0.1")
   - MPD port numeric entry (default 6600)
   - Save button — writes to `~/.config/mpd-client/config.toml`
   - Cancel button — discards changes

3. **Config persistence** — Settings saved to TOML:
   - Read on startup from config file
   - Fall back to defaults if config missing or corrupt
   - `config.toml` at standard path

4. **`cargo test` passes**

## Tasks / Subtasks

- [ ] Task 1: Create settings dialog UI (AC: 1, 2)
- [ ] Task 2: Add config read/write to TOML (AC: 3)
- [ ] Task 3: Wire startup to read config + gear icon action (AC: 1, 2)
- [ ] Task 4: Verify no regressions (AC: 4)

## Dev Notes

### Config Structure

```rust
#[derive(Serialize, Deserialize)]
struct Config {
    mpd_host: String,
    mpd_port: u16,
}
```

### Settings Dialog Pattern

```rust
let dialog = gtk4::Dialog::new();
dialog.add_button("Save", ResponseType::Accept);
dialog.add_button("Cancel", ResponseType::Cancel);
dialog.connect_response(move |d, resp| {
    if resp == ResponseType::Accept { /* save */ }
    d.close();
});
```

### What NOT to Do
- Do NOT implement theme customization
- Do NOT implement high-contrast mode

## Dev Agent Record

### Completion Notes List

### File List
