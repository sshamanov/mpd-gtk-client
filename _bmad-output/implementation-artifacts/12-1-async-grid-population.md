# Story 12.1: Async Grid Population via GtkGridView Factory

Status: ready-for-dev

## Story

As a user with a large music library,
I want the album grid to populate without freezing the UI,
so that I can scroll and interact while albums load.

## Acceptance Criteria

1. **GtkGridView factory replaces FlowBox for album grid**
   - Given the app is populating the album grid (initial load, grouped view switch, search results)
   - When `populate_album_grid` or `populate_grouped_grid` would normally run synchronously
   - Then a `GtkGridView` with `GtkSignalListItemFactory` + `gio::ListStore` is used instead of manual `FlowBox` child insertion
   - And cell widgets are created/recycled by GTK4's factory (not destroyed and recreated on every population)

2. **Batched idle-time population**
   - Given the app needs to populate the grid with N albums
   - When the population starts
   - Then items are added to the backing `ListStore` in batches of 16 per idle cycle via `glib::idle_add`
   - And the UI remains responsive during population (>30 FPS maintained)
   - And the entire process completes within the overall timing targets (album list <2s for 10K albums)

3. **Grouped views with headers preserved**
   - Given the user switches to a grouped view (Artists, Years, Genres)
   - When the grid populates
   - Then group headers appear as items in the list model (same factory approach)
   - And each header shows the group name with album count
   - And the visual appearance matches the current header style [Source: src/ui/mod.rs:962-970]

4. **Double-click playback works correctly**
   - Given the album grid uses GtkGridView
   - When the user double-clicks an album cover
   - Then the correct `PlayAlbum` command is sent with the right album name
   - And the existing `album_names` index-based mapping is replaced with direct access from the list model item

5. **Cover display and hover controls preserved**
   - Given the album grid is displayed
   - When each grid cell renders
   - Then cover images are shown (or colored placeholder if no cover)
   - And hover controls (`+`, `<-`, `>`) work on album cells
   - And group headers have no hover controls

6. **Search results grid population works**
   - Given search results arrive via `MpdEvent::SearchResults`
   - When the grid is populated with results
   - Then the same GtkGridView factory pattern is used
   - And the list model is cleared and repopulated with the results

## Tasks / Subtasks

