---
stepsCompleted: [1, 2, 3, 4, 5, 6]
inputDocuments: ["DESIGN.md", "DESIGN-validation-report.md", "CLAUDE.md"]
---

# UX Design Specification mpd-client

**Author:** User
**Date:** 2026-04-22

---

<!-- UX design content will be appended sequentially through collaborative workflow steps -->

## Executive Summary

### Project Vision

A unified MPD client that solves the dual needs of music listening through two specialized modes: Album Mode for visual, album-first listening of known collections, and Folder Mode for technical, folder-first discovery and checking of new music. The application aims to be "just work" simple with zero configuration, focusing on the music rather than settings or options. It should feel like one cohesive application with two complementary interfaces, not two separate apps bolted together.

### Target Users

**Alex (Album-First Listener):** Music collector with a large, well-tagged library who wants relaxed listening, revisiting known albums, and building mood-based queues. Frustrated by cluttered interfaces, excessive metadata, and track-focused views that obscure album context.

**Jamie (Folder-First Technical Listener):** Audiophile and new-music checker who needs to verify audio quality, inspect folder structures, check DSD/PCM formats, and organize incoming music. Frustrated by album-centric views that hide file/folder reality and missing technical metadata.

Both users want an application that works during work hours (non-distractive) but allows effective concentration on music listening or checking without annoyance.

### Key Design Challenges

1. **Dual-Mode Cohesion vs. Specialization** - Creating two distinct interfaces (Album Mode for visual browsing, Folder Mode for technical checking) that feel like one unified application, not two separate apps bolted together.
2. **Simplicity vs. Power** - Making it "just work" with zero configuration while still providing the technical depth needed for audiophile checking (DSD/PCM formats, cue sheets, folder structures).
3. **Work-Friendly Design** - Creating an interface that's calm enough for background listening during work, yet powerful enough for focused music discovery sessions.
4. **Visual-to-Textual Transition** - Smoothly transitioning users between album-first visual browsing and folder-first technical inspection without cognitive friction.

### Design Opportunities

1. **Unified Design Language** - Establishing consistent visual patterns (typography, spacing, interaction models) that work across both modes, creating a cohesive brand identity.
2. **Progressive Disclosure** - Hiding complexity (search, advanced options) by default while making it accessible when needed, keeping the interface clean.
3. **Context-Aware Right Rail** - Using the persistent 30% rail to adapt content based on mode (album context vs. transport controls) without changing the overall layout structure.
4. **Calm Interaction Design** - Minimizing visual noise, using subtle animations, and creating a focused environment that respects the music rather than competing with it.

## Core User Experience (Enhanced Version)

### Defining Experience

The core experience centers on **hover controls as primary interaction** with double-click as convenient secondary option. In Album Mode: hover buttons (`+` add to queue, `←` insert after current, `>` clear queue and play) are main controls; double-click album provides redundant "empty queue and start playing" option. In Folder Mode: double-click track/folder remains primary for immediate playback. This hybrid approach balances discoverability with power user efficiency.

**Album selection preview persists** while browsing the grid — metadata and cover art remain in the right rail until the user starts playback or explicitly deselects. No automatic fade to now-playing during exploratory browsing.

**Mode-specific metadata:** Album Mode displays artist, album, year, and format on selection. Folder Mode additionally shows sample rate, bit depth, file path, and technical audio details — giving each mode metadata depth appropriate to its use case.

### Platform Strategy

**Desktop-only Linux application** with GTK4, optimized for mouse/keyboard. **Smart MPD configuration**: auto-connect to default MPD (localhost:6600); if fails, show settings dialog; store configuration in TOML file at `~/.config/mpd-client/config.toml` with versioning and corruption recovery. **Always-accessible settings**: Small persistent gear icon (top-right corner) expands on hover; keyboard shortcut `Ctrl+,` for quick access.

### Effortless Interactions

1. **Smart connection** - Application auto-connects to MPD; configuration only required for non-standard setups.
2. **Hybrid controls** - Hover buttons provide discoverable primary actions; double-click offers convenient alternative.
3. **Transparent auto-updates** - When library changes, queue updates automatically with subtle toast notification: "Queue synchronized with library changes" (3-second display).
4. **MPD resilience** - Cached state allows queue modification during disconnections; automatic synchronization when MPD returns.
5. **Always-available settings** - Settings accessible via persistent gear icon or `Ctrl+,` keyboard shortcut.
6. **Conflict-aware synchronization** - Intelligent matching when library changes: preserve exact tracks, match by album+artist, prefer same format, use newest version.

### Critical Success Moments

