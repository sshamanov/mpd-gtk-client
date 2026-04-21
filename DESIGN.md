# DESIGN

## Purpose

This project is an MPD client built around two real workflows instead of a broad feature set:

1. listening to a known collection through an album-first visual grid
2. discovering and checking new music through a folder-oriented technical view

The product should stay narrow, calm, and utilitarian. It is not meant to become a full music manager with many competing surfaces.

## Product Principles

- Album-first by default
- Playback controls are always visible
- Primary click behavior should stay simple and direct
- Search exists, but is hidden by default
- Queue is secondary to browsing and playback
- Visual noise should stay low
- Artwork matters in album mode and in now playing
- Folder mode should stay text-first and technical

## Modes

### Album Mode

Album mode is the default entry point.

- Main surface is a cover grid only
- No default list view is needed
- Navigation is primarily visual, not search-based
- `Albums` is the default library view
- Alternate grouped views are `Artists`, `Years`, and `Genres`
- Group headers stay pinned while scrolling
- Free manual reordering is allowed only in plain `Albums` view
- Grouped views stay automatically ordered and do not support free manual reordering
- Queue is album-oriented, not track-oriented
- Hover actions are part of the primary browsing interaction

### Folder Mode

Folder mode is for new music, weak tags, and technical checking.

- Main surface is an expandable folder-oriented list
- Clicking a folder expands or collapses it
- Clicking a track selects it
- Queue is track-oriented and should stay visible
- Album art is not needed in the folder list itself
- Cover art should still be shown in now playing when available

## Layout

### Overall Shell

Use a split view with a persistent right rail.

- Left side: main browsing area
- Right side: now playing, transport, and queue-related content

Initial split:

- Left: 70%
- Right: 30%

This split is a starting point, not a hard commitment.

Initial sizing target:

- Right rail width: `clamp(320px, 30vw, 420px)`

The UI should be ready to move later toward `75/25` if browsing benefits from it.

### Right Rail

The right rail is persistent in both modes. It should not rely on tabs by default. Its internal content changes by mode while the shell stays stable.

#### Album Mode Right Rail

Initial vertical proportions:

- Now Playing: 40%
- Current Album track window: 20%
- Album Queue: 40%

These are starting values and must remain flexible.

Now Playing contains:

- cover art when available
- track title
- artist and album
- concise playback format info
- playback controls
- seekbar

Current Album contains:

- a limited-height scrollable track window for the currently playing album
- played and current track context first
- as many next tracks as fit in the available height

Album Queue contains:

- mini cover grid of queued albums
- left-to-right, then top-to-bottom ordering
- covers at 50% of left-panel album size as the starting point
- starting layout is `3x2`
- enough space to take up roughly half of the rail when needed

#### Folder Mode Right Rail

Initial vertical proportions:

- Now Playing: 55%
- Queue: 45%

Folder mode now playing is more transport-focused.

- seekbar matters more here than in album mode
- cover art may be shown if available
- queue is visible at all times and takes the rest of the rail below now playing
- queue is a plain track list, not an album mini-queue
- queue scroll should only move when the current track would leave the viewport
- queue follow behavior should keep the current track visible without unnecessary repositioning
- current track should be indicated by accent styling only, without a dedicated play icon
- selected items that are not currently playing should use a subtle border or outline

## Playback And Queue Design

### Playback Controls

Playback controls must always be visible.

- previous
- play/pause
- next
- seekbar

Seekbar behavior by mode:

- Album mode: visible but not central
- Folder mode: more important, easy to scrub visually, not precision-first

### Album Mode Queue

In album mode, the queue is conceptually an album queue, not a track queue.

- Show queued albums as a mini cover grid
- Order covers left to right, then top to bottom
- Give the currently playing album a distinct visual state
- Support drag and drop for exact placement
- Hovering the left or right side of a target album should indicate before or after insertion
- Start with a `3x2` mini-grid at 50% cover scale and refine later if needed
- Allow duplicate albums in queue
- Dragging an album off the queue grid removes it from the queue
- Duplicate albums should be rendered plainly, with no badge or stacked visual treatment

### Current Album Track Window

Album mode does not need a full track queue. It needs a limited window into the current album.

