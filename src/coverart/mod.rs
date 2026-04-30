//! Cover art subsystem — CoverProvider (fast cache read) + ActualRead (background fetch).
//! Thread: background MPD thread for writes, any thread for reads.

pub mod actual_read;
pub mod provider;
#[cfg(feature = "online-cover-art")]
pub mod online;

pub use actual_read::ActualRead;
#[cfg(feature = "online-cover-art")]
pub use online::CoverOnlineProvider;
pub use provider::{CachedCover, CoverProvider};