1. **"It connected automatically"** - Application starts and connects to MPD without configuration prompts.
2. **"The buttons do what I expect"** - Hover controls feel intuitive; double-click provides convenient alternative.
3. **"It kept my queue through library changes"** - Library updates trigger intelligent queue synchronization with clear visual feedback.
4. **"It survived the crash"** - MPD disconnection/reconnection preserves queue state exactly as user left it.
5. **"Settings are right where I need them"** - Persistent gear icon provides always-available configuration access without UI clutter.
6. **"It handled the format conflict smartly"** - When library changes introduce format variations, system preserves user intent intelligently.

### Experience Principles

1. **Hover Controls Primary, Double-Click Secondary** - Hover buttons (`+`, `←`, `>`) are main interaction method; double-click provides redundant "empty queue and start playing" option for convenience.
2. **Smart MPD Configuration** - Auto-connect to default MPD (localhost:6600); if fails, show settings dialog with connection options; store configuration in TOML file at standard path; never ask again once configured.
3. **Always-Accessible Settings** - Small persistent gear icon (top-right corner) that expands on hover; keyboard shortcut `Ctrl+,` for quick access.
4. **Transparent Auto-Updates** - Subtle toast notifications for automatic queue updates: "Queue synchronized with library changes" (3-second display).
5. **Standard Configuration Path** - TOML file at `~/.config/mpd-client/config.toml` with versioning and corruption recovery.
6. **Conflict Resolution Hierarchy** - When library changes affect queued items: 1) Preserve exact track matches, 2) Match by album title + artist, 3) Prefer same format (DSD/PCM), 4) Use newest version.
7. **Queue Modification Priority** - User queue modifications take priority; library scans wait or merge changes intelligently to prevent race conditions.
8. **Album Mode Track Window as Navigation Aid** - Current album track window serves only for next/prev control and album information; not for full queue manipulation; cannot drag albums into it.
9. **Visual Model Clarity** - Album Mode clearly distinguishes between album-level operations (grid) and track-level operations (current album window) through consistent visual language.
10. **Preview Persistence During Browsing** - Album selection preview stays in right rail while browsing the grid; only transitions to now-playing on playback start or explicit deselect. No automatic fade during exploratory browsing.
11. **Equal Mode Investment** - Album Mode and Folder Mode receive equal design polish and attention. Folder Mode is not an afterthought — each mode has metadata depth and interaction patterns appropriate to its use case.
12. **Chrome Fades During Active Listening** - Queue and controls remain visible but visually recede during active playback, minimizing distraction while keeping functionality accessible.
13. **Folder Mode Queue Precision** - Folder Mode queue displays as a track list with filenames, format, and duration. Insertion point is clearly visible when adding tracks; users see exactly where new items land in the queue.

## Desired User Satisfaction Goals

### Primary Satisfaction Goals

**Confidence** - Users should feel certain the application will work as expected, with clear feedback and predictable behavior.

**Efficiency** - Interactions should feel streamlined and purposeful, minimizing unnecessary steps while maintaining clarity.

**Reliability** - The application should work consistently across sessions, handling edge cases gracefully without user intervention.

**Right Amount of Control** - Users should have exactly the control they need for their task, without being overwhelmed by unnecessary options or limited by missing functionality.

### Satisfaction Journey Mapping

**Initial Discovery** - Should feel immediately usable with sensible defaults and clear affordances.

**Core Experience** - Should feel efficient and controlled, with interactions that match user expectations.

**Task Completion** - Should feel accomplished, with clear success indicators and no lingering uncertainty.

**Error States** - Should maintain confidence through clear error messages and recovery paths.

**Return Usage** - Should feel familiar and reliable, with consistent behavior across sessions.

### Micro-Satisfaction States

**Confidence vs. Confusion** - Clear visual feedback and predictable interactions build confidence; ambiguous states create confusion.

**Efficiency vs. Friction** - Streamlined workflows create efficiency; unnecessary steps or delays create friction.

**Reliability vs. Anxiety** - Consistent performance builds reliability; unexpected failures create anxiety.

**Control vs. Overwhelm** - Appropriate feature exposure provides control; excessive options create overwhelm.

### Design Implications

**Confidence Design** - Clear visual feedback, predictable interactions, transparent system status, and consistent behavior patterns.

**Efficiency Design** - Streamlined workflows, keyboard shortcuts, sensible defaults, and progressive disclosure of complexity.

**Reliability Design** - Robust error handling, graceful degradation, state persistence, and automatic recovery mechanisms.

**Control Design** - Context-appropriate feature exposure, customizable defaults, and clear affordances for available actions.

### Satisfaction Design Principles