- Visible list height is limited by available space
- The first visible rows should start from the played/current area
- Show as many next tracks as fit
- Allow manual scrolling backward
- On track change, reset the window so the first visible row returns to the played/current area
- Clicking a track jumps playback to that track within the current album

### Folder Mode Queue

In folder mode, the queue is primarily a track queue.

- Show it as a plain visible track list
- Keep it simple and readable rather than album-oriented
- Track-level ordering matters more than album grouping here
- Let it occupy most of the lower part of the right rail
- Follow the currently played track only when it would otherwise leave the viewport

## Album Hover Controls

Album covers in album mode should expose three small hover-only buttons.

- Buttons appear instantly on hover
- Buttons should use normal UI sizing and should not be oversized
- Top-right: `+` adds album to queue
- Bottom-right: `<-` inserts album after the current album
- Bottom-left: `>` clears queue and plays album

Albums should also support context-menu actions for queue operations.

## Technical Information

### Folder Rows

Every visible track row in folder mode should include technical data because incoming albums may vary by track.

Priority when space is limited:

- For DSD: keep format first, such as `DSD` or `DSD128`
- For PCM: keep bit depth first, then sample rate, e.g. `16/44` or `24/96`
- Less important fields collapse first

### Now Playing Technical Info

Show concise, readable format information.

- Prefer short labels when they communicate enough
- Use compact labels such as `DSD`, `DSD128`, `16/44`, `24/96`, `16/88`, `24/192`
- If technical data is unavailable, omit the missing field instead of showing placeholders

## Covers And Artwork

- Covers are central to album mode
- Covers are not needed in folder lists
- Covers should appear in now playing when available in both modes
- Cover lookup priority in v1 is: local `cover` or `album` `jpg/jpeg`, then embedded artwork, then online lookup
- Online cover results in v1 should use session cache only
- Online cover lookup in v1 should run in a background queue after library load
- Background online cover fetching should prioritize visible or currently relevant items first, then continue through the rest of the library
- If online cover lookup fails for an item, mark it as failed for the current session and skip automatic retries
- If no cover is found, show the no-cover state without blocking playback or browsing

## Interaction Rules

Keep the main interaction model consistent.

### Album Mode

- Single click on album cover selects the album
- Double click on album cover clears the queue, enqueues the album, and starts playback from track 1
- `Add to queue` appends albums to the end of the queue in v1
- Dragging an album from the main grid into the album queue inserts it at an exact position
- Dragging queued albums reorders them by exact before/after placement
- Dragging a queued album off the album queue grid removes it from the queue
- Manual album ordering in the main grid is allowed only in plain `Albums` view
- Manual album ordering in plain `Albums` view is session-only in v1
- Albums can also be added through context menu actions
- Hover controls map to `+` add to queue, `<-` insert after current album, and `>` clear queue and play
- Duplicate albums in queue are allowed
- Album context menu in v1 contains `Play now`, `Play next`, and `Add to queue`
- Queued album item menu in v1 contains `Remove` and `Play now`

### Folder Mode

- Click folder: expand or collapse
- Double click folder: clear queue, enqueue folder or album, start playback from the beginning
- Click track: select
- Double click track: replace the current queue with the opened folder content and start playback from the selected track
- Drag and drop belongs in v1 for folder-mode queue interactions
- `Add to queue` appends folders, folder track batches, or tracks to the end of the queue in v1
- Play next in folder mode inserts after the current track
- `Play next` on a folder inserts that folder's tracks as one ordered batch after the current track
- Folder and track context menus in v1 contain `Play now`, `Play next`, and `Add to queue`
- Queued folder-mode item menu in v1 contains `Remove` and `Play now`

## Technical Architecture

### Backend Role

MPD is the initial playback and library backend.

MPD is responsible for:

- playback transport
- current playback state
- queue state
- library access
- output and format data where available

The client is responsible for:

- screen structure
- browsing models
- queue presentation
- interpretation of folder structures
- drag and drop behavior
- mode-specific UX rules

This keeps the product focused on UX and preserves the option to replace the playback backend later.

### High-Level Structure

Use one playback core with mode-specific presentation layers.

Suggested structure:

1. MPD adapter layer
2. application state layer
3. browsing presenters
4. queue presenters
5. UI layer

