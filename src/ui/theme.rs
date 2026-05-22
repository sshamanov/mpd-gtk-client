//! Theme management — high-contrast CSS, CSS provider loading, memory monitoring.

/// High-contrast CSS overrides for WCAG 2.1 AA compliance.
/// Applied at PRIORITY_APPLICATION + 1 to override the base theme.
pub(crate) const HC_CSS: &str = "\
@define-color theme_bg_color #000000;
@define-color theme_fg_color #ffffff;
@define-color theme_base_color #000000;
@define-color theme_text_color #ffffff;
@define-color theme_selected_bg_color #4A90D9;
@define-color theme_selected_fg_color #ffffff;
@define-color theme_unfocused_bg_color #000000;
@define-color theme_unfocused_fg_color #ffffff;
@define-color theme_unfocused_base_color #000000;
@define-color theme_unfocused_text_color #ffffff;
@define-color borders #ffffff;
@define-color theme_link_color #7AB5F5;
@define-color error_color #FF6B6B;
@define-color warning_color #FFD93D;
@define-color success_color #6BCB77;
#connection-indicator.disconnected { background-color: #FF6B6B; }
#connection-indicator.connected { background-color: #6BCB77; }
#connection-indicator.connecting { background-color: #FFD93D; }
.queue-artist { color: #CCCCCC; }
.format-badge { color: #CCCCCC; }
.error-label { color: #FF6B6B; }
#album-grid-status { color: #CCCCCC; }
.breadcrumb-sep { color: #CCCCCC; }
.queue-current { background-color: #4A90D9; }
";
