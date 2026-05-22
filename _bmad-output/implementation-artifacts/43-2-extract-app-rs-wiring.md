# Story 43-2: Extract app.rs Application Wiring Module

## Status: done
baseline_commit: 72f53fd905fb5e6f41e93a48ed9b63a31e435abc

## Context

Architecture.md specifies `app.rs` as "thin wiring only — no business logic" for GTK application setup. Currently `ui/mod.rs` creates the Application, registers actions, and calls `connect_activate` inline. Extract this into a dedicated `src/app.rs`.

See `.claude/plans/glittery-wiggling-puzzle.md` Step 4.

## Tasks

### Task 1: Create `src/app.rs`
- Create module with `pub fn run(...)` taking all channels and state as parameters
- `app::run()` creates `gtk4::Application` with `"com.mpdclient.app"`
- Registers all actions (quit, mode switch Ctrl+1/2, search Ctrl+F, settings Ctrl+,, help Ctrl+?, toggle sidebar Ctrl+B)
- Calls `application.connect_activate(ui::build_ui)` — delegates widget building to ui module
- Calls `application.run()`

### Task 2: Export `build_ui` from ui module
- Extract the `connect_activate` body from `App::run()` into a public `ui::build_ui()` function
- Takes `&gtk4::Application` and all state/channel params
- The function is called by `app.rs` via `connect_activate`

### Task 3: Update `main.rs`
- Replace `App::new(...)` + `app.run()` calls with `app::run(...)`
- Pass state, event_rx, cmd_tx, conn_params, mpris_update_tx, metadata_cache, search_cmd_tx, toast_tx

### Task 4: Update `lib.rs`
- Add `pub mod app;`

### Task 5: Remove `App` struct
- Delete the `App` struct from `ui/mod.rs` (lines 308-343)
- Delete `App::new()` constructor (lines 322-343)
- `ui/mod.rs` now exports `build_ui()` instead of `App`

## Acceptance Criteria
1. `src/app.rs` exists with clean `run()` function — thin wiring only
2. `ui/mod.rs` exports `build_ui()` — no Application creation within ui module
3. `main.rs` calls `app::run()` — no direct `App` usage
4. All 7 actions registered in `app.rs`
5. `cargo build` passes
6. App window opens with Ctrl+Q, Ctrl+1/2, Ctrl+F working