### MPD Adapter Layer

The MPD adapter isolates protocol and transport concerns.

Responsibilities:

- connect and reconnect to MPD
- fetch playback state
- fetch queue state
- fetch library and file data
- issue playback and queue commands
- normalize MPD responses into application-friendly models

#### MPD Adapter Resilience Requirements

The adapter must handle MPD’s real‑world failure modes gracefully:

- **Connection state machine:** Track disconnected, connecting, connected, error states
- **Exponential backoff retry:** Automatic reconnection with increasing delays after failures
- **Metadata caching:** Cache album and track metadata to survive temporary disconnections
- **Partial failure handling:** Continue operating when some MPD commands fail (e.g., cover art lookup)
- **Queue sync verification:** Periodically verify local queue state matches MPD’s actual queue

### Application State Layer

Maintain one shared application state that feeds both modes.

State domains:

- playback state
- current track
- current album context
- queue state
- album browsing state
- folder browsing state
- selection state
- drag and drop state
- layout state

Rules:

- playback state is shared across modes
- browsing state is mode-specific
- queue presentation changes by mode, but the underlying queue remains one source of truth

#### State Domain Boundaries

**Shared state (visible in both modes):**
- Playback state (playing/paused, current position, volume)
- Current track and album context
- Queue state (the underlying linear playback sequence)
- Connection state (MPD connected/disconnected)

**Mode‑local state (isolated per mode):**
- Album browsing: grid scroll position, selected album, hover state, group expansion
- Folder browsing: expanded folder paths, selected track, folder tree scroll position
- Layout preferences: split ratios, rail proportions (can persist per‑mode if desired)
- Drag‑and‑drop transient state

### Browsing Presenters

#### Album Presenter

Responsibilities:

- produce album cards for the main grid
- provide grouped views for `Albums`, `Artists`, `Years`, and `Genres`
- expose pinned group headers
- support manual ordering only in plain `Albums` view
- expose drag reorder targets for manual album ordering
- apply natural default sorting in grouped views: `Artists` A-Z, `Years` newest-first, `Genres` A-Z

#### Folder Presenter

Responsibilities:

- build directory-like rows for music browsing
- expose folder expand and collapse state
- expose track rows with per-track technical metadata
- provide lightweight normalization for the known folder cases
- fall back to a one-album track split when normalization is unclear

Normalization should improve readability but must never hide ambiguity.
Normalization is for the left browsing panel only. Once a directory or cue structure has been converted into folder and track entities, all later flows should treat those entities as normal folders and tracks.

Normalization rules:

- Plain directory with track files: use the files directly and ignore `cue` or `m3u` if regular tracks are already present
- First-level directory plus second-level `dsd` or `dsf` tracks: use the longest folder name as album name and treat `dsd` or `dsf` files as tracks
- Single image file plus `cue`: use the `cue` file and present the image as split tracks
- Multi-disc image files plus `cue`: use `cue` files and merge discs into one album
- If naming differs across nested folders, choose the longest folder name by character count as album name
- If normalization is unclear, split to tracks but still present the result as one album
- If a `cue` file is broken, first try resolving the referenced image by basename with a different common audio extension such as `flac` instead of `wav`
- If cue resolution still fails, discard the `cue` and fall back to the underlying playable file

#### Normalization Failure Handling

When normalization cannot produce a clean result:

- **Show ambiguity:** Present the raw folder structure with a visual indicator (e.g., "⚠ ambiguous structure")
- **Preserve playability:** All playable files remain accessible even if grouping is unclear
- **Error recovery:** If cue parsing throws an unhandled exception, log the error and fall back to treating the cue file as a single playable item
- **User override:** Allow manual "split as album" or "treat as folder" action via context menu
- **Session memory:** Remember user’s choice for the same folder path during the session

### Queue Model

Use one underlying queue model and two presenters.

Underlying queue requirements:

- ordered linear playback sequence
- enough metadata to derive album-level and track-level views
- references for album identity, track identity, source path, queue position, and current/playing state

#### Album Queue Presenter

Responsibilities:

