//! Online cover lookup provider — MusicBrainz + Cover Art Archive. Thread: MPD background thread.
//!
//! This module is gated behind the `online-cover-art` feature flag (disabled by default).
//! It queries MusicBrainz to find a release MBID by artist + album name, then downloads
//! cover art from the Cover Art Archive. Results are cached like any other cover source.
//!
//! # Privacy
//! Only artist and album name are sent in the MusicBrainz search query. No user-identifiable
//! information is transmitted. The feature is opt-in and disabled by default.
//!
//! # Rate Limiting
//! MusicBrainz and Cover Art Archive both enforce 1 request/second. This provider implements
//! its own rate limiter to stay within those bounds.
//!
//! # Error Handling
//! - HTTP 429 (Too Many Requests): exponential backoff, retry up to 5 times
//! - HTTP 404 (Not Found): no retry (album has no cover art on CAA)
//! - HTTP 5xx / network errors: exponential backoff, retry up to 5 times

#![cfg(feature = "online-cover-art")]

use std::time::{Duration, Instant};

use serde::Deserialize;
use ureq::Error;

use crate::config::current_month;
use crate::mpd::state_machine::{MpdEvent, ToastLevel};

/// The User-Agent sent with all MusicBrainz API requests.
/// MusicBrainz requires a unique per-application User-Agent string.
const USER_AGENT: &str = concat!("mpd-client/", env!("CARGO_PKG_VERSION"));

/// MusicBrainz release search API endpoint.
const MB_SEARCH_URL: &str = "https://musicbrainz.org/ws/2/release/";

/// Cover Art Archive download endpoint (250px thumbnail).
const CAA_DOWNLOAD_URL: &str = "https://coverartarchive.org/release";

/// Default rate limit in seconds between API requests.
const DEFAULT_RATE_LIMIT_SECS: f64 = 1.0;

/// Maximum number of retries for transient failures.
const MAX_RETRIES: u32 = 5;

/// Initial backoff delay in seconds.
const INITIAL_BACKOFF_SECS: f64 = 1.0;

/// Maximum backoff delay in seconds.
const MAX_BACKOFF_SECS: f64 = 16.0;

/// Online cover lookup provider.
///
/// Queries MusicBrainz to resolve artist + album to a release MBID, then downloads
/// cover art from the Cover Art Archive. Rate-limited to 1 request/second by default.
pub struct CoverOnlineProvider {
    /// Timestamp of the last API request for rate limiting.
    last_request: Instant,
    /// Minimum interval between API requests in seconds.
    rate_limit_secs: f64,
}

/// A single match from the MusicBrainz release search response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MbRelease {
    id: String,
    title: String,
    score: u32,
    #[serde(rename = "artist-credit")]
    artist_credit: Vec<MbArtistCredit>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MbArtistCredit {
    name: String,
}

/// Top-level MusicBrainz search response.
#[derive(Debug, Deserialize)]
struct MbSearchResponse {
    releases: Vec<MbRelease>,
}

impl CoverOnlineProvider {
    /// Create a new CoverOnlineProvider with the default rate limit (1 req/s).
    pub fn new() -> Self {
        Self {
            last_request: Instant::now(),
            rate_limit_secs: DEFAULT_RATE_LIMIT_SECS,
        }
    }

    /// Create a new CoverOnlineProvider with a custom rate limit.
    ///
    /// `rate_limit_secs` — minimum interval between API requests in seconds.
    pub fn with_rate_limit(rate_limit_secs: f64) -> Self {
        Self {
            last_request: Instant::now(),
            rate_limit_secs,
        }
    }

    /// Look up cover art for the given artist and album.
    ///
    /// 1. Checks monthly data cap (if cap_mb > 0) — returns None if exceeded.
    /// 2. Enforces the rate limit (sleeps if called too soon after the last request).
    /// 3. Searches MusicBrainz for a matching release.
    /// 4. Downloads the cover image from Cover Art Archive.
    /// 5. Returns raw image bytes on success, `None` on failure.
    ///
    /// Byte counts from HTTP responses are added to `bytes_downloaded`.
    /// When the cap is first exceeded, a `MpdEvent::Toast` is emitted on `event_tx`.
    /// Month boundary is checked against `last_reset_month`.
    pub fn lookup(
        &mut self,
        artist: &str,
        album: &str,
        cap_mb: u64,
        bytes_downloaded: &mut u64,
        last_reset_month: &mut u32,
        event_tx: &std::sync::mpsc::SyncSender<MpdEvent>,
    ) -> Option<Vec<u8>> {
        log::debug!("[online_cover] lookup '{album}' by '{artist}'");

        // Step 0: Month boundary check — reset counter on calendar month rollover
        let now_month = current_month();
        if now_month != *last_reset_month {
            log::info!(
                "[online_cover] Month boundary crossed ({} → {now_month}), resetting byte counter",
                *last_reset_month
            );
            *bytes_downloaded = 0;
            *last_reset_month = now_month;
        }

        // Step 1: Cap check — suspend lookups if monthly cap already exceeded
        let cap_bytes = cap_mb.saturating_mul(1024 * 1024);
        if cap_mb > 0 && *bytes_downloaded >= cap_bytes {
            log::info!(
                "[online_cover] Monthly data cap ({cap_mb} MB) reached, lookups suspended"
            );
            return None;
        }

        // Step 2: Rate limiting
        self.enforce_rate_limit();

        // Step 3: Search MusicBrainz for the release MBID
        let mbid = self.search_musicbrainz(artist, album, bytes_downloaded)?;

        // Step 4: Download cover from Cover Art Archive with backoff on retryable errors
        let result = self.download_cover_with_backoff(&mbid, album, bytes_downloaded);

        // Emit toast when cap is first exceeded (the lookup that pushed us over the limit)
        if cap_mb > 0 && *bytes_downloaded >= cap_bytes {
            let _ = event_tx.try_send(MpdEvent::Toast {
                message: "Online cover lookup paused — monthly data cap reached".into(),
                level: ToastLevel::Warn,
            });
        }

        result
    }

