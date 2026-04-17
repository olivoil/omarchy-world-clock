use crate::time::{friendly_timezone_name, zoned_datetime};
use anyhow::Context;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::OnceLock;

pub const CONFIG_VERSION: u64 = 3;
pub const LOCAL_TIMEZONE_MIGRATION_VERSION: u64 = 2;
pub const DEFAULT_SORT_MODE: &str = "manual";
pub const DEFAULT_TIME_FORMAT: &str = "system";

const VALID_SORT_MODES: [&str; 3] = ["manual", "alpha", "time"];
const VALID_TIME_FORMATS: [&str; 3] = ["system", "24h", "ampm"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub timezones: Vec<TimezoneEntry>,
    pub sort_mode: String,
    pub time_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimezoneEntry {
    pub timezone: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub locked: bool,
}

impl TimezoneEntry {
    pub fn display_label(&self) -> String {
        let trimmed = self.label.trim();
        if trimmed.is_empty() {
            return friendly_timezone_name(&self.timezone);
        }
        trimmed.to_string()
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawTimezoneEntry {
    Legacy(String),
    Structured {
        timezone: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        locked: bool,
    },
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    version: Option<u64>,
    timezones: Option<Vec<RawTimezoneEntry>>,
    sort_mode: Option<String>,
    time_format: Option<String>,
}

#[derive(Debug, Serialize)]
struct StoredConfig<'a> {
    version: u64,
    timezones: &'a [TimezoneEntry],
    sort_mode: &'a str,
    time_format: &'a str,
}

#[derive(Debug, Clone)]
pub struct ConfigManager {
    path: PathBuf,
}

impl ConfigManager {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path: path.unwrap_or_else(default_config_path),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        self.load_with_local_timezone(&local_timezone)
    }

    fn load_with_local_timezone(&self, local_timezone: &str) -> anyhow::Result<AppConfig> {
        if !self.path.exists() {
            let config = self.default_config(local_timezone);
            self.save(&config)?;
            return Ok(config);
        }

        let config = match fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str::<RawConfig>(&text) {
                Ok(raw) => self.config_from_raw(raw, local_timezone),
                Err(_) => self.default_config(local_timezone),
            },
            Err(_) => self.default_config(local_timezone),
        };

        self.save(&config)?;
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> anyhow::Result<()> {
        let normalized = self.normalize_config(config.clone());
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let payload = StoredConfig {
            version: CONFIG_VERSION,
            timezones: &normalized.timezones,
            sort_mode: &normalized.sort_mode,
            time_format: &normalized.time_format,
        };
        let text = serde_json::to_string_pretty(&payload)?;
        fs::write(&self.path, format!("{text}\n"))
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }

    pub fn add_timezone(&self, timezone_name: &str, label: &str) -> anyhow::Result<AppConfig> {
        let mut config = self.load()?;
        let timezone_name = canonical_timezone_name(timezone_name);
        if timezone_name.is_empty() || !is_valid_timezone(&timezone_name) {
            return Ok(config);
        }

        if !config
            .timezones
            .iter()
            .any(|entry| entry.timezone == timezone_name)
        {
            config.timezones.push(TimezoneEntry {
                timezone: timezone_name,
                label: label.trim().to_string(),
                locked: false,
            });
            config = self.normalize_config(config);
            self.save(&config)?;
        }
        Ok(config)
    }

    pub fn remove_timezone(&self, timezone_name: &str) -> anyhow::Result<AppConfig> {
        let mut config = self.load()?;
        let timezone_name = canonical_timezone_name(timezone_name);
        config
            .timezones
            .retain(|entry| entry.timezone != timezone_name);
        config = self.normalize_config(config);
        self.save(&config)?;
        Ok(config)
    }

    pub fn set_sort_mode(&self, sort_mode: &str) -> anyhow::Result<AppConfig> {
        let mut config = self.load()?;
        config.sort_mode = if valid_sort_mode(sort_mode) {
            sort_mode.to_string()
        } else {
            DEFAULT_SORT_MODE.to_string()
        };
        config = self.normalize_config(config);
        self.save(&config)?;
        Ok(config)
    }

    pub fn set_timezone_locked(
        &self,
        timezone_name: &str,
        locked: bool,
    ) -> anyhow::Result<AppConfig> {
        let mut config = self.load()?;
        let timezone_name = canonical_timezone_name(timezone_name);
        if let Some(entry) = config
            .timezones
            .iter_mut()
            .find(|entry| entry.timezone == timezone_name)
        {
            entry.locked = locked;
        }
        config = self.normalize_config(config);
        self.save(&config)?;
        Ok(config)
    }

    pub fn set_time_format(&self, time_format: &str) -> anyhow::Result<AppConfig> {
        let mut config = self.load()?;
        config.time_format = if valid_time_format(time_format) {
            time_format.to_string()
        } else {
            DEFAULT_TIME_FORMAT.to_string()
        };
        config = self.normalize_config(config);
        self.save(&config)?;
        Ok(config)
    }

    fn config_from_raw(&self, raw: RawConfig, local_timezone: &str) -> AppConfig {
        let config_version = raw.version.unwrap_or(1);
        let mut seen = HashSet::new();
        let mut entries = Vec::new();

        for raw_entry in raw.timezones.unwrap_or_default() {
            let Some(entry) = self.parse_entry(raw_entry) else {
                continue;
            };
            if seen.insert(entry.timezone.clone()) {
                entries.push(entry);
            }
        }

        if config_version < LOCAL_TIMEZONE_MIGRATION_VERSION {
            let local_timezone = canonical_timezone_name(local_timezone);
            if !local_timezone.is_empty()
                && is_valid_timezone(&local_timezone)
                && seen.insert(local_timezone.clone())
            {
                entries.insert(
                    0,
                    TimezoneEntry {
                        timezone: local_timezone,
                        label: String::new(),
                        locked: false,
                    },
                );
            }
        }

        let sort_mode = raw
            .sort_mode
            .filter(|value| valid_sort_mode(value))
            .unwrap_or_else(|| DEFAULT_SORT_MODE.to_string());
        let time_format = raw
            .time_format
            .filter(|value| valid_time_format(value))
            .unwrap_or_else(|| DEFAULT_TIME_FORMAT.to_string());

        self.normalize_config(AppConfig {
            timezones: entries,
            sort_mode,
            time_format,
        })
    }

    fn parse_entry(&self, raw_entry: RawTimezoneEntry) -> Option<TimezoneEntry> {
        let (timezone, label, locked) = match raw_entry {
            RawTimezoneEntry::Legacy(timezone) => (timezone, String::new(), false),
            RawTimezoneEntry::Structured {
                timezone,
                label,
                locked,
            } => (timezone, label, locked),
        };

        let timezone = canonical_timezone_name(&timezone);
        if timezone.is_empty() || !is_valid_timezone(&timezone) {
            return None;
        }

        Some(TimezoneEntry {
            timezone,
            label: label.trim().to_string(),
            locked,
        })
    }

    fn normalize_config(&self, config: AppConfig) -> AppConfig {
        let mut seen = HashSet::new();
        let mut locked = Vec::new();
        let mut unlocked = Vec::new();

        for entry in config.timezones {
            let timezone = canonical_timezone_name(&entry.timezone);
            if timezone.is_empty()
                || !is_valid_timezone(&timezone)
                || !seen.insert(timezone.clone())
            {
                continue;
            }

            let normalized = TimezoneEntry {
                timezone,
                label: entry.label.trim().to_string(),
                locked: entry.locked,
            };

            if normalized.locked {
                locked.push(normalized);
            } else {
                unlocked.push(normalized);
            }
        }

        AppConfig {
            timezones: locked.into_iter().chain(unlocked).collect(),
            sort_mode: if valid_sort_mode(&config.sort_mode) {
                config.sort_mode
            } else {
                DEFAULT_SORT_MODE.to_string()
            },
            time_format: if valid_time_format(&config.time_format) {
                config.time_format
            } else {
                DEFAULT_TIME_FORMAT.to_string()
            },
        }
    }

    fn default_config(&self, local_timezone: &str) -> AppConfig {
        let local_timezone = canonical_timezone_name(local_timezone);
        let timezones = if local_timezone.is_empty() || !is_valid_timezone(&local_timezone) {
            Vec::new()
        } else {
            vec![TimezoneEntry {
                timezone: local_timezone,
                label: String::new(),
                locked: false,
            }]
        };

        AppConfig {
            timezones,
            sort_mode: DEFAULT_SORT_MODE.to_string(),
            time_format: DEFAULT_TIME_FORMAT.to_string(),
        }
    }
}

fn valid_sort_mode(value: &str) -> bool {
    VALID_SORT_MODES.contains(&value)
}

fn valid_time_format(value: &str) -> bool {
    VALID_TIME_FORMATS.contains(&value)
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn default_config_path() -> PathBuf {
    if let Some(path) = env::var_os("OMARCHY_WORLD_CLOCK_CONFIG") {
        return PathBuf::from(path);
    }
    home_dir().join(".config/omarchy-world-clock/config.json")
}

fn zoneinfo_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = env::var_os("TZDIR") {
        roots.push(PathBuf::from(path));
    }
    roots.push(PathBuf::from("/usr/share/zoneinfo"));
    roots.push(PathBuf::from("/usr/share/lib/zoneinfo"));
    roots
}

fn timezone_link_aliases() -> &'static HashMap<String, String> {
    static ALIASES: OnceLock<HashMap<String, String>> = OnceLock::new();
    ALIASES.get_or_init(load_timezone_link_aliases)
}

