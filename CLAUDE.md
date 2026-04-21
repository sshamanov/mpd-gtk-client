# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

This is an MPD client project focused on two primary workflows:
1. Listening to a known collection through an album-first visual grid (Album Mode)
2. Discovering and checking new music through a folder-oriented technical view (Folder Mode)

The project is currently in the design phase with a detailed specification in `DESIGN.md`. No implementation code exists yet.

## Architecture

The system follows a layered architecture with one playback core and mode-specific presentation layers:

### Core Layers
1. **MPD adapter layer** – Connects to MPD (Music Player Daemon), handles protocol and transport concerns
2. **Application state layer** – Shared state across modes (playback, queue, current track, browsing state)
3. **Browsing presenters** – Mode-specific presentation logic (album grid vs folder tree)
4. **Queue presenters** – Mode-specific queue views (album mini-grid vs track list)
5. **UI layer** – Renders the visual interface

### Key Design Decisions
- **Split view**: Persistent 70/30 split with left browsing area and right persistent rail
- **Right rail contents**: Changes by mode but always shows playback controls
- **Album mode**: Visual cover grid with hover controls, album-oriented queue (mini cover grid)
- **Folder mode**: Text-first expandable folder tree with technical metadata, track-oriented queue
- **Queue model**: Single underlying queue with two presentation layers (album vs track views)
- **Drag and drop**: Supported for queue reordering in both modes
- **Normalization**: Folder presenter normalizes music directory structures (cue files, DSD folders, etc.)

### Layout Constants
- Shell split: 70% left / 30% right (starting point)
- Right rail width: `clamp(320px, 30vw, 420px)`
- Album mode rail proportions: Now Playing (40%), Current Album track window (20%), Album Queue (40%)
- Folder mode rail proportions: Now Playing (55%), Queue (45%)

## Development Status

- **Current phase**: Design specification complete (`DESIGN.md`)
- **Implementation**: Not started – no source files exist yet
- **Technology stack**: Not chosen – needs to be selected based on design requirements
- **BMad integration**: The project has BMad installed for project management (`_bmad/` directory)

## Reference

- **Primary specification**: `DESIGN.md` contains the complete product design, interaction rules, and technical architecture
- **BMad workflow**: Use `bmad-help` skill to navigate the BMad project management system if needed

## Implementation Notes

When implementing, prioritize:
1. MPD integration layer first (playback transport, queue management)
2. Application state management (shared across modes)
3. Album mode implementation (default entry point)
4. Folder mode implementation
5. Responsive layout system that respects the design proportions

The UI should remain calm, utilitarian, and focused on the two core workflows without feature creep.