1. **Design for Confidence** - Every interaction should reinforce user certainty through clear feedback and predictable outcomes.
2. **Optimize for Efficiency** - Minimize cognitive load and interaction steps while maintaining clarity and discoverability.
3. **Ensure Reliability** - Build systems that work consistently and recover gracefully from unexpected conditions.
4. **Provide Appropriate Control** - Expose features contextually based on user needs, avoiding both underpowered and overwhelming interfaces.
5. **Maintain Calm Focus** - Create a distraction-free environment that supports concentration on the music rather than the interface.

## UX Pattern Analysis & Inspiration

### Inspiring Products Analysis

**Google Chrome/Chromium** demonstrates **intuitive navigation** through the omnibox pattern (combined address/search), **clean visual design** with minimal interface chrome, and **predictable performance** that builds user confidence. The application feels responsive and reliable, with clear error handling and helpful recovery paths.

**Alacritty** embodies the **Unix philosophy of doing one thing well** - terminal emulation without feature bloat. Its **minimalist approach** eliminates unnecessary UI elements, focusing purely on the core task. The application prioritizes **performance as a UX feature** with GPU-accelerated rendering that feels noticeably faster than alternatives.

**Hyprland** shows how to **balance simplicity with flexibility** - providing the "right amount of control" without overwhelming users. Its **keyboard-driven interaction model** becomes efficient through muscle memory, while **clean visual design** avoids distraction. The application demonstrates **graceful configuration** with sensible defaults that work immediately but allow customization when needed.

### Comparative Pattern Analysis

**Evaluation Criteria (Weighted by Importance):**
1. **Confidence Building** (25%) - Does this pattern build user certainty and predictable behavior?
2. **Efficiency Enhancement** (25%) - Does this pattern streamline interactions and reduce friction?
3. **Reliability Support** (20%) - Does this pattern contribute to consistent, dependable performance?
4. **Control Appropriateness** (20%) - Does this pattern provide right amount of user control?
5. **Calm Focus Alignment** (10%) - Does this pattern support distraction-free music listening?

**Pattern Adoption Strategies Evaluation:**

| Strategy | Chrome Omnibox Adaptation | Alacritty Minimalism | Hyprland Balance | Combined Approach |
|----------|---------------------------|----------------------|------------------|-------------------|
| **Confidence (25%)** | 9/10 - Predictable navigation | 8/10 - Clear focus | 7/10 - Requires learning | 9/10 - Balanced clarity |
| **Efficiency (25%)** | 10/10 - Fast unified search | 9/10 - No distractions | 8/10 - Keyboard efficiency | 9/10 - Multiple efficiency paths |
| **Reliability (20%)** | 8/10 - Proven pattern | 10/10 - Simple = reliable | 9/10 - Configurable reliability | 9/10 - Redundant reliability |
| **Control (20%)** | 7/10 - Limited customization | 6/10 - Minimal control | 10/10 - Right amount | 8/10 - Progressive control |
| **Calm Focus (10%)** | 8/10 - Clean interface | 10/10 - Maximum focus | 9/10 - Unobtrusive | 9/10 - Balanced focus |
| **Weighted Score** | **8.45** | **8.40** | **8.55** | **8.90** |

### Transferable UX Patterns

**Navigation Patterns:**
- **Combined search/action interface** (Chrome omnibox) → Unified music search/browse in mpd-client (8.45 weighted score)
- **Keyboard-driven efficiency** (Hyprland) → Comprehensive keyboard shortcuts for power users (8.55 weighted score)
- **Minimal interface chrome** (Alacritty) → Focus on music content rather than application UI (8.40 weighted score)

**Interaction Patterns:**
- **Performance as UX feature** (Alacritty, Chrome) → mpd-client must feel responsive and immediate
- **Progressive disclosure** (Hyprland configuration) → Advanced features available but not in the way
- **Predictable behavior** (all three) → Builds user confidence through consistency

**Visual Patterns:**
- **Clean, distraction-free interfaces** (all three) → Supports calm focus on music
- **Sensible defaults with customization** (Hyprland, Alacritty) → Works immediately but adapts to user preferences
- **Consistent visual language** (Chrome) → Creates cohesive experience across application

### Anti-Patterns to Avoid

**Feature Bloat** (Gnome comparison) - Avoid adding unnecessary features that complicate the core music playback experience.

**Overly Complex Configuration** (Sway vs Hyprland) - Configuration should be accessible but not required for basic use.

**Visual Distraction** - Interface should not compete with music for attention; follow Alacritty's minimal UI approach.

**Inconsistent Performance** - Unlike some Linux applications, mpd-client must maintain consistent responsiveness.