fn load_timezone_link_aliases() -> HashMap<String, String> {
    let mut links = HashMap::new();
    for base in zoneinfo_roots() {
        let tzdata = base.join("tzdata.zi");
        let Ok(text) = fs::read_to_string(tzdata) else {
            continue;
        };

        for raw_line in text.lines() {
            if !raw_line.starts_with("L ") {
                continue;
            }
            let mut parts = raw_line.split_whitespace();
            let _ = parts.next();
            let Some(target) = parts.next() else {
                continue;
            };
            let Some(alias) = parts.next() else {
                continue;
            };
            if alias.contains('/') {
                links.insert(alias.to_string(), target.to_string());
            }
        }
    }

    let aliases: Vec<String> = links.keys().cloned().collect();
    let mut resolved = HashMap::new();
    for alias in aliases {
        let mut current = alias.clone();
        let mut seen = HashSet::new();
        while let Some(next) = links.get(&current) {
            if !seen.insert(current.clone()) {
                break;
            }
            current = next.clone();
        }
        resolved.insert(alias, current);
    }

    resolved
}

fn canonicalize_from_zoneinfo(candidate: &str) -> Option<String> {
    for base in zoneinfo_roots() {
        let path = base.join(candidate);
        if !path.exists() {
            continue;
        }

        let Ok(real_path) = fs::canonicalize(path) else {
            continue;
        };
        let Ok(relative) = real_path.strip_prefix(&base) else {
            continue;
        };
        let rendered = relative.to_string_lossy().replace('\\', "/");
        if is_valid_timezone_name(&rendered) {
            return Some(rendered);
        }
    }

    None
}