- group queued items by album
- produce mini cover grid items for queued albums
- mark the currently playing album
- support drag insertion before or after target album
- preserve exact queue order beneath the album-level view
- allow duplicate album instances in the queue
- support removal by dragging album items off the queue grid
- append dragged items to the end when dropped onto empty queue background
- support edge autoscroll during drag operations
- support `Play now` as jump-only behavior that keeps the rest of the queue unchanged

##### Grid↔Queue Mapping Algorithm

The album queue grid is a visual representation of the underlying linear queue. Mapping between 2D grid positions and linear queue order follows these rules:

- **Grid layout:** Left‑to‑right, then top‑to‑bottom (row‑major order)
- **Grid dimensions:** Initially 3 columns (`3x2`), adjustable based on rail width
- **Position mapping:** Grid cell `(row, col)` maps to queue position `row * columns + col`
- **Drag reordering:** Dropping between grid cells inserts between the corresponding linear positions
- **Edge cases:** 
  - Dropping on the right half of a cell → insert after that queue item
  - Dropping on the left half → insert before that queue item
  - Dropping on empty grid background → append to end of queue
- **Visual feedback:** Show insertion marker between grid cells, not within cells

This ensures drag‑and‑drop in the grid visually matches the linear queue reordering.

#### Track Queue Presenter

Responsibilities:

- expose ordered track rows
- support plain visible list rendering
- support drag reorder when used in queue context
- support follow-on-leave scroll behavior for the current track
- support insertion before or after queue items using a thin insertion line, including top and bottom insertion zones
- treat dropped folders as batches of tracks
- append dragged items to the end when dropped onto empty queue background
- support edge autoscroll during drag operations
- support removal by dragging items off the queue viewport
- support `Play now` as jump-only behavior that keeps the rest of the queue unchanged

### Current Album Track Window Model

This is not a separate queue. It is a derived view over the current album context.

Responsibilities:

- derive the currently playing album track list
- maintain a scrollable visible window
- anchor the list on the played/current track area
- reset that anchor on track change
- allow manual scrolling backward and forward
- expose direct track selection within the current album

### Drag And Drop

#### Album Mode

Supported operations:

- reorder albums inside album queue
- drag albums from the main grid into the queue
- insert albums before or after exact target positions
- manually reorder albums in plain `Albums` view only
- append to the end when dropped onto empty queue background
- edge autoscroll is enabled during drag

Constraints:

- grouped views such as `Artists`, `Years`, and `Genres` do not support free manual reordering
- drag logic must operate on a linear queue even when rendered as a grid

#### Folder Mode

Supported operations are part of v1:

- drag track to queue
- drag folder or album block to queue
- drag tracks within queue to reorder
- insert at exact position between queue items, or at explicit top or bottom zones
- treat dragged folders as batches of tracks
- append to the end when dropped onto empty queue background
- edge autoscroll is enabled during drag
- dragging queue items out of the queue viewport removes them

Do not support arbitrary reordering of the source folder tree itself.

### Layout Configuration

These values should stay configurable rather than hard-coded:

- shell split
- right rail width
- album-mode rail proportions
- folder-mode rail proportions

Initial values:

- shell split: `70/30`
- right rail width: `clamp(320px, 30vw, 420px)`
- album mode rail: `40 / 20 / 40`
- folder mode rail: `55 / 45`

#### Layout Service Consideration

For maintainability, consider centralizing layout logic in a layout service that:

- **Manages proportional splits:** Stores and applies shell split, rail widths, internal proportions
- **Handles responsive breakpoints:** Adjusts grid columns, rail visibility on window resize
- **Persists preferences:** Remembers user adjustments per mode across sessions
- **Coordinates UI updates:** Notifies presenters when layout values change
- **Validates constraints:** Ensures proportions stay within usable ranges

This keeps layout calculations separate from presentation logic.

### Resilience And Fallbacks

The client should remain usable when MPD or media data is imperfect.

- Automatically try to reconnect to MPD when disconnected
- Be ready for temporary disconnect state in the UI
- Show disconnected or missing-MPD state as a calm inline empty panel rather than a blocking takeover screen
- Show the inline empty or disconnected panel primarily in the main content area
- Empty library should render as an empty interface state rather than an error
- Empty folder roots should simply show no folder entries
- Missing cover art should not block playback or browsing
- Missing technical data should simply not be shown

