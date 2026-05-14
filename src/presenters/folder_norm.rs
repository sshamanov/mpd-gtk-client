//! Folder normalization — detects CUE sheets and DSD folders, groups related entries.
//!
//! Pure functions: no GTK types. Takes `DirEntry` lists, returns `NormalizedEntry` lists.

use crate::mpd::DirEntry;

/// Outcome of normalizing a directory's entries.
#[derive(Debug, Clone)]
pub struct NormalizationResult {
    /// Normalized entry list (files + synthetic group rows).
    pub entries: Vec<NormalizedEntry>,
    /// Whether the directory contains a .cue file.
    pub has_cue: bool,
    /// Whether the directory contains DSD-format files.
    pub has_dsd: bool,
}

/// A single entry in the normalized folder listing.
#[derive(Debug, Clone, PartialEq)]
pub enum NormalizedEntry {
    File { path: String, name: String },
    CueSummary { path: String },
    DsdGroup { name: String, format: String },
}

/// Check if any file in the directory listing has a `.cue` extension.
pub fn has_cue_sheet(entries: &[DirEntry]) -> bool {
    entries.iter().any(|e| match e {
        DirEntry::File { name, .. } => name.to_lowercase().ends_with(".cue"),
        _ => false,
    })
}

/// Check if any file has a DSD-like file extension.
pub fn has_dsd_files(entries: &[DirEntry]) -> bool {
    entries.iter().any(|e| match e {
        DirEntry::File { name, .. } => {
            let lower = name.to_lowercase();
            lower.ends_with(".dsf") || lower.ends_with(".dff")
        }
        _ => false,
    })
}

/// Check whether a filename ends with the given suffix (case-insensitive).
pub fn ends_with_ci(name: &str, suffix: &str) -> bool {
    let name_lower = name.to_lowercase();
    let suffix_lower = suffix.to_lowercase();
    name_lower.ends_with(&suffix_lower)
}

/// Check if a file extension represents a DSD file.
pub fn is_dsd_extension(name: &str) -> bool {
    ends_with_ci(name, ".dsf") || ends_with_ci(name, ".dff")
}

/// Check if a file is a cue sheet.
pub fn is_cue_file(name: &str) -> bool {
    ends_with_ci(name, ".cue")
}

/// Normalize a directory listing: detect CUE, DSD, and annotate entries.
pub fn normalize_entries(entries: &[DirEntry]) -> NormalizationResult {
    let mut result = Vec::new();
    let has_cue = has_cue_sheet(entries);
    let has_dsd = has_dsd_files(entries);

    for entry in entries {
        match entry {
            DirEntry::File { name, path, .. } => {
                if has_cue && is_cue_file(name) {
                    result.push(NormalizedEntry::CueSummary {
                        path: path.clone(),
                    });
                } else {
                    result.push(NormalizedEntry::File {
                        path: path.clone(),
                        name: name.clone(),
                    });
                }
            }
            DirEntry::Directory { name, path, .. } => {
                result.push(NormalizedEntry::File {
                    path: path.clone(),
                    name: name.clone(),
                });
            }
            DirEntry::Playlist { name, path } => {
                result.push(NormalizedEntry::File {
                    path: path.clone(),
                    name: name.clone(),
                });
            }
        }
    }

    if has_dsd {
        // Collect DSD file names for the group entry
        let dsd_names: Vec<&str> = entries
            .iter()
            .filter_map(|e| match e {
                DirEntry::File { name, .. } if is_dsd_extension(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        if !dsd_names.is_empty() {
            let format = if dsd_names.iter().any(|n| ends_with_ci(n, ".dsf")) {
                "DSD (DSF)".to_string()
            } else {
                "DSD (DFF)".to_string()
            };
            result.insert(
                0,
                NormalizedEntry::DsdGroup {
                    name: format!("{} files", dsd_names.len()),
                    format,
                },
            );
        }
    }

    NormalizationResult {
        entries: result,
        has_cue,
        has_dsd,
    }
}