fn is_valid_timezone_name(value: &str) -> bool {
    Tz::from_str(value).is_ok()
}

pub fn canonical_timezone_name(timezone_name: &str) -> String {
    let candidate = timezone_name.trim();
    if candidate.is_empty() {
        return String::new();
    }

    if let Some(alias) = timezone_link_aliases().get(candidate) {
        return alias.clone();
    }

    if let Some(canonical) = canonicalize_from_zoneinfo(candidate) {
        return canonical;
    }

    candidate.to_string()
}

pub fn is_valid_timezone(timezone_name: &str) -> bool {
    is_valid_timezone_name(&canonical_timezone_name(timezone_name))
}

pub fn detect_local_timezone() -> String {
    if let Ok(output) = Command::new("timedatectl")
        .args(["show", "--property=Timezone", "--value"])
        .output()
    {
        if output.status.success() {
            let timezone = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let canonical = canonical_timezone_name(&timezone);
            if !canonical.is_empty() && is_valid_timezone(&canonical) {
                return canonical;
            }
        }
    }

    if let Ok(timezone) = iana_time_zone::get_timezone() {
        let canonical = canonical_timezone_name(&timezone);
        if !canonical.is_empty() && is_valid_timezone(&canonical) {
            return canonical;
        }
    }

    "UTC".to_string()
}

