//! Performance profiling instrumentation — timed operations logging.
//!
//! All macros compile to nothing in release builds (`cfg!(debug_assertions)`).
//! In debug builds, they log elapsed time for critical operations.
//! Operations exceeding the WARN_THRESHOLD are logged at warn level.

#[allow(dead_code)]
const WARN_THRESHOLD: std::time::Duration = std::time::Duration::from_millis(100);

/// Time an operation and log the duration at debug level.
///
/// Usage: `log_duration!("operation name", { /* code */ })`
///
/// If the operation takes longer than 100ms, logs at warn level.
#[macro_export]
macro_rules! log_duration {
    ($name:expr, $body:expr) => {{
        #[cfg(debug_assertions)]
        {
            let _start = Instant::now();
            let _result = { $body };
            let _elapsed = _start.elapsed();
            if _elapsed > $crate::profiling::WARN_THRESHOLD {
                log::warn!("[profile] {} took {:?}", $name, _elapsed);
            } else {
                log::debug!("[profile] {} took {:?}", $name, _elapsed);
            }
            _result
        }
        #[cfg(not(debug_assertions))]
        {
            $body
        }
    }};
}