    /// Wait if the time since the last request is less than the rate limit.
    fn enforce_rate_limit(&self) {
        let elapsed = self.last_request.elapsed().as_secs_f64();
        if elapsed < self.rate_limit_secs {
            let wait = self.rate_limit_secs - elapsed;
            log::debug!("[online_cover] Rate limit: waiting {wait:.1}s");
            std::thread::sleep(Duration::from_secs_f64(wait));
        }
    }

    /// Search MusicBrainz for a release matching the given artist and album.
    /// Adds the HTTP response body size to `bytes_downloaded`.
    fn search_musicbrainz(
        &mut self,
        artist: &str,
        album: &str,
        bytes_downloaded: &mut u64,
    ) -> Option<String> {
        let query_str = format!("artist:{} AND release:{}", artist, album);
        let url = format!("{MB_SEARCH_URL}?query={}&fmt=json", urlencode(&query_str));

        log::debug!(
            "[online_cover] MusicBrainz search: artist='{artist}' album='{album}'"
        );

        // Update last_request before the HTTP call so rate limiter is accurate even on failure
        self.last_request = Instant::now();

        // Send request and handle the response
        let response = match ureq::get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .call()
        {
            Ok(resp) => resp,
            Err(Error::StatusCode(code)) => {
                log::debug!(
                    "[online_cover] MusicBrainz search returned HTTP {code} for '{album}'"
                );
                return None;
            }
            Err(e) => {
                log::debug!(
                    "[online_cover] MusicBrainz search error for '{album}': {e}"
                );
                return None;
            }
        };

        // Read the response body
        let body = match response.into_body().read_to_vec() {
            Ok(b) => b,
            Err(e) => {
                log::error!("[online_cover] Failed to read MusicBrainz response: {e}");
                return None;
            }
        };

        // Count bytes downloaded
        *bytes_downloaded = bytes_downloaded.saturating_add(body.len() as u64);

        // Parse the JSON response
        let search_response: MbSearchResponse = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                log::error!(
                    "[online_cover] Failed to parse MusicBrainz response for '{album}': {e}"
                );
                return None;
            }
        };

        // Pick the best match (first result with score > 50)
        let release = search_response.releases.into_iter().find(|r| r.score > 50)?;

        log::debug!(
            "[online_cover] MusicBrainz found release {} (score={}) for '{album}'",
            release.id,
            release.score
        );
        Some(release.id)
    }

    /// Download cover art from Cover Art Archive with exponential backoff retry.
    /// Adds the HTTP response body size to `bytes_downloaded`.
    fn download_cover_with_backoff(
        &mut self,
        mbid: &str,
        album: &str,
        bytes_downloaded: &mut u64,
    ) -> Option<Vec<u8>> {
        // Rate limit before the CAA request
        self.enforce_rate_limit();

        let url = format!("{CAA_DOWNLOAD_URL}/{mbid}/front-250");
        log::debug!("[online_cover] CAA download: {url}");

        let mut delay = INITIAL_BACKOFF_SECS;

        for attempt in 1..=MAX_RETRIES {
            // Update rate-limited timestamp before each HTTP attempt,
            // even on retries — every request counts toward the rate limit.
            self.last_request = Instant::now();

            match ureq::get(&url)
                .header("User-Agent", USER_AGENT)
                .call()
            {
                Ok(response) => {
                    let data = match response.into_body().read_to_vec() {
                        Ok(d) => d,
                        Err(e) => {
                            log::error!(
                                "[online_cover] Failed to read CAA response for '{album}': {e}"
                            );
                            return None;
                        }
                    };
                    // Count bytes downloaded
                    *bytes_downloaded = bytes_downloaded.saturating_add(data.len() as u64);
                    log::debug!(
                        "[online_cover] CAA download successful for '{album}' ({} bytes)",
                        data.len()
                    );
                    return Some(data);
                }
                Err(Error::StatusCode(code)) => {
                    match code {
                        404 => {
                            log::debug!(
                                "[online_cover] CAA returned 404 for '{album}' — no cover art"
                            );
                            return None;
                        }
                        429 | 500..=599 => {
                            if attempt < MAX_RETRIES {
                                log::debug!(
                                    "[online_cover] CAA returned HTTP {code} for '{album}', retry {attempt}/{MAX_RETRIES}"
                                );
                                std::thread::sleep(Duration::from_secs_f64(delay));
                                delay = (delay * 2.0).min(MAX_BACKOFF_SECS);
                                continue;
                            }
                            log::debug!(
                                "[online_cover] CAA HTTP {code} for '{album}', all retries exhausted"
                            );
                            return None;
                        }
                        other => {
                            log::error!(
                                "[online_cover] CAA returned HTTP {other} for '{album}'"
                            );
                            return None;
                        }
                    }
                }
                Err(e) => {
                    // Transport/network error — retry
                    if attempt < MAX_RETRIES {
                        log::debug!(
                            "[online_cover] CAA transport error for '{album}': {e}, retry {attempt}/{MAX_RETRIES}"
                        );
                        std::thread::sleep(Duration::from_secs_f64(delay));
                        delay = (delay * 2.0).min(MAX_BACKOFF_SECS);
                        continue;
                    }
                    log::debug!(
                        "[online_cover] CAA transport error for '{album}', all retries exhausted"
                    );
                    return None;
                }
            }
        }

        None
    }
}

