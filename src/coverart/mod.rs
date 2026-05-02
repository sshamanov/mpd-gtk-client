//! Cover art subsystem — CoverProvider (fast cache read) + ActualRead (background fetch).
//! Thread: background MPD thread for writes, any thread for reads.

pub mod actual_read;
pub mod provider;
#[cfg(feature = "online-cover-art")]
pub mod online;

pub use actual_read::ActualRead;

/// Composite cache key: artist + separator + album name, normalized to lowercase.
/// Album name alone isn't unique (e.g., "Greatest Hits" by multiple artists),
/// and MPD may return the same album with different casings.
pub fn cover_key(artist: &str, album: &str) -> String {
    format!("{}||{}", artist.to_lowercase(), album.to_lowercase())
}
#[cfg(feature = "online-cover-art")]
pub use online::CoverOnlineProvider;
pub use provider::{CachedCover, CoverProvider};
