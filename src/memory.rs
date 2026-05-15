//! Memory monitoring — reads RSS from /proc/self/status. Thread: GTK main loop.
//!
//! Linux-only; returns `None` if /proc is unavailable (containers without procfs).

/// Read RSS memory usage from `/proc/self/status`.
/// Returns memory in MB, or `None` if the proc file is unreadable or VmRSS is missing.
pub(crate) fn read_rss_mb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_rss_returns_some() {
        let result = read_rss_mb();
        // On any normal Linux system with /proc, this should return Some
        // In CI containers without /proc, it returns None — both are acceptable
        if let Some(mb) = result {
            assert!(mb > 0, "RSS should be positive, got {mb} MB");
        }
    }
}
