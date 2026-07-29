use regex::Regex;
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const DEFAULT_WINDOW_GAP: i32 = 10;
const DEFAULT_BORDER_SIZE: i32 = 2;
const DEFAULT_WINDOW_ROUNDING: i32 = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonitorReservedSpace {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn hypr_int_regex(key: &str) -> Regex {
    Regex::new(&format!(r"^{}\s*=\s*(\d+)\b", regex::escape(key))).expect("valid regex")
}

fn parse_hypr_int(text: &str, key: &str) -> Option<i32> {
    let pattern = hypr_int_regex(key);
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(captures) = pattern.captures(line) {
            if let Some(value) = captures
                .get(1)
                .and_then(|match_| match_.as_str().parse::<i32>().ok())
            {
                return Some(value);
            }
        }
    }
    None
}

fn digits_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\d+").expect("valid regex"))
}

fn parse_hyprctl_custom_int(raw_value: &str) -> Option<i32> {
    digits_regex()
        .find(raw_value)
        .and_then(|match_| match_.as_str().parse::<i32>().ok())
}

fn load_hyprctl_option_int(option_name: &str) -> Option<i32> {
    let output = Command::new("hyprctl")
        .args(["-j", "getoption", option_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let payload: Value = serde_json::from_slice(&output.stdout).ok()?;
    if let Some(value) = payload.get("int").and_then(Value::as_i64) {
        return i32::try_from(value).ok();
    }

    payload
        .get("custom")
        .and_then(Value::as_str)
        .and_then(parse_hyprctl_custom_int)
}

fn parse_monitor_reserved_space(
    payload: &[u8],
    monitor_name: Option<&str>,
) -> Option<MonitorReservedSpace> {
    let monitors = serde_json::from_slice::<Value>(payload).ok()?;
    let monitors = monitors.as_array()?;
    let monitor = monitor_name
        .and_then(|name| {
            monitors
                .iter()
                .find(|monitor| monitor.get("name").and_then(Value::as_str) == Some(name))
        })
        .or_else(|| {
            monitors.iter().find(|monitor| {
                monitor
                    .get("focused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .or_else(|| monitors.first())?;
    let reserved = monitor.get("reserved")?.as_array()?;
    if reserved.len() != 4 {
        return None;
    }

    let value_at = |index: usize| {
        reserved[index]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .map(|value| value.max(0))
    };
    Some(MonitorReservedSpace {
        left: value_at(0)?,
        top: value_at(1)?,
        right: value_at(2)?,
        bottom: value_at(3)?,
    })
}

pub fn load_monitor_reserved_space(monitor_name: Option<&str>) -> MonitorReservedSpace {
    let Some(output) = Command::new("hyprctl")
        .args(["-j", "monitors"])
        .output()
        .ok()
        .filter(|output| output.status.success())
    else {
        return MonitorReservedSpace::default();
    };

    parse_monitor_reserved_space(&output.stdout, monitor_name).unwrap_or_default()
}

pub fn popup_surface_size(
    monitor_width: i32,
    monitor_height: i32,
    top_margin: i32,
    reserved: MonitorReservedSpace,
) -> (i32, i32) {
    (
        (monitor_width - reserved.left - reserved.right).max(200),
        (monitor_height - reserved.top - reserved.bottom - top_margin).max(200),
    )
}

fn hypr_look_and_feel_paths() -> Vec<PathBuf> {
    vec![
        home_dir().join(".config/hypr/looknfeel.conf"),
        home_dir().join(".local/share/omarchy/default/hypr/looknfeel.conf"),
    ]
}

pub fn load_window_gap() -> i32 {
    if let Some(value) = load_hyprctl_option_int("general:gaps_out") {
        return value;
    }

    for path in hypr_look_and_feel_paths() {
        if let Ok(text) = fs::read_to_string(path) {
            if let Some(value) = parse_hypr_int(&text, "gaps_out") {
                return value;
            }
        }
    }

    DEFAULT_WINDOW_GAP
}

pub fn load_window_border_size() -> i32 {
    if let Some(value) = load_hyprctl_option_int("general:border_size") {
        return value;
    }

    for path in hypr_look_and_feel_paths() {
        if let Ok(text) = fs::read_to_string(path) {
            if let Some(value) = parse_hypr_int(&text, "border_size") {
                return value;
            }
        }
    }

    DEFAULT_BORDER_SIZE
}

pub fn load_window_rounding() -> i32 {
    load_hyprctl_option_int("decoration:rounding").unwrap_or(DEFAULT_WINDOW_ROUNDING)
}

pub fn popup_top_margin(window_gap: i32) -> i32 {
    (window_gap.max(0) + 1) / 2
}

#[cfg(test)]
mod tests {
    use super::{
        parse_monitor_reserved_space, popup_surface_size, popup_top_margin, MonitorReservedSpace,
    };

    #[test]
    fn popup_starts_below_bar_with_native_half_gap() {
        assert_eq!(popup_top_margin(10), 5);
        assert_eq!(popup_top_margin(5), 3);
    }

    #[test]
    fn monitor_reserved_space_uses_the_target_output() {
        let payload = br#"[
            {"name":"DP-1","focused":false,"reserved":[8,0,4,12]},
            {"name":"eDP-1","focused":true,"reserved":[0,26,0,0]}
        ]"#;

        assert_eq!(
            parse_monitor_reserved_space(payload, Some("eDP-1")),
            Some(MonitorReservedSpace {
                left: 0,
                top: 26,
                right: 0,
                bottom: 0,
            })
        );
    }

    #[test]
    fn popup_surface_matches_the_compositor_work_area() {
        let size = popup_surface_size(
            1920,
            1200,
            5,
            MonitorReservedSpace {
                left: 0,
                top: 26,
                right: 0,
                bottom: 0,
            },
        );

        assert_eq!(size, (1920, 1169));
    }
}
