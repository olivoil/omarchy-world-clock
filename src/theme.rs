use crate::layout::load_window_rounding;
use regex::Regex;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct Palette {
    pub accent: String,
    pub foreground: String,
    pub background: String,
    pub popup_background: String,
    pub popup_foreground: String,
    pub popup_border: String,
    pub popup_background_alpha: f32,
    pub popup_border_alpha: f32,
    pub popup_border_width: i32,
    pub popup_corner_radius: i32,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: "#faa968".to_string(),
            foreground: "#f6dcac".to_string(),
            background: "#05182e".to_string(),
            popup_background: "#05182e".to_string(),
            popup_foreground: "#f6dcac".to_string(),
            popup_border: "#faa968".to_string(),
            popup_background_alpha: 0.94,
            popup_border_alpha: 0.42,
            popup_border_width: 1,
            popup_corner_radius: 18,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    accent: Option<String>,
    foreground: Option<String>,
    background: Option<String>,
    fg: Option<String>,
    bg: Option<String>,
    color0: Option<String>,
    color4: Option<String>,
    color7: Option<String>,
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn state_home() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/state"))
}

fn palette_paths() -> Vec<PathBuf> {
    vec![
        state_home().join("omarchy/current/theme/colors.toml"),
        home_dir().join(".config/omarchy/current/theme/colors.toml"),
    ]
}

fn valid_hex(value: Option<String>) -> Option<String> {
    value.filter(|color| {
        color.len() == 7
            && color.starts_with('#')
            && color[1..]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
    })
}

fn first_hex_color(value: &str) -> Option<String> {
    static HEX: OnceLock<Regex> = OnceLock::new();
    static RGBA: OnceLock<Regex> = OnceLock::new();
    let hex = HEX.get_or_init(|| Regex::new(r"#[0-9A-Fa-f]{6}").expect("valid hex regex"));
    if let Some(value) = hex.find(value) {
        return Some(value.as_str().to_string());
    }
    let rgba = RGBA.get_or_init(|| {
        Regex::new(r"rgba\(([0-9A-Fa-f]{6})(?:[0-9A-Fa-f]{2})?\)").expect("valid rgba regex")
    });
    rgba.captures(value)
        .and_then(|captures| captures.get(1))
        .map(|color| format!("#{}", color.as_str()))
}

fn shell_value<'a>(root: &'a toml::Value, section: &str, key: &str) -> Option<&'a toml::Value> {
    root.get(section)?.get(key)
}

fn shell_text(root: &toml::Value, section: &str, key: &str) -> Option<String> {
    let value = shell_value(root, section, key)?;
    match value {
        toml::Value::String(value) => Some(value.clone()),
        toml::Value::Integer(value) => Some(value.to_string()),
        toml::Value::Float(value) => Some(value.to_string()),
        _ => None,
    }
}

fn shell_number(root: &toml::Value, section: &str, key: &str) -> Option<f32> {
    shell_text(root, section, key)?.parse().ok()
}

fn resolve_shell_color(
    root: &toml::Value,
    value: &str,
    palette: &Palette,
    depth: usize,
) -> Option<String> {
    if depth > 4 {
        return None;
    }
    if let Some(color) = first_hex_color(value) {
        return Some(color);
    }
    match value.trim().to_ascii_lowercase().as_str() {
        "background" => return Some(palette.background.clone()),
        "foreground" | "text" => return Some(palette.foreground.clone()),
        "accent" => return Some(palette.accent.clone()),
        _ => {}
    }
    let (section, key) = value.split_once('.')?;
    let referenced = shell_text(root, section, key)?;
    resolve_shell_color(root, &referenced, palette, depth + 1)
}

fn apply_shell_popup_style(theme_dir: &Path, palette: &mut Palette) {
    let Ok(text) = fs::read_to_string(theme_dir.join("shell.toml")) else {
        return;
    };
    let Ok(shell) = toml::from_str::<toml::Value>(&text) else {
        return;
    };

    palette.popup_background = shell_text(&shell, "popups", "background")
        .and_then(|value| resolve_shell_color(&shell, &value, palette, 0))
        .unwrap_or_else(|| palette.background.clone());
    palette.popup_foreground = shell_text(&shell, "popups", "text")
        .and_then(|value| resolve_shell_color(&shell, &value, palette, 0))
        .unwrap_or_else(|| palette.foreground.clone());
    palette.popup_border = shell_text(&shell, "popups", "border")
        .and_then(|value| resolve_shell_color(&shell, &value, palette, 0))
        .unwrap_or_else(|| palette.accent.clone());
    palette.popup_background_alpha = shell_number(&shell, "popups", "background-alpha")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    palette.popup_border_alpha = shell_number(&shell, "popups", "border-alpha")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    palette.popup_border_width = shell_number(&shell, "popups", "border-width")
        .map(|value| value.round() as i32)
        .unwrap_or(2)
        .max(0);
    palette.popup_corner_radius = load_window_rounding().max(0);
}

fn load_palette_from_paths(paths: &[PathBuf]) -> Palette {
    let mut palette = Palette::default();
    let Some((theme, path)) = paths
        .iter()
        .find_map(|path| read_theme_file(path).map(|theme| (theme, path)))
    else {
        return palette;
    };

    if let Some(accent) = valid_hex(theme.accent).or_else(|| valid_hex(theme.color4)) {
        palette.accent = accent;
    }
    if let Some(foreground) = valid_hex(theme.foreground)
        .or_else(|| valid_hex(theme.fg))
        .or_else(|| valid_hex(theme.color7))
    {
        palette.foreground = foreground;
    }
    if let Some(background) = valid_hex(theme.background)
        .or_else(|| valid_hex(theme.bg))
        .or_else(|| valid_hex(theme.color0))
    {
        palette.background = background;
    }
    palette.popup_background = palette.background.clone();
    palette.popup_foreground = palette.foreground.clone();
    palette.popup_border = palette.accent.clone();
    if let Some(theme_dir) = path.parent() {
        apply_shell_popup_style(theme_dir, &mut palette);
    }

    palette
}

