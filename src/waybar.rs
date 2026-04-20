use crate::config::{detect_local_timezone, system_time_format, AppConfig, ConfigManager};
use crate::runtime::popup_running;
use crate::time::{format_display_time, zoned_datetime};
use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;

pub const MODULE_ICON: &str = "";
pub const MODULE_MARKER_START: &str = "  // omarchy-world-clock:start";
pub const MODULE_MARKER_END: &str = "  // omarchy-world-clock:end";
pub const LEGACY_MODULE_MARKER_START: &str = "  // omarchy-world-clock-rs:start";
pub const LEGACY_MODULE_MARKER_END: &str = "  // omarchy-world-clock-rs:end";
pub const STYLE_MARKER_START: &str = "/* omarchy-world-clock:start */";
pub const STYLE_MARKER_END: &str = "/* omarchy-world-clock:end */";
pub const LEGACY_STYLE_MARKER_START: &str = "/* omarchy-world-clock-rs:start */";
pub const LEGACY_STYLE_MARKER_END: &str = "/* omarchy-world-clock-rs:end */";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModulePayload {
    pub text: String,
    pub class: String,
    pub tooltip: String,
}

fn modules_center_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"(?s)("modules-center"\s*:\s*\[)(.*?)(\])"#)
            .expect("modules-center regex should compile")
    })
}

fn module_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(&format!(
            r#"(?s){}.*?{}"#,
            regex::escape(MODULE_MARKER_START),
            regex::escape(MODULE_MARKER_END)
        ))
        .expect("module marker regex should compile")
    })
}

fn module_marker_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(&format!(
            r#"(?s)\n?{}.*?{}\n?"#,
            regex::escape(MODULE_MARKER_START),
            regex::escape(MODULE_MARKER_END)
        ))
        .expect("module marker block regex should compile")
    })
}

fn legacy_module_marker_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(&format!(
            r#"(?s)\n?{}.*?{}\n?"#,
            regex::escape(LEGACY_MODULE_MARKER_START),
            regex::escape(LEGACY_MODULE_MARKER_END)
        ))
        .expect("legacy module marker block regex should compile")
    })
}

fn style_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(&format!(
            r#"(?s){}.*?{}"#,
            regex::escape(STYLE_MARKER_START),
            regex::escape(STYLE_MARKER_END)
        ))
        .expect("style marker regex should compile")
    })
}

fn style_marker_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(&format!(
            r#"(?s)\n?{}.*?{}\n?"#,
            regex::escape(STYLE_MARKER_START),
            regex::escape(STYLE_MARKER_END)
        ))
        .expect("style marker block regex should compile")
    })
}

fn legacy_style_marker_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(&format!(
            r#"(?s)\n?{}.*?{}\n?"#,
            regex::escape(LEGACY_STYLE_MARKER_START),
            regex::escape(LEGACY_STYLE_MARKER_END)
        ))
        .expect("legacy style marker block regex should compile")
    })
}

fn quoted_token_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#""[^"]+""#).expect("token regex should compile"))
}

pub fn module_block(command_path: &str) -> String {
    [
        MODULE_MARKER_START.to_string(),
        "  \"custom/world-clock\": {".to_string(),
        format!("    \"exec\": \"{command_path} module\","),
        "    \"return-type\": \"json\",".to_string(),
        "    \"interval\": 2,".to_string(),
        "    \"format\": \"{}\",".to_string(),
        "    \"tooltip\": true,".to_string(),
        format!("    \"on-click\": \"{command_path} toggle\","),
        "    \"on-click-right\": \"omarchy-launch-floating-terminal-with-presentation omarchy-tz-select\""
            .to_string(),
        "  },".to_string(),
        MODULE_MARKER_END.to_string(),
    ]
    .join("\n")
}

