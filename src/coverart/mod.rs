//! Cover art subsystem — CoverProvider (fast cache read) + ActualRead (background fetch).
//! Thread: background MPD thread for writes, any thread for reads.

pub mod actual_read;
pub mod provider;

pub use actual_read::ActualRead;
pub use provider::{CachedCover, CoverProvider};