### Future-Proofing

The architecture should allow later changes without redesigning the app.

Be ready for:

- changing the split from `70/30` to `75/25`
- changing right rail internal proportions
- swapping the playback backend later
- extending queue interactions without changing the main shell

Key invariant:

- one playback core
- one source of truth for queue and playback state
- multiple mode-specific presenters

## User Flows

### App Entry

1. Open in album mode
2. Show `Albums` as the default view
3. Show the main cover grid on the left
4. Show the persistent right rail on the right
5. Keep playback controls visible at all times

### Album Listening Flow

#### Browse

1. User lands in album grid
2. User visually scans covers
3. User may switch grouped view to `Artists`, `Years`, or `Genres`
4. Group headers remain pinned while scrolling

#### Select And Play Album

1. User single-clicks an album cover to select it
2. User double-clicks an album cover to clear the queue
3. The selected album is enqueued
4. Playback starts from track 1
5. Now playing updates in the right rail
6. Album queue updates as a mini cover grid
7. Hover controls appear instantly while hovering the album cover

#### Queue Additional Albums

1. User drags an album from the main grid into the album queue, uses the `+` hover button, or uses a context-menu action
2. Insertion target is shown before or after a queued album
3. Album is inserted at the exact target position
4. Queue grid updates left to right, then top to bottom

#### Reorder Album Queue

1. User drags a queued album cover
2. UI shows before or after insertion marker relative to the hovered album
3. Album is dropped into the target position
4. Queue order updates without changing the shell

#### Inspect Current Album Tracks

1. Right rail shows the current album track window
2. The first visible rows start at the played/current context
3. As many next tracks are shown as fit into the available height
4. User may scroll backward to earlier tracks
5. On track change, the window resets to the played/current anchor

#### Jump Within Current Album

1. User clicks a track in the current album track window
2. Playback jumps to that track
3. Playback continues through the album from there
4. Current track state updates in the track window

### Folder Discovery Flow

#### Enter Folder Mode

1. User switches from album mode to folder mode
2. Left panel changes from cover grid to folder-oriented list
3. Right rail stays persistent
4. Now playing remains visible

#### Browse Folders

1. User scans text-first rows
2. Track rows display technical information directly
3. DSD rows prioritize compact format labels like `DSD` or `DSD128`
4. PCM rows prioritize bit depth, then sample rate

#### Expand Folder

1. User clicks a folder row
2. Folder expands inline to show tracks
3. User can collapse it again with another click

#### Play Folder Or Album

1. User double-clicks a folder row
2. Queue is cleared
3. Folder or album content is enqueued
4. Playback starts from the beginning
5. Right rail updates now playing and queue state

#### Play Track

1. User clicks a track row to select it
2. User double-clicks the track row to replace the queue with the opened folder content
3. Playback starts from the selected track
4. Queue remains visible as a plain track list
5. Queue scroll follows only if the current track would leave the viewport

#### Use Seek In Folder Mode

1. User interacts with the always-visible seekbar in the right rail
2. Seekbar should feel visually easy to scrub
3. Precision-first behavior is not required

### Queue Presentation Flow

#### Album Mode

1. Queue is presented as albums
2. Queued albums appear as small covers
3. The currently playing album has a distinct state
4. User can reorder by drag and drop
5. Initial queue layout is `3x2` at 50% cover scale
6. Manual album reordering lasts only for the current session in v1

#### Folder Mode

1. Queue is presented as tracks
2. Queued tracks appear as a plain visible text list
3. Queue scroll follows the current track only when it would leave the viewport
4. Reordering is allowed in queue context in v1
5. `Play now` on a queued item jumps to it without rebuilding the queue

### Search Flow

1. Search is hidden by default
2. User explicitly opens search as an inline field in the header when browsing is insufficient
3. Search scope in v1 is limited to the current mode and active view
4. In album mode, search matches all available metadata
5. In folder mode, search matches full path text, normalized folder names, track titles, raw filenames, and file-related text
6. Search results are shown as a live in-place filter of the current view
7. In grouped album views, search keeps groups and shows only groups and items with matches
8. Closing search restores the previous scroll position and folder expansion state, then returns focus to the main browsing surface
