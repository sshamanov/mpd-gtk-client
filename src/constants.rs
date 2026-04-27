//! Layout constants — shell split, rail widths, mode proportions. Thread: any (compile-time constants).

pub const SHELL_SPLIT_RATIO: f64 = 0.7;
pub const RAIL_WIDTH_MIN: f64 = 320.0;
pub const RAIL_WIDTH_MAX: f64 = 420.0;
pub const ALBUM_MODE_RAIL: [f64; 3] = [0.4, 0.2, 0.4];
pub const FOLDER_MODE_RAIL: [f64; 2] = [0.55, 0.45];
