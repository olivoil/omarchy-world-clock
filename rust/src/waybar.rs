use crate::config::{
    detect_local_timezone, effective_time_format, ordered_timezones, AppConfig, ConfigManager,
};
use crate::runtime::popup_running;
use crate::time::{format_display_time, zoned_datetime};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::Path;

pub const MODULE_ICON: &str = "";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModulePayload {
    pub text: String,
    pub class: String,
    pub tooltip: String,
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
    let entries = ordered_timezones(&config.timezones, &config.sort_mode, now);
    let time_format = effective_time_format(&config.time_format);
    let rows: Vec<(String, String)> = entries
        .iter()
        .map(|entry| {
            let mut label = entry.display_label();
            if entry.timezone == local_timezone {
                label = format!("{label}  ·  Local");
            }
            let time = format_display_time(&zoned_datetime(now, &entry.timezone), &time_format);
            (label, time)
        })
        .collect();

    let mut tooltip_lines = vec!["World Clock".to_string(), String::new()];
    if rows.is_empty() {
        tooltip_lines.push("No timezones yet.".to_string());
    } else {
        tooltip_lines.extend(format_tooltip_clock_rows(&rows));
    }

    ModulePayload {
        text: MODULE_ICON.to_string(),
        class: if popup_active { "active" } else { "inactive" }.to_string(),
        tooltip: tooltip_lines.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_tooltip_clock_rows, module_payload_from_config};
    use crate::config::{AppConfig, TimezoneEntry};
    use chrono::{TimeZone, Utc};

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
                    locked: false,
                },
                TimezoneEntry {
                    timezone: "Asia/Tokyo".to_string(),
                    label: "Tokyo".to_string(),
                    locked: false,
                },
            ],
            sort_mode: "manual".to_string(),
            time_format: "ampm".to_string(),
        };
        let now = Utc.with_ymd_and_hms(2026, 4, 16, 20, 26, 0).unwrap();

        let payload = module_payload_from_config(&config, now, "UTC", true);

        assert_eq!(payload.text, "");
        assert_eq!(payload.class, "active");
        assert!(payload.tooltip.contains("Home  ·  Local"));
        assert!(payload.tooltip.contains("8:26 PM"));
    }

    #[test]
    fn module_payload_shows_empty_state() {
        let config = AppConfig {
            timezones: Vec::new(),
            sort_mode: "manual".to_string(),
            time_format: "system".to_string(),
        };
        let now = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();

        let payload = module_payload_from_config(&config, now, "UTC", false);

        assert_eq!(payload.tooltip, "World Clock\n\nNo timezones yet.");
        assert_eq!(payload.class, "inactive");
    }
}
