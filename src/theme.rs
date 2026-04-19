use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Palette {
    pub accent: String,
    pub foreground: String,
    pub background: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: "#faa968".to_string(),
            foreground: "#f6dcac".to_string(),
            background: "#05182e".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    accent: Option<String>,
    foreground: Option<String>,
    background: Option<String>,
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn load_palette() -> Palette {
    let path = home_dir().join(".config/omarchy/current/theme/colors.toml");
    let mut palette = Palette::default();
    let Ok(text) = fs::read_to_string(path) else {
        return palette;
    };
    let Ok(theme) = toml::from_str::<ThemeFile>(&text) else {
        return palette;
    };

    if let Some(accent) = theme.accent {
        palette.accent = accent;
    }
    if let Some(foreground) = theme.foreground {
        palette.foreground = foreground;
    }
    if let Some(background) = theme.background {
        palette.background = background;
    }

    palette
}

fn rgba(hex_value: &str, alpha: f32) -> String {
    let trimmed = hex_value.trim_start_matches('#');
    if trimmed.len() != 6 {
        return format!("rgba(0, 0, 0, {alpha:.3})");
    }

    let red = u8::from_str_radix(&trimmed[0..2], 16).unwrap_or(0);
    let green = u8::from_str_radix(&trimmed[2..4], 16).unwrap_or(0);
    let blue = u8::from_str_radix(&trimmed[4..6], 16).unwrap_or(0);
    format!("rgba({red}, {green}, {blue}, {alpha:.3})")
}

pub fn build_css(palette: &Palette) -> String {
    format!(
        r#"
window {{
  background: transparent;
}}

.world-clock-panel {{
  background: {panel_background};
  border: 1px solid {panel_border};
  border-radius: 18px;
  padding: 18px 32px 26px 32px;
  box-shadow: 0 18px 36px {shadow};
}}

.panel-title {{
  color: {foreground};
  font-weight: 700;
  font-size: 18px;
}}

.read-summary-time {{
  color: {foreground};
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 96px;
  font-weight: 700;
  line-height: 0.82;
  letter-spacing: 0;
}}

entry.read-summary-time {{
  color: {foreground};
  caret-color: transparent;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 18px;
  box-shadow: none;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
  padding: 0 18px;
  min-height: 126px;
}}

entry.read-summary-time text {{
  color: {foreground};
  caret-color: transparent;
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 96px;
  font-weight: 700;
  background: transparent;
  border: none;
  box-shadow: none;
  line-height: 0.82;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
  padding: 0;
}}

entry.read-summary-time:focus,
entry.read-summary-time:focus-visible,
entry.read-summary-time:focus-within {{
  background: transparent;
  border-color: transparent;
  box-shadow: none;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
}}

entry.read-summary-time:focus text,
entry.read-summary-time:focus-visible text,
entry.read-summary-time:focus-within text,
entry.read-summary-time text:focus,
entry.read-summary-time text:focus-visible {{
  background: transparent;
  border: none;
  box-shadow: none;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
}}

entry.read-summary-time.error {{
  border-color: rgba(255, 139, 139, 0.92);
}}

.read-summary-location {{
  color: {read_location};
  font-size: 16px;
  font-weight: 600;
}}

.add-screen {{
  margin-top: 4px;
}}

scrolledwindow.search-results-overlay {{
  background: {panel_background};
  border: 1px solid {panel_border};
  border-radius: 18px;
  box-shadow: 0 18px 36px {shadow};
}}

scrolledwindow.search-results-overlay > viewport {{
  background: transparent;
  border-radius: 18px;
}}

.timeline-shell {{
  margin: 0;
}}

.timeline-time {{
  color: {foreground};
  font-size: 16px;
  font-weight: 600;
}}

.timeline-zone {{
  color: {muted_foreground};
  font-size: 13px;
  font-weight: 600;
}}

.timezone-card-grid {{
  margin-top: 0;
}}

.timezone-card-shell {{
  margin-top: 0;
}}

.timezone-card {{
  background: {card_background};
  border: 1px solid {card_border};
  border-radius: 22px;
  padding: 20px 22px;
  box-shadow: 0 14px 32px {card_shadow};
}}

button.card-control-button {{
  min-width: 36px;
  min-height: 36px;
  padding: 0;
  background: {button_background};
  border: 1px solid {time_chip_border};
  border-radius: 999px;
}}

button.card-control-button:hover {{
  background: {icon_button_hover_background};
  border-color: {icon_button_hover_border};
}}

button.card-hover-delete {{
  min-width: 36px;
  min-height: 36px;
  padding: 0;
  border-radius: 999px;
  box-shadow: 0 10px 22px {card_shadow};
}}

button.card-hover-delete image {{
  color: {foreground};
}}

.timezone-card-title {{
  color: {foreground};
  font-size: 19px;
  font-weight: 700;
}}

.timezone-card-time {{
  color: {foreground};
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 52px;
  font-weight: 700;
  line-height: 0.82;
  letter-spacing: 0;
}}

entry.timezone-card-time {{
  caret-color: transparent;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 14px;
  box-shadow: none;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
  padding: 0;
  min-height: 48px;
}}

entry.timezone-card-time text {{
  caret-color: transparent;
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 52px;
  font-weight: 700;
  border: none;
  box-shadow: none;
  line-height: 0.82;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
}}

.timezone-card-subtitle-row {{
  margin-top: -8px;
}}

entry.timezone-card-time:focus,
entry.timezone-card-time:focus-visible,
entry.timezone-card-time:focus-within {{
  background: transparent;
  border-color: transparent;
  box-shadow: none;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
}}

entry.timezone-card-time:focus text,
entry.timezone-card-time:focus-visible text,
entry.timezone-card-time:focus-within text,
entry.timezone-card-time text:focus,
entry.timezone-card-time text:focus-visible {{
  background: transparent;
  border: none;
  box-shadow: none;
  line-height: 0.82;
  outline-color: transparent;
  outline-offset: 0;
  outline-style: none;
  outline-width: 0;
}}

entry.timezone-card-time.error {{
  border-color: rgba(255, 139, 139, 0.92);
}}

.timezone-card-meta {{
  color: {muted_foreground};
  font-size: 14px;
  font-weight: 500;
}}

.clock-title {{
  color: {foreground};
  font-weight: 700;
  font-size: 14px;
}}

.clock-row {{
  border-radius: 14px;
}}

.clock-row.dragging {{
  opacity: 0.18;
}}

.clock-row.drag-preview {{
  background: {panel_background};
  border: 1px solid {drag_preview_border};
  border-radius: 14px;
  padding: 10px 12px;
  box-shadow: 0 14px 28px {drag_shadow};
}}

.drag-preview-time {{
  color: {foreground};
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: 0.1em;
}}

.drag-insert-marker {{
  min-height: 4px;
  border-radius: 999px;
  background: {drag_insert};
}}

.drag-handle-label {{
  color: {drag_handle};
  font-size: 20px;
  font-weight: 700;
}}

.clock-context,
.clock-meta {{
  color: {muted_foreground};
  font-size: 12px;
}}

.hint-label {{
  color: {muted_foreground};
  font-size: 12px;
  font-weight: 700;
}}

.status-label {{
  color: {accent};
  font-size: 12px;
}}

.status-label.error {{
  color: #ff8b8b;
}}

.time-entry {{
  color: {foreground};
  caret-color: {accent};
  background: {time_chip_background};
  border: 1px solid {time_chip_border};
  border-radius: 12px;
  padding: 12px 14px;
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: 0.16em;
}}

.time-entry:focus {{
  border-color: {accent_focus_border};
  box-shadow: 0 0 0 3px {accent_focus_shadow};
}}

.time-entry.error {{
  border-color: rgba(255, 139, 139, 0.92);
}}

entry.search-entry {{
  color: {foreground};
  caret-color: {accent};
  background: {time_chip_background};
  border: 1px solid {time_chip_border};
  border-radius: 10px;
  padding: 9px 12px;
  font-size: 15px;
}}

entry.search-entry:focus {{
  border-color: {accent_focus_border};
  box-shadow: 0 0 0 3px {accent_focus_shadow};
}}

entry.add-search-entry {{
  min-height: 56px;
  border-radius: 18px;
  padding: 0 18px;
  font-size: 24px;
  font-weight: 500;
}}

entry.add-search-entry image {{
  color: {muted_foreground};
  -gtk-icon-size: 24px;
  margin-right: 12px;
}}

.add-map-shell {{
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid {card_border};
  border-radius: 24px;
  padding: 18px;
}}

.map-hover-card {{
  background: {panel_background};
  border: 1px solid {panel_border};
  border-radius: 20px;
  padding: 18px 20px;
  box-shadow: 0 20px 38px {shadow};
}}

.map-hover-title {{
  color: {foreground};
  font-size: 24px;
  font-weight: 700;
}}

.map-hover-time {{
  color: {foreground};
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 34px;
  font-weight: 700;
  letter-spacing: -0.03em;
}}

.map-hover-meta {{
  color: {muted_foreground};
  font-size: 14px;
  font-weight: 500;
}}

.map-legend {{
  margin-top: -4px;
}}

.map-legend-label {{
  color: {muted_foreground};
  font-size: 13px;
  font-weight: 600;
}}

button {{
  color: {foreground};
  background: {button_background};
  border: 1px solid {button_border};
  border-radius: 10px;
  padding: 8px 12px;
}}

button:hover {{
  background: {button_hover_background};
}}

button:focus {{
  border-color: {accent_focus_border};
}}

button.flat-button {{
  background: transparent;
  border-color: transparent;
  padding: 4px 2px;
  min-height: 32px;
  color: {muted_foreground};
  font-size: 15px;
  font-weight: 600;
}}

button.flat-button:hover {{
  background: transparent;
  color: {foreground};
}}

button.flat-button:focus {{
  border-color: transparent;
  box-shadow: none;
}}

button.icon-button {{
  background: transparent;
  border-color: {icon_button_border};
  border-radius: 999px;
  min-width: 32px;
  min-height: 32px;
  padding: 0;
}}

button.icon-button:hover {{
  background: {icon_button_hover_background};
  border-color: {icon_button_hover_border};
}}

button.icon-button:disabled {{
  opacity: 0.28;
}}

button.icon-button.active {{
  background: {icon_button_active_background};
  border-color: {icon_button_active_border};
}}

button.icon-button image {{
  color: {muted_foreground};
  -gtk-icon-size: 15px;
}}

button.icon-button.active image {{
  color: {foreground};
}}

button.remove-button {{
  min-width: 32px;
  min-height: 32px;
  padding: 0;
}}

button.remove-button:disabled {{
  opacity: 0.28;
}}

dropdown.popup-select {{
  min-width: 152px;
}}

dropdown.popup-select > button {{
  color: {foreground};
  background: {time_chip_background};
  border: 1px solid {time_chip_border};
  border-radius: 12px;
  min-height: 42px;
  padding: 10px 16px;
  font-size: 15px;
  font-weight: 600;
}}

dropdown.popup-select > button:hover {{
  background: {icon_button_hover_background};
  border-color: {icon_button_hover_border};
}}

dropdown.popup-select > button:focus {{
  border-color: {accent_focus_border};
  box-shadow: 0 0 0 3px {accent_focus_shadow};
}}

dropdown.popup-select > button arrow {{
  color: {muted_foreground};
  margin-left: 14px;
}}

dropdown.popup-select popover.background contents {{
  background: {panel_background};
  border: 1px solid {panel_border};
  border-radius: 14px;
  min-width: 184px;
  padding: 8px;
  box-shadow: 0 16px 32px {shadow};
}}

dropdown.popup-select popover.background listview {{
  background: transparent;
}}

dropdown.popup-select popover.background row {{
  min-height: 44px;
  padding: 8px 14px;
  border-radius: 12px;
}}

dropdown.popup-select popover.background row:hover,
dropdown.popup-select popover.background row:selected {{
  background: {icon_button_hover_background};
}}

dropdown.popup-select popover.background row label {{
  color: {foreground};
  font-size: 16px;
  font-weight: 600;
}}

dropdown.popup-select popover.background row:selected label {{
  color: {foreground};
}}

dropdown.popup-select popover.background row .popup-select-row {{
  min-width: 0;
}}

dropdown.popup-select popover.background row .popup-select-item-label {{
  color: {foreground};
}}

dropdown.popup-select popover.background row .popup-select-item-check {{
  color: {foreground};
  min-width: 18px;
  font-size: 16px;
  font-weight: 700;
  margin-left: auto;
}}

button.icon-button.destructive:hover {{
  background: rgba(255, 139, 139, 0.12);
  border-color: rgba(255, 139, 139, 0.28);
}}

button.search-result-button {{
  background: rgba(255, 255, 255, 0.01);
  border-color: transparent;
  padding: 10px 12px;
  border-radius: 12px;
  box-shadow: none;
}}

button.search-result-button:hover {{
  background: {search_result_hover_background};
  border-color: transparent;
}}

button.search-result-button:focus {{
  border-color: transparent;
  box-shadow: 0 0 0 3px {accent_focus_shadow};
}}

button.add-toggle {{
  padding: 9px 14px;
}}

.search-result-title {{
  color: {foreground};
  font-size: 14px;
  font-weight: 700;
}}

.search-result-meta {{
  color: {muted_foreground};
  font-size: 12px;
}}

separator {{
  color: {separator};
}}
"#,
        panel_background = rgba(&palette.background, 0.94),
        panel_border = rgba(&palette.accent, 0.42),
        shadow = rgba("#000000", 0.30),
        drag_shadow = rgba("#000000", 0.24),
        card_shadow = rgba("#000000", 0.18),
        accent = palette.accent,
        foreground = palette.foreground,
        read_location = rgba(&palette.foreground, 0.76),
        muted_foreground = rgba(&palette.foreground, 0.72),
        time_chip_background = rgba("#000000", 0.10),
        time_chip_border = rgba(&palette.foreground, 0.12),
        card_background = rgba("#ffffff", 0.045),
        card_border = rgba(&palette.foreground, 0.08),
        accent_focus_border = rgba(&palette.accent, 0.75),
        accent_focus_shadow = rgba(&palette.accent, 0.14),
        drag_preview_border = rgba(&palette.accent, 0.34),
        drag_insert = rgba(&palette.accent, 0.78),
        drag_handle = rgba(&palette.foreground, 0.44),
        button_background = rgba(&palette.background, 0.72),
        button_border = rgba(&palette.foreground, 0.10),
        button_hover_background = rgba(&palette.background, 0.86),
        icon_button_border = rgba(&palette.foreground, 0.06),
        icon_button_hover_background = rgba(&palette.foreground, 0.06),
        icon_button_hover_border = rgba(&palette.foreground, 0.16),
        icon_button_active_background = rgba(&palette.accent, 0.10),
        icon_button_active_border = rgba(&palette.accent, 0.30),
        search_result_hover_background = rgba(&palette.accent, 0.12),
        separator = rgba(&palette.foreground, 0.09),
    )
}
