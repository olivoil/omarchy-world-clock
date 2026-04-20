use crate::time::friendly_timezone_name;
use anyhow::Context;
use chrono::{Datelike, TimeZone, Utc};
use chrono_tz::{Tz, TZ_VARIANTS};
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

pub const CONFIG_VERSION: u64 = 3;
pub const DEFAULT_TIME_FORMAT: &str = "system";

const VALID_TIME_FORMATS: [&str; 3] = ["system", "24h", "ampm"];
const STANDARD_TZ_REGIONS: [&str; 10] = [
    "Africa",
    "America",
    "Antarctica",
    "Arctic",
    "Asia",
    "Atlantic",
    "Australia",
    "Europe",
    "Indian",
    "Pacific",
];
#[derive(Debug, Clone, Copy)]
struct ManualCityAlias {
    alias: &'static str,
    timezone: &'static str,
    latitude: f64,
    longitude: f64,
}

const MANUAL_CITY_ALIASES: [ManualCityAlias; 9] = [
    ManualCityAlias {
        alias: "Austin",
        timezone: "America/Chicago",
        latitude: 30.2672,
        longitude: -97.7431,
    },
    ManualCityAlias {
        alias: "Bangalore",
        timezone: "Asia/Kolkata",
        latitude: 12.9716,
        longitude: 77.5946,
    },
    ManualCityAlias {
        alias: "Bengaluru",
        timezone: "Asia/Kolkata",
        latitude: 12.9716,
        longitude: 77.5946,
    },
    ManualCityAlias {
        alias: "Delhi",
        timezone: "Asia/Kolkata",
        latitude: 28.6139,
        longitude: 77.2090,
    },
    ManualCityAlias {
        alias: "Faridabad",
        timezone: "Asia/Kolkata",
        latitude: 28.4089,
        longitude: 77.3178,
    },
    ManualCityAlias {
        alias: "Gurgaon",
        timezone: "Asia/Kolkata",
        latitude: 28.4595,
        longitude: 77.0266,
    },
    ManualCityAlias {
        alias: "Gurugram",
        timezone: "Asia/Kolkata",
        latitude: 28.4595,
        longitude: 77.0266,
    },
    ManualCityAlias {
        alias: "New Delhi",
        timezone: "Asia/Kolkata",
        latitude: 28.6139,
        longitude: 77.2090,
    },
    ManualCityAlias {
        alias: "Noida",
        timezone: "Asia/Kolkata",
        latitude: 28.5355,
        longitude: 77.3910,
    },
];

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub timezones: Vec<TimezoneEntry>,
    pub time_format: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimezoneEntry {
    pub timezone: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

impl TimezoneEntry {
    pub fn display_label(&self) -> String {
        let trimmed = self.label.trim();
        if trimmed.is_empty() {
            return friendly_timezone_name(&self.timezone);
        }
        trimmed.to_string()
    }

    pub fn read_card_title(&self) -> String {
        first_location_segment(&self.display_label())
    }
}

pub fn first_location_segment(label: &str) -> String {
    let trimmed = label.trim();
    label
        .split(',')
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimezoneSearchResult {
    pub timezone: String,
    pub title: String,
    pub subtitle: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone)]
struct AliasRecord {
    alias: String,
    normalized_alias: String,
    alias_words: Vec<String>,
    timezone: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Debug, Clone)]
struct TimezoneRecord {
    timezone: String,
    normalized_timezone: String,
    city: String,
    normalized_city: String,
    search_words: Vec<String>,
    abbreviations: Vec<String>,
    abbreviations_folded: Vec<String>,
    search_blob: String,
}

#[derive(Debug, Clone)]
pub struct TimezoneResolver {
    zones: Vec<String>,
    alias_records: Vec<AliasRecord>,
    alias_lookup: HashMap<String, Vec<AliasRecord>>,
    direct_lookup: HashMap<String, String>,
    city_lookup: HashMap<String, Vec<String>>,
    normalized_timezone_lookup: HashMap<String, Vec<String>>,
    abbreviation_lookup: HashMap<String, Vec<String>>,
    records: Vec<TimezoneRecord>,
}

#[derive(Debug, Clone)]
pub struct RemotePlaceSearch {
    zones: HashSet<String>,
    timeout: f64,
    cache: HashMap<String, Vec<TimezoneSearchResult>>,
}

#[derive(Debug, Deserialize)]
struct RemotePlaceResponse {
    results: Option<Vec<RemotePlaceResult>>,
}