pub fn ordered_timezones(
    entries: &[TimezoneEntry],
    sort_mode: &str,
    reference_utc: DateTime<Utc>,
) -> Vec<TimezoneEntry> {
    let mut locked = Vec::new();
    let mut unlocked = Vec::new();

    for entry in entries {
        if entry.locked {
            locked.push(entry.clone());
        } else {
            unlocked.push(entry.clone());
        }
    }

    match sort_mode {
        "alpha" => {
            unlocked.sort_by(|left, right| {
                left.display_label()
                    .to_lowercase()
                    .cmp(&right.display_label().to_lowercase())
                    .then_with(|| {
                        left.timezone
                            .to_lowercase()
                            .cmp(&right.timezone.to_lowercase())
                    })
            });
        }
        "time" => {
            unlocked.sort_by(|left, right| {
                zoned_datetime(reference_utc, &left.timezone)
                    .naive_local()
                    .cmp(&zoned_datetime(reference_utc, &right.timezone).naive_local())
                    .then_with(|| {
                        left.display_label()
                            .to_lowercase()
                            .cmp(&right.display_label().to_lowercase())
                    })
                    .then_with(|| {
                        left.timezone
                            .to_lowercase()
                            .cmp(&right.timezone.to_lowercase())
                    })
            });
        }
        _ => {}
    }

    locked.extend(unlocked);
    locked
}

pub fn waybar_clock_config_paths() -> Vec<PathBuf> {
    vec![
        home_dir().join(".config/waybar/config.jsonc"),
        home_dir().join(".config/waybar/config"),
        home_dir().join(".local/share/omarchy/config/waybar/config.jsonc"),
    ]
}

fn waybar_clock_format_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#""clock"\s*:\s*\{[\s\S]*?"format"\s*:\s*"(?P<format>(?:\\.|[^"\\])*)""#)
            .expect("valid regex")
    })
}

pub fn load_waybar_clock_format(paths: Option<&[PathBuf]>) -> Option<String> {
    let candidates = paths
        .map(|paths| paths.to_vec())
        .unwrap_or_else(waybar_clock_config_paths);
    let pattern = waybar_clock_format_regex();

    for path in candidates {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let Some(captures) = pattern.captures(&contents) else {
            continue;
        };
        let raw_format = captures.name("format")?.as_str();
        let wrapped = format!("\"{raw_format}\"");
        if let Ok(decoded) = serde_json::from_str::<String>(&wrapped) {
            return Some(decoded);
        }
        return Some(raw_format.to_string());
    }

    None
}

fn infer_time_format_inner(clock_format: &str) -> Option<&'static str> {
    if ["%I", "%l", "%p", "%P", "%r"]
        .iter()
        .any(|token| clock_format.contains(token))
    {
        return Some("ampm");
    }
    if ["%H", "%k", "%R", "%T"]
        .iter()
        .any(|token| clock_format.contains(token))
    {
        return Some("24h");
    }
    None
}

fn locale_format(item: libc::nl_item) -> Option<String> {
    let value = unsafe { libc::nl_langinfo(item) };
    if value.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .ok()
        .map(ToOwned::to_owned)
}

pub fn detect_system_time_format_with_paths(paths: Option<&[PathBuf]>) -> String {
    if let Some(clock_format) = load_waybar_clock_format(paths) {
        if let Some(inferred) = infer_time_format_inner(&clock_format) {
            return inferred.to_string();
        }
    }

    if let Some(locale_time_format) = locale_format(libc::T_FMT) {
        if let Some(inferred) = infer_time_format_inner(&locale_time_format) {
            return inferred.to_string();
        }
    }

    if locale_format(libc::T_FMT_AMPM).is_some_and(|format| !format.is_empty()) {
        return "ampm".to_string();
    }

    "24h".to_string()
}

