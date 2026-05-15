# Story 34.1: Eliminate Redundant Config::load() Call

Status: done

## Story

As a developer,
I want `Config::load()` called only once during startup,
so that the TOML file is not parsed twice when the window close handler is set up.

## Acceptance Criteria

1. **Single Config::load() call**
   - Given the application starts
   - When the startup sequence runs
   - Then `Config::load()` is called exactly once (in `main.rs`)
   - And the window close handler reuses the already-loaded config

2. **Window geometry saved correctly**
   - Given the window close handler saves geometry
   - When the user resizes the window and closes it
   - Then the geometry is still saved correctly
   - And the saved config preserves all fields from the originally loaded config plus the geometry update

3. **No regression in config handling**
   - Given the config contains all fields (host, port, profiles, window geometry, etc.)
   - When the close handler writes geometry
   - Then no fields are lost due to the config reuse
   - And only the window_geometry field is modified before saving

## Technical Requirements

- Currently `Config::load()` is called in `main.rs` line 167 and again in `ui/mod.rs` line 394 (window close handler).
- Fix: Pass the already-loaded config reference into the close handler closure.
- The config is loaded in `main.rs` and stored as `let mut config = Config::load()`. This can be threaded through `App::new()` or captured in the `application.connect_activate` closure.
- The close handler (connected to `window.connect_close_request`) currently loads its own config to read/write geometry. Instead, it should clone the config, update geometry, and save.
- Simplest fix: In the `connect_activate` closure, capture the loaded config and move it into the close handler: `let cfg_for_close = config.clone(); window.connect_close_request(move |_| { let mut c = cfg_for_close.clone(); c.window_geometry = Some(...); let _ = c.save(); })`.

## References
- [Source: epics.md] Epic 34: General Code Quality — Story 34.1
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — Config loaded twice during startup
- [Source: src/main.rs:167] First Config::load() call
- [Source: src/ui/mod.rs:394] Second Config::load() call in close handler