#[derive(Debug, Deserialize)]
struct RemotePlaceResult {
    timezone: Option<String>,
    name: Option<String>,
    admin1: Option<String>,
    country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[allow(dead_code)]
    version: Option<u64>,
    timezones: Option<Vec<TimezoneEntry>>,
    time_format: Option<String>,
}

#[derive(Debug, Serialize)]
struct StoredConfig<'a> {
    version: u64,
    timezones: &'a [TimezoneEntry],
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
            time_format: &normalized.time_format,
        };
        let text = serde_json::to_string_pretty(&payload)?;
        fs::write(&self.path, format!("{text}\n"))
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }

    pub fn add_timezone(&self, timezone_name: &str, label: &str) -> anyhow::Result<AppConfig> {
        self.add_timezone_with_coordinate(timezone_name, label, None, None)
    }

    pub fn add_timezone_with_coordinate(
        &self,
        timezone_name: &str,
        label: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> anyhow::Result<AppConfig> {
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
            let (latitude, longitude) = sanitize_place_coordinate(latitude, longitude);
            config.timezones.push(TimezoneEntry {
                timezone: timezone_name,
                label: label.trim().to_string(),
                latitude,
                longitude,
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

    fn config_from_raw(&self, raw: RawConfig, _local_timezone: &str) -> AppConfig {
        let RawConfig {
            timezones,
            time_format,
            ..
        } = raw;

        let time_format = time_format
            .filter(|value| valid_time_format(value))
            .unwrap_or_else(|| DEFAULT_TIME_FORMAT.to_string());
        self.normalize_config(AppConfig {
            timezones: timezones.unwrap_or_default(),
            time_format,
        })
    }

    fn normalize_config(&self, config: AppConfig) -> AppConfig {
        let mut seen = HashSet::new();
        let mut timezones = Vec::new();

        for entry in config.timezones {
            let timezone = canonical_timezone_name(&entry.timezone);
            if timezone.is_empty()
                || !is_valid_timezone(&timezone)
                || !seen.insert(timezone.clone())
            {
                continue;
            }

            let (latitude, longitude) = sanitize_place_coordinate(entry.latitude, entry.longitude);
            timezones.push(TimezoneEntry {
                timezone,
                label: entry.label.trim().to_string(),
                latitude,
                longitude,
            });
        }

        AppConfig {
            timezones,
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
                latitude: None,
                longitude: None,
            }]
        };

        AppConfig {
            timezones,
            time_format: DEFAULT_TIME_FORMAT.to_string(),
        }
    }
}

fn valid_time_format(value: &str) -> bool {
    VALID_TIME_FORMATS.contains(&value)
}

fn valid_place_coordinate(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

fn sanitize_place_coordinate(
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    match (latitude, longitude) {
        (Some(latitude), Some(longitude)) if valid_place_coordinate(latitude, longitude) => {
            (Some(latitude), Some(longitude))
        }
        _ => (None, None),
    }
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

pub fn all_timezones() -> Vec<String> {
    TZ_VARIANTS
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

pub fn canonical_timezone_names<I, S>(zones: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut canonical = Vec::new();
    let mut seen = HashSet::new();
    for timezone_name in zones {
        let resolved = canonical_timezone_name(timezone_name.as_ref());
        if resolved.is_empty() || !seen.insert(resolved.clone()) {
            continue;
        }
        canonical.push(resolved);
    }
    canonical
}

impl TimezoneResolver {
    pub fn new(zones: Option<Vec<String>>) -> Self {
        let zones = canonical_timezone_names(zones.unwrap_or_else(all_timezones));
        let direct_lookup = zones
            .iter()
            .map(|zone| (zone.to_lowercase(), zone.clone()))
            .collect::<HashMap<_, _>>();

        let mut resolver = Self {
            zones: zones.clone(),
            alias_records: Vec::new(),
            alias_lookup: HashMap::new(),
            direct_lookup,
            city_lookup: HashMap::new(),
            normalized_timezone_lookup: HashMap::new(),
            abbreviation_lookup: HashMap::new(),
            records: Vec::new(),
        };

        resolver.records = zones
            .iter()
            .map(|timezone_name| resolver.build_record(timezone_name))
            .collect::<Vec<_>>();
        resolver.alias_records = resolver.build_alias_records();

        for alias in &resolver.alias_records {
            resolver
                .alias_lookup
                .entry(alias.normalized_alias.clone())
                .or_default()
                .push(alias.clone());
        }

        for record in &resolver.records {
            push_lookup_value(
                &mut resolver.normalized_timezone_lookup,
                &record.normalized_timezone,
                &record.timezone,
            );
            push_lookup_value(
                &mut resolver.city_lookup,
                &record.normalized_city,
                &record.timezone,
            );
            for abbreviation in &record.abbreviations_folded {
                push_lookup_value(
                    &mut resolver.abbreviation_lookup,
                    abbreviation,
                    &record.timezone,
                );
            }
        }

        resolver
    }

    pub fn resolve(&self, raw_value: &str) -> Option<String> {
        let candidate = raw_value.trim();
        if candidate.is_empty() {
            return None;
        }

        if let Some(exact) = self.direct_lookup.get(&candidate.to_lowercase()) {
            return Some(exact.clone());
        }

        let normalized = Self::normalize(candidate);
        let exact_normalized = self
            .normalized_timezone_lookup
            .get(&normalized)
            .cloned()
            .unwrap_or_default();
        if exact_normalized.len() == 1 {
            return exact_normalized.first().cloned();
        }

        let alias_matches = self
            .alias_lookup
            .get(&normalized)
            .cloned()
            .unwrap_or_default();
        if !alias_matches.is_empty() {
            let timezones = alias_matches
                .iter()
                .map(|alias| alias.timezone.clone())
                .collect::<HashSet<_>>();
            if timezones.len() == 1 {
                return timezones.into_iter().next();
            }
        }

        let city_matches = self
            .city_lookup
            .get(&normalized)
            .cloned()
            .unwrap_or_default();
        if city_matches.len() == 1 {
            return city_matches.first().cloned();
        }

        let abbreviation_matches = self
            .abbreviation_lookup
            .get(&normalized)
            .cloned()
            .unwrap_or_default();
        if abbreviation_matches.len() == 1 {
            return abbreviation_matches.first().cloned();
        }

        let matches = self.search(candidate, 2);
        if matches.len() == 1 {
            return matches.first().map(|item| item.timezone.clone());
        }
        None
    }

    pub fn search(&self, raw_value: &str, limit: usize) -> Vec<TimezoneSearchResult> {
        let query = Self::normalize(raw_value);
        if query.is_empty() {
            return Vec::new();
        }

        let mut alias_scored = self
            .alias_records
            .iter()
            .filter_map(|alias| {
                self.score_alias(alias, &query)
                    .map(|score| (score, alias.alias.clone(), alias.timezone.clone(), alias))
            })
            .collect::<Vec<_>>();
        let mut scored = self
            .records
            .iter()
            .filter_map(|record| {
                self.score_record(record, &query)
                    .map(|score| (score, record.city.clone(), record.timezone.clone(), record))
            })
            .collect::<Vec<_>>();

        alias_scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });

        let mut results = Vec::new();
        let mut seen_timezones = HashSet::new();

        for (_, _, _, alias) in alias_scored {
            if !seen_timezones.insert(alias.timezone.clone()) {
                continue;
            }
            let Some(record) = self.direct_lookup_record(&alias.timezone) else {
                continue;
            };
            let abbreviation_text = if record.abbreviations.is_empty() {
                "Timezone".to_string()
            } else {
                record.abbreviations.join(" / ")
            };
            results.push(TimezoneSearchResult {
                timezone: alias.timezone.clone(),
                title: alias.alias.clone(),
                subtitle: format!("{}  ·  {}", alias.timezone, abbreviation_text),
                latitude: alias.latitude,
                longitude: alias.longitude,
            });
            if results.len() >= limit {
                return results;
            }
        }

        for (_, _, _, record) in scored {
            if !seen_timezones.insert(record.timezone.clone()) {
                continue;
            }
            let abbreviation_text = if record.abbreviations.is_empty() {
                "Timezone".to_string()
            } else {
                record.abbreviations.join(" / ")
            };
            results.push(TimezoneSearchResult {
                timezone: record.timezone.clone(),
                title: record.city.clone(),
                subtitle: format!("{}  ·  {}", record.timezone, abbreviation_text),
                latitude: None,
                longitude: None,
            });
            if results.len() >= limit {
                break;
            }
        }

        results
    }

    pub fn describe_timezone(&self, timezone_name: &str) -> Option<TimezoneSearchResult> {
        let canonical_timezone = canonical_timezone_name(timezone_name);
        if canonical_timezone.is_empty() {
            return None;
        }

        if let Some(record) = self.direct_lookup_record(&canonical_timezone) {
            let abbreviation_text = if record.abbreviations.is_empty() {
                "Timezone".to_string()
            } else {
                record.abbreviations.join(" / ")
            };
            return Some(TimezoneSearchResult {
                timezone: record.timezone.clone(),
                title: record.city.clone(),
                subtitle: format!("{}  ·  {}", record.timezone, abbreviation_text),
                latitude: None,
                longitude: None,
            });
        }

        if !self.zones.contains(&canonical_timezone) {
            return None;
        }

        Some(TimezoneSearchResult {
            title: friendly_timezone_name(&canonical_timezone),
            subtitle: canonical_timezone.clone(),
            timezone: canonical_timezone,
            latitude: None,
            longitude: None,
        })
    }

    pub fn normalize(value: &str) -> String {
        let without_marks = value
            .nfkd()
            .filter(|character| !is_combining_mark(*character))
            .collect::<String>();
        let translated = without_marks
            .chars()
            .map(|character| match character {
                '/' | '_' | '-' | '.' | ',' | ':' | '(' | ')' | '\'' => ' ',
                _ => character,
            })
            .collect::<String>();
        translated
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn build_alias_records(&self) -> Vec<AliasRecord> {
        let mut aliases = HashMap::new();
        for alias in MANUAL_CITY_ALIASES {
            self.add_alias_record(
                &mut aliases,
                alias.alias,
                alias.timezone,
                Some((alias.latitude, alias.longitude)),
            );
        }

        for (alias, timezone_name) in timezone_link_aliases() {
            self.add_alias_record(&mut aliases, alias, timezone_name, None);

            let mut alias_parts = alias.split('/');
            let alias_region = alias_parts.next().unwrap_or_default();
            let alias_city = alias_parts.next().unwrap_or_default();
            let mut timezone_parts = timezone_name.split('/');
            let timezone_region = timezone_parts.next().unwrap_or_default();
            if !alias_region.is_empty()
                && alias_region == timezone_region
                && STANDARD_TZ_REGIONS.contains(&alias_region)
                && Self::is_city_alias_candidate(alias_city)
            {
                self.add_alias_record(
                    &mut aliases,
                    &alias_city.replace('_', " "),
                    timezone_name,
                    None,
                );
            }
        }

        let mut values = aliases.into_values().collect::<Vec<_>>();
        values.sort_by(|left, right| {
            left.alias
                .cmp(&right.alias)
                .then_with(|| left.timezone.cmp(&right.timezone))
        });
        values
    }

    fn add_alias_record(
        &self,
        aliases: &mut HashMap<(String, String), AliasRecord>,
        alias: &str,
        timezone_name: &str,
        coordinate: Option<(f64, f64)>,
    ) {
        let canonical_timezone = canonical_timezone_name(timezone_name);
        if !self.zones.contains(&canonical_timezone) {
            return;
        }

        let normalized_alias = Self::normalize(alias);
        if normalized_alias.is_empty() {
            return;
        }

        let key = (alias.to_string(), canonical_timezone.clone());
        if aliases.contains_key(&key) {
            return;
        }
        let (latitude, longitude) = coordinate
            .filter(|(latitude, longitude)| valid_place_coordinate(*latitude, *longitude))
            .map(|(latitude, longitude)| (Some(latitude), Some(longitude)))
            .unwrap_or((None, None));

        aliases.insert(
            key,
            AliasRecord {
                alias: alias.to_string(),
                normalized_alias: normalized_alias.clone(),
                alias_words: unique_words(&normalized_alias),
                timezone: canonical_timezone,
                latitude,
                longitude,
            },
        );
    }

    fn is_city_alias_candidate(value: &str) -> bool {
        let letters = value
            .chars()
            .filter(|character| character.is_alphabetic())
            .collect::<Vec<_>>();
        if letters.len() < 4 {
            return false;
        }
        value.to_uppercase() != value
    }

    fn direct_lookup_record(&self, timezone_name: &str) -> Option<&TimezoneRecord> {
        self.records
            .iter()
            .find(|record| record.timezone == timezone_name)
    }

    fn build_record(&self, timezone_name: &str) -> TimezoneRecord {
        let now_utc = Utc::now();
        let zone = Tz::from_str(timezone_name).unwrap_or(chrono_tz::UTC);
        let year = now_utc.year();
        let seasonal_samples = vec![
            now_utc,
            Utc.with_ymd_and_hms(year, 1, 15, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(year, 7, 15, 0, 0, 0).unwrap(),
            now_utc + chrono::Duration::days(182),
        ];

        let mut abbreviations = Vec::new();
        for moment in seasonal_samples {
            let abbreviation = moment.with_timezone(&zone).format("%Z").to_string();
            if !abbreviation.is_empty() && !abbreviations.contains(&abbreviation) {
                abbreviations.push(abbreviation);
            }
        }

        let city = timezone_name
            .split('/')
            .next_back()
            .unwrap_or(timezone_name)
            .replace('_', " ");
        let search_blob = timezone_name.replace(['_', '-'], " ");
        let normalized_timezone = Self::normalize(&timezone_name.replace('/', " "));
        let normalized_city = Self::normalize(&city);
        let search_blob_normalized = Self::normalize(&search_blob);
        let search_words = unique_words(&search_blob_normalized);
        let abbreviations_folded = abbreviations
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<Vec<_>>();

        TimezoneRecord {
            timezone: timezone_name.to_string(),
            normalized_timezone,
            city,
            normalized_city,
            search_words,
            abbreviations,
            abbreviations_folded,
            search_blob: search_blob_normalized,
        }
    }

    fn score_record(&self, record: &TimezoneRecord, query: &str) -> Option<i32> {
        if query == record.timezone.to_lowercase() {
            return Some(1400);
        }
        if query == record.normalized_timezone {
            return Some(1360);
        }
        if query == record.normalized_city {
            return Some(1320);
        }
        if record.abbreviations_folded.iter().any(|item| item == query) {
            return Some(if record.abbreviations_folded.len() == 1 {
                1280
            } else {
                1260
            });
        }
        if record.normalized_timezone.starts_with(query) {
            return Some(1180);
        }
        if record
            .search_words
            .iter()
            .any(|word| word.starts_with(query))
        {
            return Some(1120);
        }
        if record.normalized_city.contains(query) {
            return Some(1060);
        }
        if record.normalized_timezone.contains(query) {
            return Some(1000);
        }
        if record
            .abbreviations_folded
            .iter()
            .any(|abbreviation| abbreviation.contains(query))
        {
            return Some(960);
        }
        if record.search_blob.contains(query) {
            return Some(920);
        }
        None
    }

    fn score_alias(&self, alias: &AliasRecord, query: &str) -> Option<i32> {
        if query == alias.normalized_alias {
            return Some(1500);
        }
        if alias.normalized_alias.starts_with(query) {
            return Some(1440);
        }
        if alias.alias_words.iter().any(|word| word.starts_with(query)) {
            return Some(1400);
        }
        if alias.normalized_alias.contains(query) {
            return Some(1340);
        }
        None
    }
}

impl RemotePlaceSearch {
    const ENDPOINT: &'static str = "https://geocoding-api.open-meteo.com/v1/search";

    pub fn new(zones: Option<Vec<String>>, timeout: Option<f64>) -> Self {
        Self {
            zones: canonical_timezone_names(zones.unwrap_or_else(all_timezones))
                .into_iter()
                .collect(),
            timeout: timeout.unwrap_or(2.5),
            cache: HashMap::new(),
        }
    }

    pub fn search(&mut self, raw_value: &str, limit: usize) -> Vec<TimezoneSearchResult> {
        let query = raw_value.split_whitespace().collect::<Vec<_>>().join(" ");
        let query_key = TimezoneResolver::normalize(&query);
        if query_key.len() < 3 {
            return Vec::new();
        }

        let cached = if let Some(cached) = self.cache.get(&query_key) {
            cached.clone()
        } else {
            let fetched = self.fetch(&query);
            self.cache.insert(query_key.clone(), fetched.clone());
            fetched
        };

        cached.into_iter().take(limit).collect()
    }

    fn fetch(&self, query: &str) -> Vec<TimezoneSearchResult> {
        let Ok(client) = Client::builder()
            .timeout(Duration::from_secs_f64(self.timeout))
            .build()
        else {
            return Vec::new();
        };

        let Ok(response) = client
            .get(Self::ENDPOINT)
            .query(&[("name", query), ("count", "12"), ("format", "json")])
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, "omarchy-world-clock/1.0")
            .send()
        else {
            return Vec::new();
        };

        let Ok(payload) = response.json::<RemotePlaceResponse>() else {
            return Vec::new();
        };

        let mut results = Vec::new();
        let mut seen_timezones = HashSet::new();
        for item in payload.results.unwrap_or_default() {
            let Some(raw_timezone) = item.timezone.as_deref() else {
                continue;
            };

            let timezone_name = canonical_timezone_name(raw_timezone.trim());
            if !self.zones.contains(&timezone_name) || !seen_timezones.insert(timezone_name.clone())
            {
                continue;
            }

            let Some(title) = Self::format_title(&item) else {
                continue;
            };

            let mut subtitle_parts = vec![timezone_name.clone()];
            let location_summary = Self::format_location_summary(&item);
            if !location_summary.is_empty() {
                subtitle_parts.push(location_summary);
            }
            let (latitude, longitude) = sanitize_place_coordinate(item.latitude, item.longitude);

            results.push(TimezoneSearchResult {
                timezone: timezone_name,
                title,
                subtitle: subtitle_parts.join("  ·  "),
                latitude,
                longitude,
            });
        }

        results
    }

    fn format_title(item: &RemotePlaceResult) -> Option<String> {
        let parts = Self::unique_parts([
            item.name.as_deref(),
            item.admin1.as_deref(),
            item.country.as_deref(),
        ]);
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }

    fn format_location_summary(item: &RemotePlaceResult) -> String {
        Self::unique_parts([item.admin1.as_deref(), item.country.as_deref()]).join(", ")
    }

    fn unique_parts<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Vec<String> {
        let mut parts = Vec::new();
        let mut seen = HashSet::new();
        for value in values {
            let Some(value) = value else {
                continue;
            };
            let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if cleaned.is_empty() {
                continue;
            }
            let folded = cleaned.to_lowercase();
            if !seen.insert(folded) {
                continue;
            }
            parts.push(cleaned);
        }
        parts
    }
}

fn push_lookup_value(lookup: &mut HashMap<String, Vec<String>>, key: &str, value: &str) {
    let entry = lookup.entry(key.to_string()).or_default();
    if !entry.iter().any(|existing| existing == value) {
        entry.push(value.to_string());
    }
}

fn unique_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut seen = HashSet::new();
    for word in value.split_whitespace() {
        if seen.insert(word.to_string()) {
            words.push(word.to_string());
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_timezone_name, detect_system_time_format_with_paths, AppConfig, ConfigManager,
        TimezoneEntry, TimezoneResolver,
    };
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
                    latitude: None,
                    longitude: None,
                }],
                time_format: "system".to_string(),
            }
        );
    }

    #[test]
    fn config_normalizes_entries_preserving_order() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
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
            time_format: "system".to_string(),
        };

        manager.save(&config).unwrap();
        let loaded = manager.load_with_local_timezone("UTC").unwrap();
        assert_eq!(loaded.timezones[0].timezone, "UTC");
        assert_eq!(loaded.timezones[1].timezone, "Asia/Tokyo");
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
    fn add_timezone_with_coordinate_persists_valid_place_coordinate() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let updated = manager
            .add_timezone_with_coordinate(
                "America/Chicago",
                "Austin",
                Some(30.2672),
                Some(-97.7431),
            )
            .unwrap();

        assert_eq!(updated.timezones[1].label, "Austin");
        assert_eq!(updated.timezones[1].latitude, Some(30.2672));
        assert_eq!(updated.timezones[1].longitude, Some(-97.7431));

        let loaded = manager.load_with_local_timezone("UTC").unwrap();
        assert_eq!(loaded.timezones[1].latitude, Some(30.2672));
        assert_eq!(loaded.timezones[1].longitude, Some(-97.7431));
    }

    #[test]
    fn manual_city_alias_results_include_place_coordinates() {
        let resolver = TimezoneResolver::new(Some(vec!["America/Chicago".to_string()]));
        let result = resolver.search("Austin", 1).pop().unwrap();

        assert_eq!(result.timezone, "America/Chicago");
        assert_eq!(result.latitude, Some(30.2672));
        assert_eq!(result.longitude, Some(-97.7431));
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
    fn set_time_format_validates_values() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let format_updated = manager.set_time_format("ampm").unwrap();
        assert_eq!(format_updated.time_format, "ampm");

        let fallback_format = manager.set_time_format("bogus").unwrap();
        assert_eq!(fallback_format.time_format, "system");
    }
}