pub fn style_block() -> String {
    [
        STYLE_MARKER_START.to_string(),
        "#custom-world-clock {".to_string(),
        "  min-width: 12px;".to_string(),
        "  margin-left: 6px;".to_string(),
        "  margin-right: 0;".to_string(),
        "  font-size: 12px;".to_string(),
        "  opacity: 0.72;".to_string(),
        "}".to_string(),
        String::new(),
        "#custom-world-clock.active {".to_string(),
        "  opacity: 1;".to_string(),
        "}".to_string(),
        STYLE_MARKER_END.to_string(),
    ]
    .join("\n")
}

fn patch_modules_center(text: &str, include_module: bool) -> Result<String> {
    let Some(captures) = modules_center_regex().captures(text) else {
        bail!("Could not find modules-center in Waybar config.");
    };
    let Some(match_all) = captures.get(0) else {
        bail!("Could not parse modules-center in Waybar config.");
    };
    let prefix = captures
        .get(1)
        .map(|capture| capture.as_str())
        .ok_or_else(|| anyhow!("missing modules-center prefix"))?;
    let content = captures
        .get(2)
        .map(|capture| capture.as_str())
        .ok_or_else(|| anyhow!("missing modules-center contents"))?;
    let suffix = captures
        .get(3)
        .map(|capture| capture.as_str())
        .ok_or_else(|| anyhow!("missing modules-center suffix"))?;

    let mut tokens = quoted_token_regex()
        .find_iter(content)
        .map(|token| token.as_str().to_string())
        .collect::<Vec<_>>();
    let target = "\"custom/world-clock\"";
    let legacy_target = "\"custom/world-clock-rs\"";

    tokens.retain(|token| token != legacy_target);

    if include_module && !tokens.iter().any(|token| token == target) {
        if let Some(index) = tokens.iter().position(|token| token == "\"clock\"") {
            tokens.insert(index + 1, target.to_string());
        } else {
            tokens.push(target.to_string());
        }
    }
    if !include_module {
        tokens.retain(|token| token != target);
    }

    let multiline = content.contains('\n');
    let rebuilt_content = if multiline {
        let item_indent = content
            .lines()
            .find_map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('"') {
                    Some(line[..line.len() - trimmed.len()].to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "    ".to_string());
        let closing_indent = match_all
            .as_str()
            .lines()
            .last()
            .map(|line| line.trim_end_matches(']'))
            .map(str::trim_end)
            .and_then(|line| {
                let trimmed = line.trim_start();
                if trimmed.len() == line.len() {
                    None
                } else {
                    Some(line[..line.len() - trimmed.len()].to_string())
                }
            })
            .unwrap_or_else(|| "  ".to_string());
        let mut rebuilt = String::from("\n");
        rebuilt.push_str(
            &tokens
                .iter()
                .enumerate()
                .map(|(index, token)| {
                    format!(
                        "{item_indent}{token}{}",
                        if index + 1 < tokens.len() { "," } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        rebuilt.push('\n');
        rebuilt.push_str(&closing_indent);
        rebuilt
    } else {
        tokens.join(", ")
    };

    Ok(format!(
        "{}{}{}{}{}",
        &text[..match_all.start()],
        prefix,
        rebuilt_content,
        suffix,
        &text[match_all.end()..]
    ))
}

fn cleanup_config_after_block_removal(text: String) -> String {
    let text = Regex::new(r#"(?m)^\s*,\s*$\n?"#)
        .expect("standalone comma cleanup regex should compile")
        .replace_all(&text, "")
        .into_owned();
    let text = Regex::new(r#",\s*\n\s*\n}"#)
        .expect("double blank cleanup regex should compile")
        .replace(&text, "\n}")
        .into_owned();
    Regex::new(r#",\s*\n}"#)
        .expect("trailing comma cleanup regex should compile")
        .replace(&text, "\n}")
        .into_owned()
}

pub fn patch_config_text(text: &str, command_path: &str) -> Result<String> {
    let text = patch_modules_center(text, true)?;
    let text = cleanup_config_after_block_removal(
        legacy_module_marker_block_regex()
            .replace(&text, "\n")
            .into_owned(),
    );
    let block = module_block(command_path);
    if module_marker_regex().is_match(&text) {
        return Ok(module_marker_regex()
            .replace(&text, block.as_str())
            .into_owned());
    }

    let insert_pattern = Regex::new(r#"\n}\s*$"#).expect("insert pattern should compile");
    if insert_pattern.is_match(&text) {
        return Ok(insert_pattern
            .replace(&text, format!(",\n{block}\n}}\n").as_str())
            .into_owned());
    }

    bail!("Could not append module block to Waybar config.")
}

pub fn unpatch_config_text(text: &str) -> Result<String> {
    let text = patch_modules_center(text, false)?;
    let text = module_marker_block_regex()
        .replace(&text, "\n")
        .into_owned();
    let text = legacy_module_marker_block_regex()
        .replace(&text, "\n")
        .into_owned();
    Ok(cleanup_config_after_block_removal(text))
}

pub fn patch_style_text(text: &str) -> String {
    let text = legacy_style_marker_block_regex()
        .replace(text, "\n")
        .into_owned();
    let block = style_block();
    if style_marker_regex().is_match(&text) {
        let replaced = style_marker_regex()
            .replace(&text, block.as_str())
            .into_owned();
        return format!("{}\n", replaced.trim_end());
    }

    let mut rendered = text;
    if !rendered.is_empty() && !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    rendered.push('\n');
    rendered.push_str(&block);
    rendered.push('\n');
    rendered
}

pub fn unpatch_style_text(text: &str) -> String {
    let stripped = style_marker_block_regex().replace(text, "\n").into_owned();
    let stripped = legacy_style_marker_block_regex()
        .replace(&stripped, "\n")
        .into_owned();
    format!("{}\n", stripped.trim_end())
}

fn pad_right(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.chars().count());
    format!("{value}{}", " ".repeat(padding))
}

fn pad_left(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.chars().count());
    format!("{}{value}", " ".repeat(padding))
}

pub fn format_tooltip_clock_rows(rows: &[(String, String)]) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }

    let widest_label = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let widest_time = rows
        .iter()
        .map(|(_, time)| time.chars().count())
        .max()
        .unwrap_or(0);

    rows.iter()
        .map(|(label, time)| {
            format!(
                "{}  {}",
                pad_right(label, widest_label),
                pad_left(time, widest_time)
            )
        })
        .collect()
}

pub fn module_payload(pid_path: &Path) -> anyhow::Result<ModulePayload> {
    let config = ConfigManager::new(None).load()?;
    let now = Utc::now();
    let local_timezone = detect_local_timezone();
    Ok(module_payload_from_config(
        &config,
        now,
        &local_timezone,
        popup_running(pid_path),
    ))
}

pub fn module_payload_from_config(
    config: &AppConfig,
    now: DateTime<Utc>,
    local_timezone: &str,
    popup_active: bool,
) -> ModulePayload {
    let time_format = system_time_format();
    module_payload_from_config_with_time_format(
        config,
        now,
        local_timezone,
        popup_active,
        &time_format,
    )
}

fn module_payload_from_config_with_time_format(
    config: &AppConfig,
    now: DateTime<Utc>,
    local_timezone: &str,
    popup_active: bool,
    time_format: &str,
) -> ModulePayload {
    let anchor = zoned_datetime(now, local_timezone);
    let mut entries = config
        .timezones
        .iter()
        .filter(|entry| entry.timezone != local_timezone)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_zoned = zoned_datetime(now, &left.timezone);
        let right_zoned = zoned_datetime(now, &right.timezone);
        left_zoned
            .naive_local()
            .signed_duration_since(anchor.naive_local())
            .num_minutes()
            .cmp(
                &right_zoned
                    .naive_local()
                    .signed_duration_since(anchor.naive_local())
                    .num_minutes(),
            )
            .then_with(|| left.display_label().cmp(&right.display_label()))
    });

    let rows: Vec<(String, String)> = entries
        .into_iter()
        .map(|entry| {
            let label = entry.read_card_title();
            let time = format_display_time(&zoned_datetime(now, &entry.timezone), &time_format);
            (label, time)
        })
        .collect();

    ModulePayload {
        text: MODULE_ICON.to_string(),
        class: if popup_active { "active" } else { "inactive" }.to_string(),
        tooltip: if rows.is_empty() {
            "No additional timezones yet.".to_string()
        } else {
            format_tooltip_clock_rows(&rows).join("\n")
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_tooltip_clock_rows, module_payload_from_config,
        module_payload_from_config_with_time_format, patch_config_text, patch_style_text,
        unpatch_config_text, unpatch_style_text,
    };
    use crate::config::{AppConfig, TimezoneEntry};
    use chrono::{TimeZone, Utc};

    const WAYBAR_CONFIG: &str = r#"{
  "modules-center": ["clock", "custom/update"],
  "clock": {
    "format": "{:L%A %H:%M}"
  },
  "tray": {
    "icon-size": 12
  }
}
"#;

    const WAYBAR_STYLE: &str = r#"#clock {
  margin-left: 5px;
}
"#;

    const LEGACY_WAYBAR_CONFIG: &str = r#"{
  "modules-center": ["clock", "custom/world-clock-rs", "custom/update"],
  "clock": {
    "format": "{:L%A %H:%M}"
  },
  // omarchy-world-clock-rs:start
  "custom/world-clock-rs": {
    "exec": "~/.local/bin/omarchy-world-clock-rs module"
  },
  // omarchy-world-clock-rs:end,
}
"#;

    const LEGACY_WAYBAR_STYLE: &str = r#"#clock {
  margin-left: 5px;
}

/* omarchy-world-clock-rs:start */
#custom-world-clock-rs {
  opacity: 0.72;
}
/* omarchy-world-clock-rs:end */
"#;

    #[test]
    fn aligns_tooltip_rows_to_widest_label() {
        let rows = vec![
            ("Local  Cancun".to_string(), "22:03".to_string()),
            ("Vancouver".to_string(), "20:03".to_string()),
            ("Paris".to_string(), "05:03".to_string()),
            ("Los Angeles".to_string(), "20:03".to_string()),
        ];

        assert_eq!(
            format_tooltip_clock_rows(&rows),
            vec![
                "Local  Cancun  22:03".to_string(),
                "Vancouver      20:03".to_string(),
                "Paris          05:03".to_string(),
                "Los Angeles    20:03".to_string(),
            ]
        );
    }

    #[test]
    fn module_payload_marks_local_timezone_and_uses_ampm() {
        let config = AppConfig {
            timezones: vec![
                TimezoneEntry {
                    timezone: "UTC".to_string(),
                    label: "Home".to_string(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    timezone: "Asia/Tokyo".to_string(),
                    label: "Tokyo".to_string(),
                    latitude: None,
                    longitude: None,
                },
            ],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 4, 16, 20, 26, 0).unwrap();

        let payload =
            module_payload_from_config_with_time_format(&config, now, "UTC", true, "ampm");

        assert_eq!(payload.text, "");
        assert_eq!(payload.class, "active");
        assert!(!payload.tooltip.contains("World Clock"));
        assert!(!payload.tooltip.contains("Home"));
        assert!(payload.tooltip.contains("Tokyo"));
        assert!(payload.tooltip.contains("5:26 AM"));
    }

    #[test]
    fn module_payload_sorts_tooltip_rows_by_popup_time_order() {
        let config = AppConfig {
            timezones: vec![
                TimezoneEntry {
                    timezone: "Europe/Paris".to_string(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    timezone: "America/Cancun".to_string(),
                    label: "Home".to_string(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    timezone: "Asia/Kolkata".to_string(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    timezone: "America/Chicago".to_string(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    timezone: "America/Los_Angeles".to_string(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                },
            ],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 5, 5, 0).unwrap();

        let payload = module_payload_from_config_with_time_format(
            &config,
            now,
            "America/Cancun",
            false,
            "24h",
        );
        let lines = payload.tooltip.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("Los Angeles"));
        assert!(lines[1].contains("Chicago"));
        assert!(lines[2].contains("Paris"));
        assert!(lines[3].contains("Kolkata"));
        assert!(!payload.tooltip.contains("Home"));
    }

    #[test]
    fn module_payload_uses_read_card_titles_in_tooltip() {
        let config = AppConfig {
            timezones: vec![
                TimezoneEntry {
                    timezone: "America/Cancun".to_string(),
                    label: "Home".to_string(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    timezone: "Europe/Paris".to_string(),
                    label: "Rennes, Brittany, France".to_string(),
                    latitude: None,
                    longitude: None,
                },
            ],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 5, 5, 0).unwrap();

        let payload = module_payload_from_config_with_time_format(
            &config,
            now,
            "America/Cancun",
            false,
            "24h",
        );

        assert_eq!(payload.tooltip, "Rennes  07:05");
        assert!(!payload.tooltip.contains("Brittany"));
    }

    #[test]
    fn module_payload_shows_empty_state() {
        let config = AppConfig {
            timezones: Vec::new(),
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();

        let payload = module_payload_from_config(&config, now, "UTC", false);

        assert_eq!(payload.tooltip, "No additional timezones yet.");
        assert_eq!(payload.class, "inactive");
    }

    #[test]
    fn patch_config_inserts_module_once() {
        let patched = patch_config_text(WAYBAR_CONFIG, "~/.local/bin/omarchy-world-clock").unwrap();
        assert!(patched.contains(
            "\"modules-center\": [\"clock\", \"custom/world-clock\", \"custom/update\"]"
        ));
        assert!(patched.contains("\"custom/world-clock\": {"));

        let patched_twice =
            patch_config_text(&patched, "~/.local/bin/omarchy-world-clock").unwrap();
        assert_eq!(patched, patched_twice);
    }

    #[test]
    fn patch_config_removes_legacy_preview_module() {
        let patched =
            patch_config_text(LEGACY_WAYBAR_CONFIG, "~/.local/bin/omarchy-world-clock").unwrap();
        assert!(!patched.contains("\"custom/world-clock-rs\""));
        assert!(!patched.contains("omarchy-world-clock-rs:start"));
        assert!(!patched.contains("\n,\n"));
        assert!(patched.contains("\"custom/world-clock\": {"));
    }

    #[test]
    fn unpatch_config_removes_module() {
        let patched = patch_config_text(WAYBAR_CONFIG, "~/.local/bin/omarchy-world-clock").unwrap();
        let unpatched = unpatch_config_text(&patched).unwrap();
        assert!(!unpatched.contains("\"custom/world-clock\""));
        assert!(unpatched.contains("\"modules-center\": [\"clock\", \"custom/update\"]"));
    }

    #[test]
    fn patch_style_is_idempotent() {
        let patched = patch_style_text(WAYBAR_STYLE);
        assert!(patched.contains("#custom-world-clock"));
        assert!(!patched.contains("tooltip {"));
        assert!(!patched.contains("tooltip label"));
        assert_eq!(patched, patch_style_text(&patched));

        let unpatched = unpatch_style_text(&patched);
        assert!(!unpatched.contains("#custom-world-clock"));
    }

    #[test]
    fn patch_style_removes_legacy_preview_style() {
        let patched = patch_style_text(LEGACY_WAYBAR_STYLE);
        assert!(!patched.contains("#custom-world-clock-rs"));
        assert!(patched.contains("#custom-world-clock"));
    }
}