impl Default for CoverOnlineProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// URL-encode a string for use in query parameters.
fn urlencode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => {
                result.push_str("%20");
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoding_spaces() {
        assert_eq!(urlencode("hello world"), "hello%20world");
    }

    #[test]
    fn test_urlencoding_special_chars() {
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn test_urlencoding_alphanumeric() {
        assert_eq!(urlencode("Hello123"), "Hello123");
    }

    #[test]
    fn test_musicbrainz_response_parsing() {
        let json = r#"{
            "releases": [
                {
                    "id": "123e4567-e89b-12d3-a456-426614174000",
                    "title": "Test Album",
                    "score": 100,
                    "artist-credit": [
                        { "name": "Test Artist" }
                    ]
                },
                {
                    "id": "223e4567-e89b-12d3-a456-426614174001",
                    "title": "Test Album (Remastered)",
                    "score": 45,
                    "artist-credit": [
                        { "name": "Test Artist" }
                    ]
                }
            ]
        }"#;

        let response: MbSearchResponse =
            serde_json::from_str(json).expect("Should parse MusicBrainz response");

        assert_eq!(response.releases.len(), 2);
        assert_eq!(
            response.releases[0].id,
            "123e4567-e89b-12d3-a456-426614174000"
        );
        assert_eq!(response.releases[0].title, "Test Album");
        assert_eq!(response.releases[0].score, 100);
        assert_eq!(response.releases[0].artist_credit[0].name, "Test Artist");
    }

    #[test]
    fn test_empty_musicbrainz_response() {
        let json = r#"{"releases": []}"#;
        let response: MbSearchResponse =
            serde_json::from_str(json).expect("Should parse empty response");
        assert!(response.releases.is_empty());
    }

    #[test]
    fn test_rate_limit_default() {
        let provider = CoverOnlineProvider::new();
        assert!((provider.rate_limit_secs - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rate_limit_custom() {
        let provider = CoverOnlineProvider::with_rate_limit(2.5);
        assert!((provider.rate_limit_secs - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_score_filtering() {
        let json = r#"{
            "releases": [
                {
                    "id": "low-score-mbid",
                    "title": "Low Score Album",
                    "score": 30,
                    "artist-credit": [{"name": "Artist"}]
                },
                {
                    "id": "high-score-mbid",
                    "title": "High Score Album",
                    "score": 95,
                    "artist-credit": [{"name": "Artist"}]
                }
            ]
        }"#;

        let response: MbSearchResponse =
            serde_json::from_str(json).expect("Should parse");

        let best = response.releases.into_iter().find(|r| r.score > 50);
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, "high-score-mbid");
    }

    #[test]
    fn test_no_match_over_score_threshold() {
        let json = r#"{
            "releases": [
                {
                    "id": "low-mbid",
                    "title": "Low Album",
                    "score": 10,
                    "artist-credit": [{"name": "Artist"}]
                }
            ]
        }"#;

        let response: MbSearchResponse =
            serde_json::from_str(json).expect("Should parse");

        let best = response.releases.into_iter().find(|r| r.score > 50);
        assert!(best.is_none());
    }
}