pub fn effective_time_format(time_format: &str) -> String {
    match time_format {
        "ampm" => "ampm".to_string(),
        "24h" => "24h".to_string(),
        _ => detect_system_time_format_with_paths(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_timezone_name, detect_system_time_format_with_paths, ordered_timezones,
        AppConfig, ConfigManager, TimezoneEntry,
    };
    use chrono::{TimeZone, Utc};
    use std::fs;
    use tempfile::TempDir;

    fn manager_in(temp_dir: &TempDir) -> ConfigManager {
        ConfigManager::new(Some(temp_dir.path().join("config.json")))
    }

    #[test]
    fn config_round_trips_and_inserts_local_timezone() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        let loaded = manager.load_with_local_timezone("UTC").unwrap();

        assert_eq!(
            loaded,
            AppConfig {
                timezones: vec![TimezoneEntry {
                    timezone: "UTC".to_string(),
                    label: String::new(),
                    locked: false,
                }],
                sort_mode: "manual".to_string(),
                time_format: "system".to_string(),
            }
        );
    }

    #[test]
    fn config_loads_legacy_timezone_list() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(&path, "{\"timezones\": [\"UTC\", \"Asia/Tokyo\"]}\n").unwrap();

        let manager = ConfigManager::new(Some(path));
        let loaded = manager.load_with_local_timezone("UTC").unwrap();

        assert_eq!(
            loaded.timezones,
            vec![
                TimezoneEntry {
                    timezone: "UTC".to_string(),
                    label: String::new(),
                    locked: false,
                },
                TimezoneEntry {
                    timezone: "Asia/Tokyo".to_string(),
                    label: String::new(),
                    locked: false,
                },
            ]
        );
    }

    #[test]
    fn config_normalizes_locked_entries_to_front() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
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
                    locked: true,
                },
            ],
            sort_mode: "manual".to_string(),
            time_format: "system".to_string(),
        };

        manager.save(&config).unwrap();
        let loaded = manager.load_with_local_timezone("UTC").unwrap();
        assert_eq!(loaded.timezones[0].timezone, "Asia/Tokyo");
        assert_eq!(loaded.timezones[1].timezone, "UTC");
    }

    #[test]
    fn ordered_timezones_keeps_locked_entries_first() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();
        let entries = vec![
            TimezoneEntry {
                timezone: "UTC".to_string(),
                label: "Home".to_string(),
                locked: false,
            },
            TimezoneEntry {
                timezone: "Asia/Tokyo".to_string(),
                label: "Tokyo".to_string(),
                locked: true,
            },
            TimezoneEntry {
                timezone: "America/New_York".to_string(),
                label: "New York".to_string(),
                locked: false,
            },
        ];

        let ordered = ordered_timezones(&entries, "time", reference);
        assert_eq!(ordered[0].timezone, "Asia/Tokyo");
        assert_eq!(ordered[1].timezone, "America/New_York");
        assert_eq!(ordered[2].timezone, "UTC");
    }

    #[test]
    fn detects_system_time_format_from_waybar_clock() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.jsonc");
        fs::write(
            &path,
            "{\n  \"clock\": {\n    \"format\": \"{:L%A %I:%M %p}\"\n  }\n}\n",
        )
        .unwrap();

        assert_eq!(
            detect_system_time_format_with_paths(Some(&[path])),
            "ampm".to_string()
        );
    }

    #[test]
    fn canonicalizes_alias_when_system_tzdata_exposes_it() {
        let canonical = canonical_timezone_name("Asia/Calcutta");
        assert!(!canonical.is_empty());
    }

    #[test]
    fn add_timezone_persists_unique_entries() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let updated = manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();
        let duplicated = manager.add_timezone("Asia/Tokyo", "Ignored").unwrap();

        assert_eq!(updated.timezones.len(), 2);
        assert_eq!(duplicated.timezones.len(), 2);
        assert_eq!(duplicated.timezones[1].label, "Tokyo");
    }

    #[test]
    fn remove_timezone_persists_change() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();

        let updated = manager.remove_timezone("Asia/Tokyo").unwrap();
        assert_eq!(updated.timezones.len(), 1);
        assert_eq!(updated.timezones[0].timezone, "UTC");
    }

    #[test]
    fn set_timezone_locked_moves_entry_ahead_of_unlocked_rows() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();

        let updated = manager.set_timezone_locked("Asia/Tokyo", true).unwrap();
        assert_eq!(updated.timezones[0].timezone, "Asia/Tokyo");
        assert!(updated.timezones[0].locked);
    }

    #[test]
    fn set_sort_mode_and_time_format_validate_values() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let sort_updated = manager.set_sort_mode("alpha").unwrap();
        assert_eq!(sort_updated.sort_mode, "alpha");

        let format_updated = manager.set_time_format("ampm").unwrap();
        assert_eq!(format_updated.time_format, "ampm");

        let fallback = manager.set_sort_mode("bogus").unwrap();
        assert_eq!(fallback.sort_mode, "manual");

        let fallback_format = manager.set_time_format("bogus").unwrap();
        assert_eq!(fallback_format.time_format, "system");
    }
}