fn read_theme_file(path: &Path) -> Option<ThemeFile> {
    let text = fs::read_to_string(path).ok()?;
    toml::from_str::<ThemeFile>(&text).ok()
}

pub fn load_palette() -> Palette {
    load_palette_from_paths(&palette_paths())
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
  border: {panel_border_width}px solid {panel_border};
  border-radius: {panel_corner_radius}px;
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

.search-result-meta link,
.search-result-meta link:hover,
.search-result-meta link:visited {{
  color: {muted_foreground};
  text-decoration-line: none;
}}

separator {{
  color: {separator};
}}
"#,
        panel_background = rgba(&palette.popup_background, palette.popup_background_alpha,),
        panel_border = rgba(&palette.popup_border, palette.popup_border_alpha),
        panel_border_width = palette.popup_border_width,
        panel_corner_radius = palette.popup_corner_radius,
        shadow = rgba("#000000", 0.30),
        card_shadow = rgba("#000000", 0.18),
        accent = palette.accent,
        foreground = palette.popup_foreground,
        read_location = rgba(&palette.popup_foreground, 0.76),
        muted_foreground = rgba(&palette.popup_foreground, 0.72),
        time_chip_background = rgba("#000000", 0.10),
        time_chip_border = rgba(&palette.popup_foreground, 0.12),
        card_background = rgba("#ffffff", 0.045),
        card_border = rgba(&palette.popup_foreground, 0.08),
        accent_focus_border = rgba(&palette.accent, 0.75),
        accent_focus_shadow = rgba(&palette.accent, 0.14),
        button_background = rgba(&palette.background, 0.72),
        button_border = rgba(&palette.popup_foreground, 0.10),
        button_hover_background = rgba(&palette.background, 0.86),
        icon_button_border = rgba(&palette.popup_foreground, 0.06),
        icon_button_hover_background = rgba(&palette.popup_foreground, 0.06),
        icon_button_hover_border = rgba(&palette.popup_foreground, 0.16),
        icon_button_active_background = rgba(&palette.accent, 0.10),
        icon_button_active_border = rgba(&palette.accent, 0.30),
        search_result_hover_background = rgba(&palette.accent, 0.12),
        separator = rgba(&palette.popup_foreground, 0.09),
    )
}

#[cfg(test)]
mod tests {
    use super::{load_palette_from_paths, Palette};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn loads_omarchy_palette_from_first_available_path() {
        let temp_dir = TempDir::new().unwrap();
        let missing = temp_dir.path().join("missing/colors.toml");
        let current = temp_dir.path().join("state/colors.toml");
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        fs::write(
            &current,
            r##"accent = "#7c7ca8"
foreground = "#c8c8c8"
background = "#13131D"
"##,
        )
        .unwrap();

        let palette = load_palette_from_paths(&[missing, current]);

        assert_eq!(palette.accent, "#7c7ca8");
        assert_eq!(palette.foreground, "#c8c8c8");
        assert_eq!(palette.background, "#13131D");
    }

    #[test]
    fn supports_legacy_palette_keys_and_rejects_invalid_colors() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("colors.toml");
        fs::write(
            &path,
            r##"fg = "#a1a2a7"
bg = "#13131D"
accent = ""
color4 = "#7c7ca8"
"##,
        )
        .unwrap();

        let palette = load_palette_from_paths(&[path]);

        assert_eq!(palette.accent, "#7c7ca8");
        assert_eq!(palette.foreground, "#a1a2a7");
        assert_eq!(palette.background, "#13131D");
    }

    #[test]
    fn loads_omarchy_shell_popup_surface_tokens() {
        let temp_dir = TempDir::new().unwrap();
        let theme_dir = temp_dir.path().join("theme");
        let colors_path = theme_dir.join("colors.toml");
        fs::create_dir_all(&theme_dir).unwrap();
        fs::write(
            &colors_path,
            r##"accent = "#7c7ca8"
foreground = "#c8c8c8"
background = "#13131D"
"##,
        )
        .unwrap();
        fs::write(
            theme_dir.join("shell.toml"),
            r##"[hyprland]
active-border = "#b57b97 #13131D 45deg"

[popups]
background = "background"
background-alpha = 0.9
text = "foreground"
border = "hyprland.active-border"
border-alpha = 0.8
border-width = 3
"##,
        )
        .unwrap();

        let palette = load_palette_from_paths(&[colors_path]);

        assert_eq!(palette.popup_background, "#13131D");
        assert_eq!(palette.popup_foreground, "#c8c8c8");
        assert_eq!(palette.popup_border, "#b57b97");
        assert_eq!(palette.popup_background_alpha, 0.9);
        assert_eq!(palette.popup_border_alpha, 0.8);
        assert_eq!(palette.popup_border_width, 3);
    }

    #[test]
    fn missing_palette_uses_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let palette = load_palette_from_paths(&[temp_dir.path().join("missing")]);

        assert_eq!(palette.accent, Palette::default().accent);
        assert_eq!(palette.foreground, Palette::default().foreground);
        assert_eq!(palette.background, Palette::default().background);
    }
}
