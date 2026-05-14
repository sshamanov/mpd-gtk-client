# Story 31.6: Orphaned Cache File GC / LRU Eviction

Status: done

## Story

As a developer,
I want orphaned `{md5}.jpg` files in the cover cache to be cleaned up,
so that the cache directory does not grow unboundedly when album art changes.

## Acceptance Criteria

1. **Orphaned file garbage collection at startup**
   - Given the cover cache contains `.jpg` files that are no longer referenced by any entry in `index.json`
   - When the application starts (or periodically, e.g., every 1000 cache writes)
   - Then orphaned files are deleted
   - And a debug log reports the number of files removed

2. **LRU eviction when cache exceeds size limit**
   - Given the cache directory exceeds a configurable size limit (default: 1GB)
   - When a new cover is written to cache
   - Then old or least-recently-used entries are evicted until the cache size is below the limit
   - And evicted entries are removed from index.json atomically

3. **Configurable cache size limit**
   - Given the application is configured
   - When the user sets `[cover_cache].max_size_mb` in config.toml
   - Then the eviction threshold is adjusted accordingly
   - And the default is 1000MB

## Technical Requirements

- GC scan: list all `.jpg` files in the cache directory, cross-reference against all MD5 hashes in `index.json`, delete unreferenced files.
- LRU: track last-access time from filesystem mtime, evict oldest entries when over budget.
- GC runs at startup (fast scan, <100ms for 10K files) and optionally on a background timer (every 1000 writes or every 10 minutes).
- Must not run concurrently with cache writes — coordinate via the Cover Proc worker lifecycle (e.g., run GC only when the worker is idle).
- Config entry: `[cover_cache] max_size_mb = 1000` in `config.toml`, read at startup and on config reload.

## References
- [Source: epics.md] Epic 31: Cover Pipeline Reliability — Story 31.6
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — Orphaned {md5}.jpg files accumulate when cover art changes
- [Source: src/coverart/cover_proc.rs] Cache write path
- [Source: src/coverart/provider.rs] Cache read path and index management
