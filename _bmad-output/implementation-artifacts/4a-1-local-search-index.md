# Story 4a.1: Local Search Index

Status: ready-for-dev

## Story

As a user,
I want search results to appear instantly,
so that browsing feels responsive even with a large library.

## Acceptance Criteria

1. **Build search index** — When albums are loaded, build an in-memory index:
   - Index album title, artist name as lowercase tokens
   - Store as `HashMap<String, Vec<(String, String)>>` (token → matching albums)
   - Thread-safe via `Arc<RwLock<Index>>`

2. **Local search** — When searching, query the local index:
   - Split query into lowercase tokens
   - Intersect result sets across tokens
   - Return deduplicated (artist, album) pairs
   - No MPD round trip needed

3. **Fallback to MPD** — If index is empty (not yet built), fall back to MPD `search any`

4. **`cargo test` passes**

## Tasks / Subtasks

- [ ] Task 1: Build search index module (AC: 1)
- [ ] Task 2: Integrate with album loading (AC: 1)
- [ ] Task 3: Wire search to local index with MPD fallback (AC: 2, 3)
- [ ] Task 4: Verify no regressions (AC: 4)

## Dev Notes

### Index Structure

```rust
pub struct SearchIndex {
    albums: Vec<(String, String)>, // (artist, album)
    index: HashMap<String, Vec<usize>>, // token → album indices
}
```

### Tokenization

```rust
fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase().split_whitespace().map(|t| {
        t.trim_matches(|c: char| !c.is_alphanumeric()).to_string()
    }).filter(|t| !t.is_empty()).collect()
}
```

### What NOT to Do
- Do NOT implement relevance scoring (4a-2)
- Do NOT implement mode-scoped strategies (4a-2)
- Do NOT add new dependencies

## Dev Agent Record

### Completion Notes List

### File List
