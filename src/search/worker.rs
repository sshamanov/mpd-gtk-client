//! Search worker — dedicated thread for search index builds and queries.
//! Thread: persistent background thread (1, spawned once at app start).
//!
//! Owns `SearchIndex`. Receives `SearchCommand` via channel from the GTK thread,
//! executes index builds, queries, and resets, then emits `MpdEvent::SearchResults`
//! with scored results. No MPD protocol knowledge, no GTK imports.
//!
//! Architecture: architecture.md §2240-2244 (6-thread model)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crate::mpd::state_machine::MpdEvent;
use crate::search::SearchIndex;

/// Commands the GTK thread sends to the search worker.
pub enum SearchCommand {
    /// Rebuild the index from a flat (artist, album) list.
    BuildIndex(Vec<(String, String)>),
    /// Execute a search query and emit results with the given generation number and cap.
    Search(String, u64, usize),
    /// Clear the index.
    Reset,
}

/// Newtype wrapper for the search command sender, exposed to the UI thread.
#[derive(Clone)]
pub struct SearchCommandSender {
    tx: mpsc::SyncSender<SearchCommand>,
}

impl SearchCommandSender {
    pub fn new(tx: mpsc::SyncSender<SearchCommand>) -> Self {
        Self { tx }
    }

    pub fn send(&self, cmd: SearchCommand) {
        if let Err(e) = self.tx.try_send(cmd) {
            log::warn!("[search-worker] Command dropped — channel full: {e:?}");
        }
    }
}

type EventSender = mpsc::SyncSender<MpdEvent>;

/// Spawn the Search worker thread.
///
/// Receives `SearchCommand` from the GTK thread via `cmd_rx`, owns the
/// `SearchIndex`, and emits `MpdEvent::SearchResults` via `event_tx`.
pub fn spawn(
    cmd_rx: mpsc::Receiver<SearchCommand>,
    event_tx: EventSender,
    stop: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("search-worker".into())
        .spawn(move || {
            log::info!("[search-worker] Thread started");

            let mut index = SearchIndex::new();
            let mut index_ready = false;

            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match cmd_rx.recv_timeout(Duration::from_millis(500)) {
                        Ok(SearchCommand::BuildIndex(albums)) => {
                            index.build(&albums);
                            index_ready = true;
                            log::debug!(
                                "[search-worker] Index built: {} albums",
                                index.album_count()
                            );
                        }
                        Ok(SearchCommand::Search(query, generation, cap)) => {
                            if !index_ready {
                                log::debug!(
                                    "[search-worker] Query '{}' (gen {}) before index ready — signaling SearchIndexing",
                                    query,
                                    generation
                                );
                                let _ = event_tx.try_send(MpdEvent::SearchIndexing);
                                return Ok(());
                            }
                            let (scored, total) = index.search(&query, cap);
                            log::debug!(
                                "[search-worker] Query '{}' (gen {}) returned {} of {} results",
                                query,
                                generation,
                                scored.len(),
                                total
                            );
                            let results: Vec<(String, String)> = scored
                                .into_iter()
                                .map(|(a, b, _)| (a, b))
                                .collect();
                            let _ = event_tx.try_send(MpdEvent::SearchResults { results, generation, total });
                        }
                        Ok(SearchCommand::Reset) => {
                            index = SearchIndex::new();
                            index_ready = false;
                            log::debug!("[search-worker] Index reset");
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            // Signal the outer loop to break
                            return Err(mpsc::RecvTimeoutError::Disconnected);
                        }
                    }
                    Ok(())
                }));

                match result {
                    Ok(Ok(())) => {} // Normal processing
                    Ok(Err(mpsc::RecvTimeoutError::Disconnected)) => break,
                    Ok(Err(_)) => unreachable!(),
                    Err(panic_info) => {
                        let msg = panic_info
                            .downcast_ref::<&str>()
                            .map(|s| *s)
                            .or_else(|| panic_info.downcast_ref::<String>().map(|s| s.as_str()))
                            .unwrap_or("unknown panic");
                        log::error!("[search-worker] Panic recovered: {msg}; resetting index");
                        index = SearchIndex::new();
                    }
                }
            }

            log::info!("[search-worker] Thread terminated");
        })
        .expect("Failed to spawn search-worker thread");
}