- [ ] Task 1: Create `AlbumGridItem` enum for the list model (AC: #1)
  - Define `enum AlbumGridItem { Header { name: String, count: usize }, Album { artist: String, name: String, album_id: String } }`
  - Must implement `glib::Boxed` or use `glib::Object` subclass for compatibility with `gio::ListStore`
  - [ ] Subtask 1.1: Implement `glib::Boxed` derive/impl for AlbumGridItem
  - [ ] Subtask 1.2: Verify it works with `gio::ListStore::new()`

- [ ] Task 2: Create GtkGridView + factory setup (AC: #1)
  - [ ] Subtask 2.1: Replace `GtkFlowBox` creation with `GtkGridView` creation in `src/ui/mod.rs`
  - [ ] Subtask 2.2: Create `GtkSignalListItemFactory` with `setup`/`bind`/`unbind` signals
  - [ ] Subtask 2.3: In `setup`: create widget shell (cover cell or header label based on item type)
  - [ ] Subtask 2.4: In `bind`: populate widget from AlbumGridItem data
  - [ ] Subtask 2.5: Set up `GtkSingleSelection` or `GtkNoSelection` as grid model adapter
  - [ ] Subtask 2.6: Wire double-click handler via `connect_activate` on GridView

- [ ] Task 3: Implement batched idle-time population (AC: #2)
  - [ ] Subtask 3.1: Create a population function that processes items in batches of 16
  - [ ] Subtask 3.2: Use `glib::idle_add` to schedule batch processing
  - [ ] Subtask 3.3: Clear the `ListStore` and start new batch sequence on each population
  - [ ] Subtask 3.4: Handle cancellation (new population request while previous in-flight)

- [ ] Task 4: Implement grouped view population (AC: #3)
  - [ ] Subtask 4.1: Convert grouped data into flat list with interleaved header items
  - [ ] Subtask 4.2: Use same factory with conditional widget rendering based on item type
  - [ ] Subtask 4.3: Style header items to match current group header appearance

- [ ] Task 5: Wire up event handlers (AC: #4, #5, #6)
  - [ ] Subtask 5.1: Update `MpdEvent::Albums` handler to populate via new factory pattern
  - [ ] Subtask 5.2: Update `MpdEvent::AlbumsGrouped` handler to populate via new factory pattern
  - [ ] Subtask 5.3: Update `MpdEvent::SearchResults` handler to populate via new factory pattern
  - [ ] Subtask 5.4: Remove `album_names` after verifying double-click works with model data

- [ ] Task 6: Clean up and verify (AC: #1, #6)
  - [ ] Subtask 6.1: Remove `populate_album_grid` and `populate_grouped_grid` functions
  - [ ] Subtask 6.2: Remove `FlowBox` import and related dead code
  - [ ] Subtask 6.3: Verify `cargo build` and `cargo test` pass
  - [ ] Subtask 6.4: Verify the grid loads, grouped views work, search results display

## Dev Notes

### Architecture Context

- **Current implementation** uses `GtkFlowBox` with synchronous `remove()` + `append()` cycles that block the GTK main thread. [Source: src/ui/mod.rs:956-998]
- **Target architecture** uses `GtkGridView` + `GtkSignalListItemFactory` + `gio::ListStore`. [Source: architecture.md §12]
- The factory pattern means widgets are created once and recycled — `setup` creates the widget shell, `bind` populates it from model data. [Source: architecture.md §10a]
- **6-thread model** requires all grid mutation on GTK main thread. Batched idle population is compatible — it adds items to the model in small batches per idle cycle.

### Key Technical Decisions

1. **`AlbumGridItem` needs `glib::Boxed` registration** for `gio::ListStore`. Use `glib::Boxed` derive macro:
   ```rust
   #[derive(Clone, glib::Boxed)]
   #[boxed_type(name = "AlbumGridItem")]
   pub enum AlbumGridItem {
       Header { name: String, count: u32 },
       Album { artist: String, name: String, album_id: String },
   }
   ```

2. **SignalListItemFactory setup pattern** (gtk4-rs 0.11):
   ```rust
   let factory = gtk4::SignalListItemFactory::new();
   factory.connect_setup(move |_factory, item| {
       let list_item = item.downcast_ref::<gtk4::ListItem>().unwrap();
       // Create widget shell based on expected item type
       let widget = create_cell_widget();
       list_item.set_child(Some(&widget));
   });
   factory.connect_bind(move |_factory, item| {
       let list_item = item.downcast_ref::<gtk4::ListItem>().unwrap();
       let item_data = list_item.item().and_then(|obj| obj.downcast::<AlbumGridItem>().ok());
       // Update widget from item_data
       update_cell_widget(&list_item, &item_data);
   });
   ```

3. **Column count** via `GtkGridView::set_max_columns()` or connect to `LayoutService`. Currently FlowBox uses `set_max_children_per_line()` derived from window width. Same calculation applies to GridView.

4. **Double-click** on GtkGridView: use `connect_activate` signal (fires on double-click/Enter). The handler reads the active item directly from the model:
   ```rust
   grid_view.connect_activate(move |grid, position| {
       if let Some(model) = grid.model() {
           if let Some(item) = model.item(position) {
               if let Some(album_item) = item.downcast_ref::<AlbumGridItem>() {
                   // Handle album activation
               }
           }
       }
   });
   ```

5. **Widget registry for covers**: The existing `CoverWidgets` map (`RefCell<HashMap<String, Picture>>`) for in-place cover updates still works. The key remains `album_id`. The factory `bind` signal should check this registry after populating from model data.

### Files to Touch

| File | Change |
|------|--------|
| `src/ui/mod.rs` | Replace FlowBox with GridView, factory setup, batch population, wire events. Remove `album_names`, `populate_album_grid`, `populate_grouped_grid`. |
| `src/ui/widgets/album_cover.rs` | May need minor refactor if `create_album_cover` API changes for factory pattern. |

### Testing

- Existing integration tests in `tests/` should pass unchanged (mock MPD tests)
- No new tests required for this story (grid rendering is visual — testing requires UI integration)
- `cargo build` and `cargo test` must pass

### Anti-patterns to Avoid

- Do NOT use `GtkBuilderListItemFactory` (requires UI templates in XML — complicates Rust integration). Use `GtkSignalListItemFactory` with Rust callbacks.
- Do NOT recreate widgets on model change — the factory pattern handles recycling. Only update widget content in `bind`.
- Do NOT skip batched population — even if the model supports `extend()`, schedule work in `glib::idle_add` batches to avoid long GTK main thread stalls.
- Do NOT store `album_names` in a separate `Vec<Option<String>>` — access album name directly from the list model item.

## References

- [Source: architecture.md §10a — Large-Library Rendering Strategy]
- [Source: architecture.md §12 — Performance Architecture]
- [Source: epics.md — Epic 12: UI Responsiveness / Story 12.1]
- [Source: src/ui/mod.rs:956-998 — Current FlowBox implementation]
- [Source: src/ui/mod.rs:129-163 — FlowBox setup and double-click handler]
- [Source: src/ui/mod.rs:716-779 — Event handlers that trigger grid population]
- [Source: Cargo.toml:10 — gtk4 0.11 with v4_14 feature]

## Dev Agent Record

### Agent Model Used

n/a

### Debug Log References

n/a

### Completion Notes List

n/a

### File List

- `_bmad-output/implementation-artifacts/12-1-async-grid-population.md` (this file)
- `_bmad-output/implementation-artifacts/sprint-status.yaml` (updated: epic-12 → in-progress, 12-1 → ready-for-dev)