**Hidden Functionality** - Unlike some tiling WMs, core functions should be discoverable without reading documentation.

### Design Inspiration Strategy

**Strategic Recommendation:** Pursue **Combined Approach** (8.90 weighted score) that blends Chrome's navigation efficiency, Alacritty's performance focus, and Hyprland's control balance, adapted specifically for music playback context.

**What to Adopt (High-Scoring Patterns):**
- **Chrome's unified navigation efficiency** (8.45) → Combined search/browse for music discovery
- **Alacritty's performance focus** (8.40) → Optimize library browsing responsiveness as UX feature
- **Hyprland's control balance** (8.55) → Progressive disclosure of advanced features

**What to Adapt (Context-Specific Modifications):**
- **Omnibox pattern** → Adapt to dual-mode context (Album vs Folder search behaviors)
- **Minimalist philosophy** → Balance with necessary music metadata display
- **Configuration approach** → Simplify for music-specific settings (MPD connection, library paths)

**What to Avoid (Low-Value Patterns):**
- **Pure terminal minimalism** → Music requires some metadata display
- **Over-customization** → Keep configuration focused on music playback needs
- **Feature discovery complexity** → Core functions must be immediately discoverable

## Design System Foundation

### Architecture Decision Record: Design System Foundation

**Participants:**
- **Platform Integration Architect (PIA)** - Focuses on native Linux desktop integration
- **Music UX Specialist (MUS)** - Focuses on music playback interaction patterns  
- **Development Efficiency Lead (DEL)** - Focuses on development speed and maintainability
- **Design System Strategist (DSS)** - Focuses on long-term scalability and consistency

**Proposed Options:**
1. **Pure GTK4 Native** - Maximum platform integration, minimal customization
2. **Custom GTK4 Theme with Extensions** - Balanced approach with theme + component extensions
3. **Custom Design System** - Complete visual uniqueness, specialized music components

**Persona Perspectives and Trade-offs:**

**PIA (Platform Integration Architect):**
*"We must prioritize native Linux desktop integration. Pure GTK4 Native ensures perfect compatibility with system themes, accessibility features, and platform conventions. Users expect Linux applications to respect their desktop environment. Customization should not break platform integration."*

**Trade-off:** Platform consistency vs. visual differentiation

**MUS (Music UX Specialist):**
*"Music playback requires specialized visual feedback - hover controls, album grids, track progress visualization. Pure GTK4 lacks music-specific patterns. We need Custom Design System to create optimal music interactions, even if it sacrifices some platform consistency."*

**Trade-off:** Music interaction optimization vs. platform conventions

**DEL (Development Efficiency Lead):**
*"Our team has limited design expertise. Custom GTK4 Theme with Extensions balances development speed with customization. We can start with GTK4 foundation and progressively add music components. This matches our capability while allowing future enhancement."*

**Trade-off:** Development speed vs. feature completeness

**DSS (Design System Strategist):**
*"Long-term we need a scalable system. Custom GTK4 Theme provides foundation for evolution. We can establish design tokens now, extend components gradually. This creates sustainable path from basic theme to sophisticated music design system."*

**Trade-off:** Immediate needs vs. long-term scalability

**Synthesis and Decision:**

**Consensus Decision:** **Custom GTK4 Theme with Extended Components**

**Rationale Synthesis:**
1. **Platform Integration** (PIA satisfied) - GTK4 foundation maintains native integration
2. **Music Interaction Needs** (MUS partially satisfied) - Component extensions allow music-specific patterns
3. **Development Efficiency** (DEL satisfied) - Progressive enhancement matches team capabilities  
4. **Strategic Scalability** (DSS satisfied) - Design tokens enable future evolution

**Explicit Trade-offs Accepted:**
- **Visual differentiation limited** to maintain platform integration
- **Music patterns implemented gradually** via component extensions
- **Immediate feature completeness sacrificed** for sustainable development path
- **Platform conventions prioritized** over complete visual uniqueness

**Implementation Roadmap with Architecture Guardrails:**

**Phase 1: Foundation (Platform Integration Focus)**
- GTK4 Adwaita theme baseline
- Basic color/typography customization respecting platform conventions
- Design token system established

**Phase 2: Music Extensions (Music UX Focus)**
- Album grid component with hover controls
- Track list with technical metadata
- Music playback visual feedback patterns

**Phase 3: Advanced Patterns (Design System Focus)**
- Sophisticated music interaction patterns
- Advanced theming capabilities
- Component library maturity

**Architecture Decision:**
**Custom GTK4 Theme with Extended Components** provides optimal balance across all architect concerns, with explicit trade-offs documented and phased implementation addressing each persona's priorities.