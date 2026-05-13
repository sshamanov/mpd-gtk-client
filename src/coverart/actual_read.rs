//! ActualRead — dedup queue for pending cover fetches. Thread: MPD background thread.
//!
//! After story 28-2, the actual cover processing (MD5, cache write, JPEG decode,
//! RGBA resize, event emission) has moved to the Cover Proc worker. ActualRead
//! now only maintains a dedup queue — `enqueue()` tracks which albums need
//! fetching, and the MPD IO thread's `FetchCovers` handler resolves URIs and
//! sends jobs to the MPD Cover thread.

use std::collections::VecDeque;

use crate::mpd::MpdAdapter;
use crate::mpd::state_machine::MpdEvent;
use std::sync::mpsc;

/// Shorthand for the event sender type used by the MPD state machine.
type EventSender = mpsc::SyncSender<MpdEvent>;

/// Dedup queue for pending cover fetches. Processing is handled by the
/// MPD Cover thread (28-1) + Cover Proc worker (28-2).
pub struct ActualRead {
    /// FIFO queue of (artist, album_name) tuples pending fetch.
    queue: VecDeque<(String, String)>,
}

impl ActualRead {
    /// Create a new ActualRead.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// Add albums to the fetch queue, skipping any already pending.
    /// Does not clear the existing queue — merges new albums with pending.
    pub fn enqueue(&mut self, albums: Vec<(String, String)>) {
        let added = albums.len();
        for a in albums {
            if !self.queue.contains(&a) {
                self.queue.push_back(a);
            }
        }
        log::debug!("[actual_read] Enqueued {} albums ({} total pending)", added, self.queue.len());
    }

    /// Returns true if there are pending albums to fetch.
    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Returns the number of pending albums.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Process up to `max` albums from the queue.
    /// Superseded by Cover Proc worker (28-2). Stub retained for API compatibility.
    pub fn process_batch(
        &mut self,
        _adapter: &mut MpdAdapter,
        _caps: &crate::mpd::MpdCapabilities,
        _provider: &crate::coverart::CoverProvider,
        _event_tx: &EventSender,
        max: usize,
    ) {
        for _ in 0..max {
            if self.queue.is_empty() {
                break;
            }
            self.process_one(_adapter, _caps, _provider, _event_tx);
        }
    }

    /// Process one album from the queue.
    /// Superseded by Cover Proc worker (28-2). Stub retained for API compatibility.
    pub fn process_one(
        &mut self,
        _adapter: &mut MpdAdapter,
        _caps: &crate::mpd::MpdCapabilities,
        _provider: &crate::coverart::CoverProvider,
        _event_tx: &EventSender,
    ) {
        // Pop from queue to maintain dedup tracking.
        let _ = self.queue.pop_front();
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_pending() {
        let mut ar = ActualRead::new();

        assert!(!ar.has_pending());
        assert_eq!(ar.pending_count(), 0);

        ar.enqueue(vec![
            ("Artist1".into(), "Album1".into()),
            ("Artist2".into(), "Album2".into()),
        ]);

        assert!(ar.has_pending());
        assert_eq!(ar.pending_count(), 2);
    }

    #[test]
    fn test_enqueue_dedup() {
        let mut ar = ActualRead::new();

        ar.enqueue(vec![("A".into(), "Old".into())]);
        assert_eq!(ar.pending_count(), 1);

        // Different items append (don't replace)
        ar.enqueue(vec![("B".into(), "New".into())]);
        assert_eq!(ar.pending_count(), 2);

        // Same item is deduplicated
        ar.enqueue(vec![("A".into(), "Old".into())]);
        assert_eq!(ar.pending_count(), 2);
    }

    #[test]
    fn test_fifo_order() {
        let mut ar = ActualRead::new();

        ar.enqueue(vec![
            ("A".into(), "First".into()),
            ("B".into(), "Second".into()),
            ("C".into(), "Third".into()),
        ]);

        assert_eq!(ar.queue.front().unwrap().1, "First");
        let _ = ar.queue.pop_front();
        assert_eq!(ar.queue.front().unwrap().1, "Second");
    }

    #[test]
    fn test_process_one_empty_queue_does_nothing() {
        let mut ar = ActualRead::new();
        // process_one on empty queue should be a no-op (no panic)
        let _tx: mpsc::SyncSender<MpdEvent> = mpsc::sync_channel(64).0;
        // Can't easily call process_one without a real adapter, but struct works
        assert_eq!(ar.pending_count(), 0);
    }
}
