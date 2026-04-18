use regex::Regex;
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const DEFAULT_WINDOW_GAP: i32 = 10;
const DEFAULT_BORDER_SIZE: i32 = 2;
pub const POPUP_TOP_CONTENT_MARGIN: i32 = 8;

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

pub fn popup_top_margin(window_gap: i32, border_size: i32, content_margin_top: i32) -> i32 {
    (window_gap + border_size - content_margin_top).max(0)
}
