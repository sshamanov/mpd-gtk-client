# Story 5b.2: Keyboard Navigation Audit

Status: done

## Acceptance Criteria

1. **All keyboard shortcuts documented**
2. **`cargo test` passes**

## Existing Keyboard Shortcuts

| Shortcut | Action | Implemented In |
|----------|--------|---------------|
| `Ctrl+Q` | Quit | 1a-1 |
| `Ctrl+1` | Album Mode | 2-1 |
| `Ctrl+2` | Folder Mode | 2-1 |
| `Ctrl+F` | Focus search | 1b-4 |
| `Ctrl+,` | Settings dialog | 5b-1 |
| `Escape` | Clear search | 1b-4 |
| `Delete` | Remove queue item | 3-3 |
| `Shift+Up/Down` | Reorder queue | 3-4 |
| `Left/Backspace` | Navigate to parent directory (Folder Mode) | 2-1 |
| `Arrow keys` | Grid navigation (Album Mode) | 1b-1 |
| `Enter` | Activate selection | 1b-1 |
| Double-click | Play album / play queue item | 1b-2 / 3-2 |

## Completion Notes

- All primary functions have keyboard shortcuts
- Tab/high-contrast audit deferred per spec (WCAG 2.1 AA requires visual testing)
- No code changes needed

## Status

done
