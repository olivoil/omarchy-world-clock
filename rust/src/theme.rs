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
  padding: 18px 18px 12px 18px;
  box-shadow: 0 18px 36px {shadow};
}}

.panel-title {{
  color: {foreground};
  font-weight: 700;
  font-size: 18px;
}}

.clock-title {{
  color: {foreground};
  font-weight: 700;
  font-size: 14px;
}}

.clock-context,
.clock-meta {{
  color: {muted_foreground};
  font-size: 12px;
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

button.icon-button {{
  background: transparent;
  border-color: {icon_button_border};
  border-radius: 999px;
  min-width: 34px;
  min-height: 34px;
  padding: 6px;
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

.empty-state-title {{
  color: {foreground};
  font-size: 15px;
  font-weight: 700;
}}

.empty-state-copy {{
  color: {muted_foreground};
  font-size: 13px;
}}

separator {{
  color: {separator};
}}
"#,
        panel_background = rgba(&palette.background, 0.94),
        panel_border = rgba(&palette.accent, 0.42),
        shadow = rgba("#000000", 0.30),
        accent = palette.accent,
        foreground = palette.foreground,
        muted_foreground = rgba(&palette.foreground, 0.72),
        time_chip_background = rgba("#000000", 0.10),
        time_chip_border = rgba(&palette.foreground, 0.12),
        accent_focus_border = rgba(&palette.accent, 0.75),
        accent_focus_shadow = rgba(&palette.accent, 0.14),
        icon_button_border = rgba(&palette.foreground, 0.06),
        icon_button_hover_background = rgba(&palette.foreground, 0.06),
        icon_button_hover_border = rgba(&palette.foreground, 0.16),
        icon_button_active_background = rgba(&palette.accent, 0.10),
        icon_button_active_border = rgba(&palette.accent, 0.30),
        separator = rgba(&palette.foreground, 0.09),
    )
}
