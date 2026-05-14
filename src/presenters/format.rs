//! Format presenters — format badge and year badge display text.
//!
//! Pure functions: no GTK types, no side effects.
//! `AudioFormat::display_text()` remains on the struct in `mpd::state_machine`.

use std::collections::HashMap;

/// Build a compact format badge from MPD currentsong audio metadata.
pub fn format_badge(song: &HashMap<String, String>) -> Option<String> {
    // Check for DSD audio first (Audio field: "dsd64", "dsd128", etc.)
    if let Some(audio) = song.get("Audio") {
        let lower = audio.to_lowercase();
        if lower.starts_with("dsd") {
            return Some(audio.to_uppercase());
        }
    }
    // PCM: use Format field (e.g., "44100:24:2" → "24/44.1")
    // DSD via format: "2822400:1:2" → "DSD64"
    if let Some(format) = song.get("Format") {
        let parts: Vec<&str> = format.split(':').collect();
        if parts.len() >= 2 {
            if let Ok(rate) = parts[0].parse::<u32>() {
                // DSD rates: 2822400 = DSD64, 5644800 = DSD128, etc.
                let dsd_base: u32 = 44100 * 64;
                if rate >= dsd_base && rate % dsd_base == 0 {
                    let mult = rate / dsd_base;
                    return Some(format!("DSD{}", mult * 64));
                }
            }
            if parts.len() >= 2 {
                if let Ok(bits) = parts[1].parse::<u32>() {
                    if parts.len() == 3 {
                        let rate_str = if let Ok(r) = parts[0].parse::<f64>() {
                            format!("{:.1}", r / 1000.0)
                                .trim_end_matches('0')
                                .trim_end_matches('.')
                                .to_string()
                        } else {
                            parts[0].to_string()
                        };
                        return Some(format!("{}/{}", bits, rate_str));
                    }
                }
                return Some(format.to_string());
            }
        }
    }
    None
}

/// Format the year badge: `'84` for years < 2000, full 4 digits for >= 2000.
pub fn year_badge(year: Option<&str>) -> Option<String> {
    year.and_then(|y| {
        let y_num: u32 = y.parse().ok()?;
        if y_num < 2000 {
            Some(format!("'{}", &y[y.len().saturating_sub(2)..]))
        } else {
            Some(y.to_string())
        }
    })
}
