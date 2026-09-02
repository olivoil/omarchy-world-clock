use crate::remote_response::{
    open_meteo_client, read_json_response, MAX_OPEN_METEO_RESPONSE_BYTES,
};
use crate::time::friendly_timezone_name;
use crate::timezone_grid::timezone_at;
use anyhow::Context;
use chrono::{Datelike, TimeZone, Utc};
use chrono_tz::{Tz, TZ_VARIANTS};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::CStr;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

pub const CONFIG_VERSION: u64 = 8;
pub const LOCAL_TIMEZONE_MIGRATION_VERSION: u64 = 2;
const SEPARATE_PLACE_LABEL_VERSION: u64 = 8;

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

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub timezones: Vec<TimezoneEntry>,
    pub pinned_locations: Vec<LocationKey>,
    pub disable_open_meteo_geolocation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddLocationOutcome {
    pub config: AppConfig,
    pub added: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationKey {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u64,
    // Retained only so v7-and-earlier pin identities can be migrated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub timezone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

#[derive(Debug)]
pub(crate) struct PinnedLocationIndex<'a> {
    pinned_entries: Vec<&'a TimezoneEntry>,
    pinned_ids: HashSet<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimezoneEntry {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u64,
    pub timezone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub place: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

impl TimezoneEntry {
    pub fn place_label(&self) -> String {
        let trimmed = self.place.trim();
        if trimmed.is_empty() {
            return friendly_timezone_name(&self.timezone);
        }
        trimmed.to_string()
    }

    pub fn display_label(&self) -> String {
        let trimmed = self.label.trim();
        if trimmed.is_empty() {
            return self.place_label();
        }
        trimmed.to_string()
    }

    pub fn read_card_title(&self) -> String {
        first_location_segment(&self.display_label())
    }

    pub fn read_place_title(&self) -> String {
        first_location_segment(&self.place_label())
    }

    pub fn has_custom_label(&self) -> bool {
        !self.label.trim().is_empty()
    }

    pub fn location_key(&self) -> LocationKey {
        LocationKey {
            id: self.id,
            timezone: String::new(),
            label: String::new(),
        }
    }

    // Compatibility lookup for v7 config migration and older CLI callers.
    pub fn matches_location(&self, timezone: &str, label: &str) -> bool {
        self.timezone == canonical_timezone_name(timezone)
            && normalized_location_label(&self.timezone, &self.display_label())
                == normalized_location_label(timezone, label)
    }

    pub fn matches_place(&self, timezone: &str, place: &str) -> bool {
        self.timezone == canonical_timezone_name(timezone)
            && normalized_location_label(&self.timezone, &self.place_label())
                == normalized_location_label(timezone, place)
    }
}

impl LocationKey {
    pub fn matches(&self, entry: &TimezoneEntry) -> bool {
        if self.id != 0 {
            return self.id == entry.id;
        }
        entry.matches_location(&self.timezone, &self.label)
    }
}

impl<'a> PinnedLocationIndex<'a> {
    pub(crate) fn pinned_entries(&self) -> impl Iterator<Item = &'a TimezoneEntry> + '_ {
        self.pinned_entries.iter().copied()
    }

    pub(crate) fn first_pinned_entry(&self) -> Option<&'a TimezoneEntry> {
        self.pinned_entries.first().copied()
    }

    pub(crate) fn contains(&self, entry: &TimezoneEntry) -> bool {
        entry.id != 0 && self.pinned_ids.contains(&entry.id)
    }
}

impl AppConfig {
    pub(crate) fn pinned_location_index(&self) -> PinnedLocationIndex<'_> {
        let mut entries_by_id = HashMap::with_capacity(self.timezones.len());
        for entry in &self.timezones {
            entries_by_id.entry(entry.id).or_insert(entry);
        }

        let mut pinned_entries = Vec::with_capacity(self.pinned_locations.len());
        let mut pinned_ids = HashSet::with_capacity(self.pinned_locations.len());
        for pinned in &self.pinned_locations {
            let entry = if pinned.id != 0 {
                entries_by_id.get(&pinned.id).copied()
            } else {
                self.timezones.iter().find(|entry| pinned.matches(entry))
            };
            if let Some(entry) = entry {
                pinned_entries.push(entry);
                pinned_ids.insert(entry.id);
            }
        }

        PinnedLocationIndex {
            pinned_entries,
            pinned_ids,
        }
    }

    pub fn pinned_entries(&self) -> impl Iterator<Item = &TimezoneEntry> {
        self.pinned_location_index().pinned_entries.into_iter()
    }

    // Preserve the original single-pin accessor for callers that only need
    // the first pinned location.
    pub fn pinned_entry(&self) -> Option<&TimezoneEntry> {
        self.pinned_entries().next()
    }

    pub fn is_pinned(&self, entry: &TimezoneEntry) -> bool {
        self.pinned_locations
            .iter()
            .any(|location| location.matches(entry))
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

fn normalized_location_label(timezone: &str, label: &str) -> String {
    let label = label.trim();
    let effective_label = if label.is_empty() {
        friendly_timezone_name(&canonical_timezone_name(timezone))
    } else {
        label.to_string()
    };
    TimezoneResolver::normalize(&effective_label)
}

fn next_unused_location_id(used: &HashSet<u64>) -> u64 {
    (1..=u64::MAX)
        .find(|candidate| !used.contains(candidate))
        .expect("all World Clock location IDs are exhausted")
}

fn assign_missing_location_ids(entries: &mut [TimezoneEntry]) {
    let mut used = HashSet::with_capacity(entries.len());
    for entry in entries {
        if entry.id == 0 || used.contains(&entry.id) {
            entry.id = next_unused_location_id(&used);
        }
        used.insert(entry.id);
    }
}

fn next_location_id(entries: &[TimezoneEntry]) -> u64 {
    let used = entries.iter().map(|entry| entry.id).collect::<HashSet<_>>();
    next_unused_location_id(&used)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimezoneSearchResult {
    pub timezone: String,
    pub title: String,
    pub subtitle: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub open_meteo_attribution: bool,
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawTimezoneEntry {
    Legacy(String),
    Structured {
        #[serde(default)]
        id: u64,
        timezone: String,
        #[serde(default)]
        place: Option<String>,
        #[serde(default)]
        label: String,
        #[serde(default)]
        latitude: Option<f64>,
        #[serde(default)]
        longitude: Option<f64>,
    },
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[allow(dead_code)]
    version: Option<u64>,
    timezones: Option<Vec<RawTimezoneEntry>>,
    pinned_locations: Option<Vec<LocationKey>>,
    // v6 identified one pin by timezone and label.
    pinned_location: Option<LocationKey>,
    // v5 and earlier identified the pin only by timezone.
    pinned_timezone: Option<String>,
    disable_open_meteo_geolocation: Option<bool>,
}

#[derive(Debug, Serialize)]
struct StoredConfig<'a> {
    version: u64,
    timezones: &'a [TimezoneEntry],
    #[serde(skip_serializing_if = "Option::is_none")]
    pinned_locations: Option<&'a [LocationKey]>,
    #[serde(skip_serializing_if = "is_false")]
    disable_open_meteo_geolocation: bool,
}

#[derive(Debug, Clone)]
pub struct ConfigManager {
    path: PathBuf,
}

struct ConfigFileLock {
    file: File,
}

impl Drop for ConfigFileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
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
        let (config, needs_write) = self.read_config(local_timezone)?;
        if !needs_write {
            return Ok(config);
        }

        let _lock = self.lock_config()?;
        self.load_unlocked(local_timezone)
    }

    fn read_config(&self, local_timezone: &str) -> anyhow::Result<(AppConfig, bool)> {
        if !self.path.exists() {
            return Ok((self.default_config(local_timezone), true));
        }

        let text = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let raw = serde_json::from_str::<RawConfig>(&text)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;
        let config = self.config_from_raw(raw, local_timezone);
        let normalized_text = self.serialize(&config)?;
        Ok((config, text != normalized_text))
    }

    fn load_unlocked(&self, local_timezone: &str) -> anyhow::Result<AppConfig> {
        let (config, needs_write) = self.read_config(local_timezone)?;
        if needs_write {
            self.save_unlocked(&config)?;
        }
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> anyhow::Result<()> {
        let _lock = self.lock_config()?;
        self.save_unlocked(config)
    }

    fn save_unlocked(&self, config: &AppConfig) -> anyhow::Result<()> {
        let normalized = self.normalize_config(config.clone());
        let text = self.serialize(&normalized)?;
        self.write_atomically(&text)
    }

    fn lock_config(&self) -> anyhow::Result<ConfigFileLock> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let mut lock_path = self.path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock_path = PathBuf::from(lock_path);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("failed to open config lock {}", lock_path.display()))?;
        file.lock()
            .with_context(|| format!("failed to lock config {}", self.path.display()))?;
        Ok(ConfigFileLock { file })
    }

    fn mutate_with_local_timezone<F>(
        &self,
        local_timezone: &str,
        mutate: F,
    ) -> anyhow::Result<(AppConfig, bool)>
    where
        F: FnOnce(&mut AppConfig) -> anyhow::Result<bool>,
    {
        let _lock = self.lock_config()?;
        let mut config = self.load_unlocked(local_timezone)?;
        let changed = mutate(&mut config)?;
        if changed {
            config = self.normalize_config(config);
            self.save_unlocked(&config)?;
        }
        Ok((config, changed))
    }

    fn serialize(&self, config: &AppConfig) -> anyhow::Result<String> {
        let payload = StoredConfig {
            version: CONFIG_VERSION,
            timezones: &config.timezones,
            pinned_locations: (!config.pinned_locations.is_empty())
                .then_some(config.pinned_locations.as_slice()),
            disable_open_meteo_geolocation: config.disable_open_meteo_geolocation,
        };
        let text = serde_json::to_string_pretty(&payload)?;
        Ok(format!("{text}\n"))
    }

    fn write_atomically(&self, text: &str) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.json");
        let temporary_path = self.path.with_file_name(format!(
            ".{file_name}.tmp-{}-{timestamp}",
            std::process::id()
        ));

        let result = (|| -> anyhow::Result<()> {
            let mut temporary_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .with_context(|| format!("failed to create {}", temporary_path.display()))?;
            temporary_file
                .write_all(text.as_bytes())
                .with_context(|| format!("failed to write {}", temporary_path.display()))?;
            temporary_file
                .sync_all()
                .with_context(|| format!("failed to sync {}", temporary_path.display()))?;
            fs::rename(&temporary_path, &self.path).with_context(|| {
                format!(
                    "failed to replace {} with {}",
                    self.path.display(),
                    temporary_path.display()
                )
            })?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    pub fn add_timezone(&self, timezone_name: &str, label: &str) -> anyhow::Result<AppConfig> {
        Ok(self
            .add_timezone_with_coordinate(timezone_name, label, None, None)?
            .config)
    }

    pub fn add_timezone_with_coordinate(
        &self,
        timezone_name: &str,
        place: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> anyhow::Result<AddLocationOutcome> {
        self.add_location_with_coordinate(timezone_name, place, "", latitude, longitude)
    }

    pub fn add_location_with_coordinate(
        &self,
        timezone_name: &str,
        place: &str,
        label: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> anyhow::Result<AddLocationOutcome> {
        let local_timezone = detect_local_timezone();
        self.add_location_with_coordinate_for_local(
            timezone_name,
            place,
            label,
            latitude,
            longitude,
            &local_timezone,
        )
    }

    #[cfg(test)]
    fn add_timezone_with_coordinate_for_local(
        &self,
        timezone_name: &str,
        place: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
        local_timezone: &str,
    ) -> anyhow::Result<AddLocationOutcome> {
        self.add_location_with_coordinate_for_local(
            timezone_name,
            place,
            "",
            latitude,
            longitude,
            local_timezone,
        )
    }

    fn add_location_with_coordinate_for_local(
        &self,
        timezone_name: &str,
        place: &str,
        label: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
        local_timezone: &str,
    ) -> anyhow::Result<AddLocationOutcome> {
        let timezone_name = canonical_timezone_name(timezone_name);
        let place = place.trim().to_string();
        let label = label.trim().to_string();
        let (latitude, longitude) = sanitize_place_coordinate(latitude, longitude);
        let (config, added) = self.mutate_with_local_timezone(local_timezone, move |config| {
            if timezone_name.is_empty() || !is_valid_timezone(&timezone_name) {
                return Ok(false);
            }

            config.timezones.push(TimezoneEntry {
                id: next_location_id(&config.timezones),
                timezone: timezone_name,
                place,
                label,
                latitude,
                longitude,
            });
            Ok(true)
        })?;
        Ok(AddLocationOutcome { config, added })
    }

    pub fn remove_timezone(&self, timezone_name: &str) -> anyhow::Result<AppConfig> {
        self.remove_location(timezone_name, None)
    }

    pub fn rename_location(
        &self,
        timezone_name: &str,
        current_label: &str,
        new_label: &str,
    ) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        let timezone_name = canonical_timezone_name(timezone_name);
        let current_label = current_label.to_string();
        let new_label = new_label.trim().to_string();
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            let matches = config
                .timezones
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.matches_location(&timezone_name, &current_label))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                anyhow::bail!("location is not in the World Clock list: {timezone_name}");
            }
            if matches.len() > 1 {
                anyhow::bail!("multiple locations use {timezone_name}; specify a location ID");
            }

            let index = matches[0];
            if config.timezones[index].label == new_label {
                return Ok(false);
            }
            config.timezones[index].label = new_label;
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    pub fn rename_location_by_id(&self, id: u64, new_label: &str) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        let new_label = new_label.trim().to_string();
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            let Some(entry) = config.timezones.iter_mut().find(|entry| entry.id == id) else {
                anyhow::bail!("location is not in the World Clock list: {id}");
            };
            if entry.label == new_label {
                return Ok(false);
            }
            entry.label = new_label;
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    pub fn remove_location(
        &self,
        timezone_name: &str,
        label: Option<&str>,
    ) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        let timezone_name = canonical_timezone_name(timezone_name);
        let label = label.map(str::to_string);
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            let matches = config
                .timezones
                .iter()
                .filter(|entry| {
                    entry.timezone == timezone_name
                        && label
                            .as_deref()
                            .is_none_or(|label| entry.matches_location(&timezone_name, label))
                })
                .count();
            if matches == 0 {
                anyhow::bail!("location is not in the World Clock list: {timezone_name}");
            }
            if matches > 1 {
                anyhow::bail!("multiple locations use {timezone_name}; specify a location ID");
            }
            if config.timezones.len() <= 1 {
                anyhow::bail!("keep at least one timezone in World Clock");
            }
            config.timezones.retain(|entry| {
                entry.timezone != timezone_name
                    || label
                        .as_deref()
                        .is_some_and(|label| !entry.matches_location(&timezone_name, label))
            });
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    pub fn remove_location_by_id(&self, id: u64) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            if !config.timezones.iter().any(|entry| entry.id == id) {
                anyhow::bail!("location is not in the World Clock list: {id}");
            }
            if config.timezones.len() <= 1 {
                anyhow::bail!("keep at least one timezone in World Clock");
            }
            config.timezones.retain(|entry| entry.id != id);
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    pub fn set_pinned_timezone(&self, timezone_name: Option<&str>) -> anyhow::Result<AppConfig> {
        self.set_pinned_location(timezone_name, None)
    }

    pub fn pin_location(
        &self,
        timezone_name: &str,
        label: Option<&str>,
    ) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        let timezone_name = canonical_timezone_name(timezone_name);
        let label = label.map(str::to_string);
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            let matches = config
                .timezones
                .iter()
                .filter(|entry| {
                    entry.timezone == timezone_name
                        && label
                            .as_deref()
                            .is_none_or(|label| entry.matches_location(&timezone_name, label))
                })
                .collect::<Vec<_>>();
            if matches.is_empty() {
                anyhow::bail!("location is not in the World Clock list: {timezone_name}");
            }
            if matches.len() > 1 {
                anyhow::bail!("multiple locations use {timezone_name}; specify a location ID");
            }
            let key = matches[0].location_key();
            if config.pinned_locations.contains(&key) {
                return Ok(false);
            }
            config.pinned_locations.push(key);
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    pub fn pin_location_by_id(&self, id: u64) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            let Some(entry) = config.timezones.iter().find(|entry| entry.id == id) else {
                anyhow::bail!("location is not in the World Clock list: {id}");
            };
            let key = entry.location_key();
            if config.pinned_locations.contains(&key) {
                return Ok(false);
            }
            config.pinned_locations.push(key);
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    pub fn unpin_location(
        &self,
        timezone_name: &str,
        label: Option<&str>,
    ) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        let timezone_name = canonical_timezone_name(timezone_name);
        let label = label.map(str::to_string);
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            let matches = config
                .timezones
                .iter()
                .filter(|entry| {
                    entry.timezone == timezone_name
                        && label
                            .as_deref()
                            .is_none_or(|label| entry.matches_location(&timezone_name, label))
                })
                .collect::<Vec<_>>();
            if matches.is_empty() {
                anyhow::bail!("location is not in the World Clock list: {timezone_name}");
            }
            if matches.len() > 1 {
                anyhow::bail!("multiple locations use {timezone_name}; specify a location ID");
            }
            let key = matches[0].location_key();
            let previous_len = config.pinned_locations.len();
            config.pinned_locations.retain(|pinned| pinned != &key);
            Ok(config.pinned_locations.len() != previous_len)
        })
        .map(|(config, _)| config)
    }

    pub fn unpin_location_by_id(&self, id: u64) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            if !config.timezones.iter().any(|entry| entry.id == id) {
                anyhow::bail!("location is not in the World Clock list: {id}");
            }
            let previous_len = config.pinned_locations.len();
            config.pinned_locations.retain(|pinned| pinned.id != id);
            Ok(config.pinned_locations.len() != previous_len)
        })
        .map(|(config, _)| config)
    }

    pub fn clear_pinned_locations(&self) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        self.mutate_with_local_timezone(&local_timezone, |config| {
            if config.pinned_locations.is_empty() {
                return Ok(false);
            }
            config.pinned_locations.clear();
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    // Retain the setter's replace semantics for API compatibility. Interactive
    // pinning uses `pin_location` so existing pins remain selected.
    pub fn set_pinned_location(
        &self,
        timezone_name: Option<&str>,
        label: Option<&str>,
    ) -> anyhow::Result<AppConfig> {
        let local_timezone = detect_local_timezone();
        let timezone_name = timezone_name.map(canonical_timezone_name);
        let label = label.map(str::to_string);
        self.mutate_with_local_timezone(&local_timezone, move |config| {
            config.pinned_locations = match timezone_name {
                None => Vec::new(),
                Some(ref timezone_name) => {
                    let matches = config
                        .timezones
                        .iter()
                        .filter(|entry| {
                            entry.timezone == *timezone_name
                                && label.as_deref().is_none_or(|label| {
                                    entry.matches_location(timezone_name, label)
                                })
                        })
                        .collect::<Vec<_>>();
                    if matches.is_empty() {
                        anyhow::bail!("location is not in the World Clock list: {timezone_name}");
                    }
                    if matches.len() > 1 {
                        anyhow::bail!(
                            "multiple locations use {timezone_name}; specify a location ID"
                        );
                    }
                    vec![matches[0].location_key()]
                }
            };
            Ok(true)
        })
        .map(|(config, _)| config)
    }

    fn config_from_raw(&self, raw: RawConfig, local_timezone: &str) -> AppConfig {
        let RawConfig {
            version,
            timezones,
            pinned_locations,
            pinned_location,
            pinned_timezone,
            disable_open_meteo_geolocation,
        } = raw;
        let config_version = version.unwrap_or(1);
        let mut entries = Vec::new();

        for raw_entry in timezones.unwrap_or_default() {
            let Some(entry) = self.parse_entry(raw_entry, config_version) else {
                continue;
            };
            entries.push(entry);
        }
        assign_missing_location_ids(&mut entries);

        if config_version < LOCAL_TIMEZONE_MIGRATION_VERSION {
            let local_timezone = canonical_timezone_name(local_timezone);
            if !local_timezone.is_empty()
                && is_valid_timezone(&local_timezone)
                && !entries
                    .iter()
                    .any(|entry| entry.matches_location(&local_timezone, ""))
            {
                entries.insert(
                    0,
                    TimezoneEntry {
                        id: next_location_id(&entries),
                        timezone: local_timezone,
                        place: String::new(),
                        label: String::new(),
                        latitude: None,
                        longitude: None,
                    },
                );
            }
        }

        let pinned_locations = match pinned_locations {
            Some(locations) => locations,
            None => {
                let singular_pin = pinned_location.or_else(|| {
                    let pinned_timezone = canonical_timezone_name(pinned_timezone.as_deref()?);
                    entries
                        .iter()
                        .find(|entry| entry.timezone == pinned_timezone)
                        .map(TimezoneEntry::location_key)
                });
                singular_pin.into_iter().collect()
            }
        };

        self.normalize_config(AppConfig {
            timezones: entries,
            pinned_locations,
            disable_open_meteo_geolocation: disable_open_meteo_geolocation.unwrap_or(false),
        })
    }

    fn parse_entry(
        &self,
        raw_entry: RawTimezoneEntry,
        config_version: u64,
    ) -> Option<TimezoneEntry> {
        let (id, timezone, place, label, latitude, longitude) = match raw_entry {
            RawTimezoneEntry::Legacy(timezone) => {
                (0, timezone, String::new(), String::new(), None, None)
            }
            RawTimezoneEntry::Structured {
                id,
                timezone,
                place,
                label,
                latitude,
                longitude,
            } => {
                if config_version < SEPARATE_PLACE_LABEL_VERSION && place.is_none() {
                    (id, timezone, label, String::new(), latitude, longitude)
                } else {
                    (
                        id,
                        timezone,
                        place.unwrap_or_default(),
                        label,
                        latitude,
                        longitude,
                    )
                }
            }
        };

        let timezone = canonical_timezone_name(&timezone);
        if timezone.is_empty() || !is_valid_timezone(&timezone) {
            return None;
        }
        let (latitude, longitude) = sanitize_place_coordinate(latitude, longitude);

        Some(TimezoneEntry {
            id,
            timezone,
            place: place.trim().to_string(),
            label: label.trim().to_string(),
            latitude,
            longitude,
        })
    }

    fn normalize_config(&self, config: AppConfig) -> AppConfig {
        let pinned_locations = config.pinned_locations;
        let mut timezones = Vec::new();

        for entry in config.timezones {
            let timezone = canonical_timezone_name(&entry.timezone);
            if timezone.is_empty() || !is_valid_timezone(&timezone) {
                continue;
            }

            let (latitude, longitude) = sanitize_place_coordinate(entry.latitude, entry.longitude);
            let normalized = TimezoneEntry {
                id: entry.id,
                timezone,
                place: entry.place.trim().to_string(),
                label: entry.label.trim().to_string(),
                latitude,
                longitude,
            };

            timezones.push(normalized);
        }
        assign_missing_location_ids(&mut timezones);

        let mut normalized_pins = Vec::new();
        for pinned in pinned_locations {
            let Some(key) = timezones
                .iter()
                .find(|entry| pinned.matches(entry))
                .map(TimezoneEntry::location_key)
            else {
                continue;
            };
            if !normalized_pins.contains(&key) {
                normalized_pins.push(key);
            }
        }

        AppConfig {
            timezones,
            pinned_locations: normalized_pins,
            disable_open_meteo_geolocation: config.disable_open_meteo_geolocation,
        }
    }

    fn default_config(&self, local_timezone: &str) -> AppConfig {
        let local_timezone = canonical_timezone_name(local_timezone);
        let timezones = if local_timezone.is_empty() || !is_valid_timezone(&local_timezone) {
            Vec::new()
        } else {
            let place = friendly_timezone_name(&local_timezone);
            vec![TimezoneEntry {
                id: 1,
                timezone: local_timezone,
                place,
                label: String::new(),
                latitude: None,
                longitude: None,
            }]
        };

        AppConfig {
            timezones,
            pinned_locations: Vec::new(),
            disable_open_meteo_geolocation: false,
        }
    }
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

fn parse_iso6709_component(component: &str, degree_digits: usize) -> Option<f64> {
    let sign = match component.chars().next()? {
        '+' => 1.0,
        '-' => -1.0,
        _ => return None,
    };
    let digits = &component[1..];
    if digits.len() != degree_digits + 2 && digits.len() != degree_digits + 4 {
        return None;
    }

    let degrees = digits.get(..degree_digits)?.parse::<f64>().ok()?;
    let minutes = digits
        .get(degree_digits..degree_digits + 2)?
        .parse::<f64>()
        .ok()?;
    let seconds = if digits.len() == degree_digits + 4 {
        digits
            .get(degree_digits + 2..degree_digits + 4)?
            .parse::<f64>()
            .ok()?
    } else {
        0.0
    };

    if minutes >= 60.0 || seconds >= 60.0 {
        return None;
    }

    Some(sign * (degrees + minutes / 60.0 + seconds / 3600.0))
}

fn parse_zone_tab_coordinate(value: &str) -> Option<(f64, f64)> {
    let longitude_start = value
        .char_indices()
        .skip(1)
        .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index))?;
    let latitude = parse_iso6709_component(value.get(..longitude_start)?, 2)?;
    let longitude = parse_iso6709_component(value.get(longitude_start..)?, 3)?;
    Some((latitude, longitude))
}

fn merge_zone_tab_coordinates(coordinates: &mut BTreeMap<String, (f64, f64)>, text: &str) {
    for (timezone_name, coordinate) in text.lines().filter_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let mut columns = trimmed.split('\t');
        let _country_codes = columns.next()?;
        let coordinate_text = columns.next()?;
        let timezone_name = columns.next()?;
        let coordinate = parse_zone_tab_coordinate(coordinate_text)?;
        Some((timezone_name.to_string(), coordinate))
    }) {
        coordinates.entry(timezone_name).or_insert(coordinate);
    }
}

fn timezone_coordinate_lookup() -> &'static BTreeMap<String, (f64, f64)> {
    static COORDINATES: OnceLock<BTreeMap<String, (f64, f64)>> = OnceLock::new();
    COORDINATES.get_or_init(|| {
        let mut coordinates = BTreeMap::new();
        for root in zoneinfo_roots() {
            for filename in ["zone1970.tab", "zone.tab"] {
                let Ok(text) = fs::read_to_string(root.join(filename)) else {
                    continue;
                };
                merge_zone_tab_coordinates(&mut coordinates, &text);
            }
        }
        coordinates
    })
}

/// Return a persisted place coordinate, falling back to the canonical
/// timezone coordinate bundled with the system tzdata. This stays local and
/// cheap enough for the native snapshot path; remote geocoding remains a
/// search-only operation.
pub fn place_coordinate(entry: &TimezoneEntry) -> Option<(f64, f64)> {
    match sanitize_place_coordinate(entry.latitude, entry.longitude) {
        (Some(latitude), Some(longitude)) => Some((latitude, longitude)),
        _ => {
            let coordinates = timezone_coordinate_lookup();
            coordinates.get(&entry.timezone).copied().or_else(|| {
                coordinates
                    .get(&canonical_timezone_name(&entry.timezone))
                    .copied()
            })
        }
    }
}

fn timezone_coordinate(timezone: &str) -> (Option<f64>, Option<f64>) {
    place_coordinate(&TimezoneEntry {
        id: 0,
        timezone: canonical_timezone_name(timezone),
        place: String::new(),
        label: String::new(),
        latitude: None,
        longitude: None,
    })
    .map(|(latitude, longitude)| (Some(latitude), Some(longitude)))
    .unwrap_or((None, None))
}

fn verified_colocated_timezone_link_coordinate(
    alias: &str,
    timezone_name: &str,
) -> Option<(f64, f64)> {
    if let Some(coordinate) = timezone_coordinate_lookup().get(alias).copied() {
        return Some(coordinate);
    }

    // Most tzdb links only share clock rules and cannot safely inherit the
    // canonical zone's geography. Prefer an alias's own zone.tab entry above,
    // then keep the small set of true city renames explicit so those aliases
    // can still focus the globe offline.
    let colocated = matches!((alias, timezone_name), ("Asia/Saigon", "Asia/Ho_Chi_Minh"));
    if !colocated {
        return None;
    }

    let (Some(latitude), Some(longitude)) = timezone_coordinate(timezone_name) else {
        return None;
    };
    Some((latitude, longitude))
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u64) -> bool {
    *value == 0
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

pub fn omarchy_shell_config_paths() -> Vec<PathBuf> {
    let omarchy_root = env::var_os("OMARCHY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/omarchy"));
    vec![
        home_dir().join(".config/omarchy/shell.json"),
        omarchy_root.join("config/omarchy/shell.json"),
    ]
}

fn load_omarchy_shell_widget_setting(
    paths: Option<&[PathBuf]>,
    widget_id: &str,
    setting_name: &str,
) -> Option<String> {
    let candidates = paths
        .map(|paths| paths.to_vec())
        .unwrap_or_else(omarchy_shell_config_paths);

    for path in candidates {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        let Some(layout) = root
            .pointer("/bar/layout")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };

        for section in ["left", "center", "right"] {
            let Some(entries) = layout.get(section).and_then(serde_json::Value::as_array) else {
                continue;
            };
            if let Some(value) = entries.iter().find_map(|entry| {
                (entry.get("id").and_then(serde_json::Value::as_str) == Some(widget_id))
                    .then(|| entry.get(setting_name).and_then(serde_json::Value::as_str))
                    .flatten()
            }) {
                return Some(value.to_string());
            }
        }
    }

    None
}

pub fn load_omarchy_shell_clock_format(paths: Option<&[PathBuf]>) -> Option<String> {
    load_omarchy_shell_widget_setting(paths, "omarchy.clock", "format")
}

pub fn load_omarchy_shell_weather_unit(paths: Option<&[PathBuf]>) -> Option<String> {
    let unit = load_omarchy_shell_widget_setting(paths, "omarchy.weather", "unit")?
        .trim()
        .to_ascii_lowercase();
    matches!(unit.as_str(), "metric" | "imperial").then_some(unit)
}

fn omarchy_weather_location_paths() -> Vec<PathBuf> {
    vec![home_dir().join(".local/state/omarchy/settings/weather.json")]
}

fn json_coordinate(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
    })
}

fn load_omarchy_weather_coordinate(paths: Option<&[PathBuf]>) -> Option<(f64, f64)> {
    let candidates = paths
        .map(|paths| paths.to_vec())
        .unwrap_or_else(omarchy_weather_location_paths);

    for path in candidates {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        let latitude = json_coordinate(root.get("latitude"));
        let longitude = json_coordinate(root.get("longitude"));
        if let (Some(latitude), Some(longitude)) = sanitize_place_coordinate(latitude, longitude) {
            return Some((latitude, longitude));
        }
    }

    None
}

fn zone_tab_paths() -> Vec<PathBuf> {
    zoneinfo_roots()
        .into_iter()
        .flat_map(|root| [root.join("zone.tab"), root.join("zone1970.tab")])
        .collect()
}

fn weather_unit_for_timezone(timezone: &str, paths: Option<&[PathBuf]>) -> Option<String> {
    let timezone = canonical_timezone_name(timezone);
    let candidates = paths
        .map(|paths| paths.to_vec())
        .unwrap_or_else(zone_tab_paths);

    for path in candidates {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let Some(country_codes) = contents.lines().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let mut columns = trimmed.split('\t');
            let country_codes = columns.next()?;
            let _coordinate = columns.next()?;
            let timezone_name = columns.next()?;
            (timezone_name == timezone).then_some(country_codes)
        }) else {
            continue;
        };

        let imperial = country_codes
            .split(',')
            .any(|code| matches!(code.trim(), "US" | "LR" | "MM"));
        return Some(if imperial { "imperial" } else { "metric" }.to_string());
    }

    None
}

pub fn resolve_omarchy_weather_unit(
    shell_paths: Option<&[PathBuf]>,
    location_paths: Option<&[PathBuf]>,
    zone_paths: Option<&[PathBuf]>,
    local_timezone: &str,
) -> Option<String> {
    load_omarchy_shell_weather_unit(shell_paths)
        .or_else(|| {
            let (latitude, longitude) = load_omarchy_weather_coordinate(location_paths)?;
            let timezone = timezone_at(latitude, longitude)?;
            weather_unit_for_timezone(&timezone, zone_paths)
        })
        .or_else(|| weather_unit_for_timezone(local_timezone, zone_paths))
}

fn infer_time_format_inner(clock_format: &str) -> Option<&'static str> {
    if ["%I", "%l", "%p", "%P", "%r"]
        .iter()
        .any(|token| clock_format.contains(token))
    {
        return Some("ampm");
    }
    if clock_format.contains("AP") || clock_format.contains("ap") {
        return Some("ampm");
    }
    if ["%H", "%k", "%R", "%T"]
        .iter()
        .any(|token| clock_format.contains(token))
    {
        return Some("24h");
    }
    static QT_HOUR_TOKEN: OnceLock<Regex> = OnceLock::new();
    let qt_hour_token = QT_HOUR_TOKEN.get_or_init(|| {
        Regex::new(r"(^|[^A-Za-z])(?:HH|H|hh|h)([^A-Za-z]|$)").expect("valid Qt hour token regex")
    });
    if qt_hour_token.is_match(clock_format) {
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
    if let Some(clock_format) = load_omarchy_shell_clock_format(paths) {
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

pub fn system_time_format() -> String {
    detect_system_time_format_with_paths(None)
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
        let mut seen_locations = HashSet::new();
        let mut seen_alias_timezones = HashSet::new();

        for (_, _, _, alias) in alias_scored {
            // tzdata links often expose both a friendly city alias ("Rosario")
            // and its legacy identifier ("America/Rosario"). They are names
            // for the same canonical place, so retain the best-scoring alias.
            // Remote place results use their own coordinates and are not
            // affected, preserving distinct cities that share a timezone.
            if !seen_alias_timezones.insert(alias.timezone.clone())
                || !seen_locations.insert((alias.timezone.clone(), Self::normalize(&alias.alias)))
            {
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
                // A tzdb link shares clock rules with its canonical zone, not
                // necessarily geography. Pacific/Johnston, for example,
                // resolves to Pacific/Honolulu but must not be plotted or
                // persisted at Honolulu's representative coordinate.
                latitude: alias.latitude,
                longitude: alias.longitude,
                open_meteo_attribution: false,
            });
            if results.len() >= limit {
                return results;
            }
        }

        for (_, _, _, record) in scored {
            if !seen_locations.insert((record.timezone.clone(), Self::normalize(&record.city))) {
                continue;
            }
            let abbreviation_text = if record.abbreviations.is_empty() {
                "Timezone".to_string()
            } else {
                record.abbreviations.join(" / ")
            };
            let (latitude, longitude) = timezone_coordinate(&record.timezone);
            results.push(TimezoneSearchResult {
                timezone: record.timezone.clone(),
                title: record.city.clone(),
                subtitle: format!("{}  ·  {}", record.timezone, abbreviation_text),
                latitude,
                longitude,
                open_meteo_attribution: false,
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
            let (latitude, longitude) = timezone_coordinate(&record.timezone);
            return Some(TimezoneSearchResult {
                timezone: record.timezone.clone(),
                title: record.city.clone(),
                subtitle: format!("{}  ·  {}", record.timezone, abbreviation_text),
                latitude,
                longitude,
                open_meteo_attribution: false,
            });
        }

        if !self.zones.contains(&canonical_timezone) {
            return None;
        }

        let (latitude, longitude) = timezone_coordinate(&canonical_timezone);
        Some(TimezoneSearchResult {
            title: friendly_timezone_name(&canonical_timezone),
            subtitle: canonical_timezone.clone(),
            timezone: canonical_timezone,
            latitude,
            longitude,
            open_meteo_attribution: false,
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

        for (alias, timezone_name) in timezone_link_aliases() {
            let coordinate = verified_colocated_timezone_link_coordinate(alias, timezone_name);
            self.add_alias_record(&mut aliases, alias, timezone_name, coordinate);

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
                    coordinate,
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
    const MAX_DISPLAY_FIELD_CHARS: usize = 256;
    const MAX_REMOTE_RESULTS: usize = 12;

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
        let Ok(payload) = self.fetch_payload(query, Self::ENDPOINT) else {
            return Vec::new();
        };

        self.results_from_payload(payload)
    }

    fn fetch_payload(&self, query: &str, endpoint: &str) -> anyhow::Result<RemotePlaceResponse> {
        let client = open_meteo_client(Duration::from_secs_f64(self.timeout))
            .context("could not initialize the Open-Meteo geocoding client")?;

        let requested_count = Self::MAX_REMOTE_RESULTS.to_string();
        let response = client
            .get(endpoint)
            .query(&[
                ("name", query),
                ("count", requested_count.as_str()),
                ("format", "json"),
            ])
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, "omarchy-world-clock/1.0")
            .send()
            .context("could not reach Open-Meteo geocoding")?;
        if !response.status().is_success() {
            anyhow::bail!(
                "Open-Meteo geocoding returned HTTP status {}",
                response.status()
            );
        }

        read_json_response::<RemotePlaceResponse>(response, MAX_OPEN_METEO_RESPONSE_BYTES)
            .context("could not read the Open-Meteo geocoding response")
    }

    fn results_from_payload(&self, payload: RemotePlaceResponse) -> Vec<TimezoneSearchResult> {
        let mut results = Vec::new();
        let mut seen_locations = HashSet::new();
        for item in payload.results.unwrap_or_default() {
            let Some(raw_timezone) = item.timezone.as_deref() else {
                continue;
            };

            let timezone_name = canonical_timezone_name(raw_timezone.trim());
            if !self.zones.contains(&timezone_name) {
                continue;
            }

            let Some(title) = Self::format_title(&item) else {
                continue;
            };
            if !seen_locations.insert((timezone_name.clone(), TimezoneResolver::normalize(&title)))
            {
                continue;
            }

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
                open_meteo_attribution: true,
            });
            if results.len() == Self::MAX_REMOTE_RESULTS {
                break;
            }
        }

        results
    }

    fn format_title(item: &RemotePlaceResult) -> Option<String> {
        let values = [
            item.name.as_deref(),
            item.admin1.as_deref(),
            item.country.as_deref(),
        ];
        // Remote place names can be persisted as labels and later cross into
        // shell-owned text renderers, so reject markup at the trust boundary.
        if values
            .iter()
            .flatten()
            .any(|value| !Self::display_field_is_safe(value))
        {
            return None;
        }

        let parts = Self::unique_parts(values);
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }

    fn display_field_is_safe(value: &str) -> bool {
        let mut char_count = 0;
        for character in value.chars() {
            char_count += 1;
            if char_count > Self::MAX_DISPLAY_FIELD_CHARS
                || character.is_control()
                || matches!(character, '<' | '>')
            {
                return false;
            }
        }
        true
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
        canonical_timezone_name, detect_system_time_format_with_paths,
        load_omarchy_shell_weather_unit, merge_zone_tab_coordinates, parse_zone_tab_coordinate,
        AppConfig, ConfigManager, LocationKey, RemotePlaceResponse, RemotePlaceResult,
        RemotePlaceSearch, TimezoneEntry, TimezoneResolver,
    };
    use crate::remote_response::{
        serve_http_redirect_to_response, serve_http_response_without_length,
        MAX_OPEN_METEO_RESPONSE_BYTES,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    fn manager_in(temp_dir: &TempDir) -> ConfigManager {
        ConfigManager::new(Some(temp_dir.path().join("config.json")))
    }

    #[test]
    fn config_round_trips_and_inserts_local_timezone() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        let loaded = manager.load_with_local_timezone("UTC").unwrap();
        let utc = canonical_timezone_name("UTC");

        assert_eq!(
            loaded,
            AppConfig {
                timezones: vec![TimezoneEntry {
                    id: 1,
                    timezone: utc,
                    place: "UTC".to_string(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                }],
                pinned_locations: vec![],
                disable_open_meteo_geolocation: false,
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
        let utc = canonical_timezone_name("UTC");

        assert_eq!(
            loaded.timezones,
            vec![
                TimezoneEntry {
                    id: 1,
                    timezone: utc,
                    place: String::new(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                },
                TimezoneEntry {
                    id: 2,
                    timezone: "Asia/Tokyo".to_string(),
                    place: String::new(),
                    label: String::new(),
                    latitude: None,
                    longitude: None,
                },
            ]
        );
    }

    #[test]
    fn config_loads_open_meteo_geolocation_opt_out() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            "{\"timezones\": [\"UTC\"], \"disable_open_meteo_geolocation\": true}\n",
        )
        .unwrap();

        let manager = ConfigManager::new(Some(path));
        let loaded = manager.load_with_local_timezone("UTC").unwrap();

        assert!(loaded.disable_open_meteo_geolocation);
    }

    #[test]
    fn malformed_config_is_preserved_instead_of_resetting_locations() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        let partial_config = "{\n  \"version\": 4,\n  \"timezones\": [\n";
        fs::write(&path, partial_config).unwrap();

        let manager = ConfigManager::new(Some(path.clone()));
        let error = manager.load_with_local_timezone("UTC").unwrap_err();

        assert!(error.to_string().contains("failed to parse"));
        assert_eq!(fs::read_to_string(path).unwrap(), partial_config);
    }

    #[cfg(unix)]
    #[test]
    fn canonical_config_load_does_not_require_write_access() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        let manager = ConfigManager::new(Some(path.clone()));
        let expected = AppConfig {
            timezones: vec![TimezoneEntry {
                id: 1,
                timezone: canonical_timezone_name("UTC"),
                place: "UTC".to_string(),
                label: String::new(),
                latitude: None,
                longitude: None,
            }],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        fs::write(&path, manager.serialize(&expected).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        fs::set_permissions(temp_dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

        let loaded = manager.load_with_local_timezone("UTC").unwrap();

        fs::set_permissions(temp_dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(loaded, expected);
    }

    #[test]
    fn config_rewrites_legacy_row_and_time_format_settings() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "version": 3,
  "timezones": [
    {
      "timezone": "UTC",
      "label": "Home",
      "locked": true
    },
    {
      "timezone": "Asia/Tokyo",
      "label": "Tokyo",
      "locked": false
    }
  ],
  "sort_mode": "time",
  "time_format": "ampm"
}
"#,
        )
        .unwrap();

        let manager = ConfigManager::new(Some(path.clone()));
        let loaded = manager.load_with_local_timezone("UTC").unwrap();
        let rewritten = fs::read_to_string(path).unwrap();

        assert_eq!(loaded.timezones.len(), 2);
        assert!(!rewritten.contains("\"locked\""));
        assert!(!rewritten.contains("\"sort_mode\""));
        assert!(!rewritten.contains("\"time_format\""));
        assert!(rewritten.contains("\"version\": 8"));
    }

    #[test]
    fn detects_system_time_format_from_omarchy_shell_clock() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("shell.json");
        fs::write(
            &path,
            r#"{
  "bar": {
    "layout": {
      "center": [
        { "id": "omarchy.clock", "format": "dddd h:mm AP" }
      ]
    }
  }
}
"#,
        )
        .unwrap();

        assert_eq!(
            detect_system_time_format_with_paths(Some(&[path])),
            "ampm".to_string()
        );
    }

    #[test]
    fn reads_only_explicit_omarchy_weather_units() {
        let temp_dir = TempDir::new().unwrap();
        let metric_path = temp_dir.path().join("metric-shell.json");
        let automatic_path = temp_dir.path().join("automatic-shell.json");
        fs::write(
            &metric_path,
            r#"{
  "bar": {
    "layout": {
      "right": [
        { "id": "omarchy.weather", "unit": " Metric " }
      ]
    }
  }
}
"#,
        )
        .unwrap();
        fs::write(
            &automatic_path,
            r#"{
  "bar": {
    "layout": {
      "right": [
        { "id": "omarchy.weather", "unit": "auto" }
      ]
    }
  }
}
"#,
        )
        .unwrap();

        assert_eq!(
            load_omarchy_shell_weather_unit(Some(&[metric_path])),
            Some("metric".to_string())
        );
        assert_eq!(
            load_omarchy_shell_weather_unit(Some(&[automatic_path])),
            None
        );
    }

    #[test]
    fn canonicalizes_alias_when_system_tzdata_exposes_it() {
        let canonical = canonical_timezone_name("Asia/Calcutta");
        assert!(!canonical.is_empty());
    }

    #[test]
    fn local_search_results_include_coordinates_for_globe_focus() {
        let resolver = TimezoneResolver::new(Some(vec!["Asia/Tokyo".to_string()]));

        let result = resolver
            .search("Tokyo", 1)
            .into_iter()
            .next()
            .expect("Tokyo should resolve locally");

        assert!(result.latitude.is_some());
        assert!(result.longitude.is_some());
    }

    #[test]
    fn local_search_collapses_legacy_aliases_for_the_same_place() {
        let canonical = canonical_timezone_name("America/Rosario");
        let resolver = TimezoneResolver::new(Some(vec![canonical.clone()]));

        let results = resolver.search("Rosario", 8);
        let matching = results
            .iter()
            .filter(|result| result.timezone == canonical)
            .collect::<Vec<_>>();

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].title, "Rosario");
        assert_eq!(matching[0].latitude, None);
        assert_eq!(matching[0].longitude, None);
    }

    #[test]
    fn location_alias_does_not_inherit_the_canonical_zones_coordinates() {
        let canonical = canonical_timezone_name("Pacific/Johnston");
        let resolver = TimezoneResolver::new(Some(vec![canonical.clone()]));

        let result = resolver
            .search("Johnston", 8)
            .into_iter()
            .find(|result| result.timezone == canonical)
            .expect("Johnston alias should remain searchable");

        assert_eq!(result.title, "Johnston");
        assert_eq!(result.latitude, None);
        assert_eq!(result.longitude, None);
    }

    #[test]
    fn timezone_link_uses_its_own_zone_tab_coordinate_when_available() {
        let canonical = canonical_timezone_name("Pacific/Saipan");
        let resolver = TimezoneResolver::new(Some(vec![canonical.clone()]));

        let result = resolver
            .search("Saipan", 8)
            .into_iter()
            .find(|result| result.timezone == canonical)
            .expect("Saipan alias should remain searchable");

        assert_eq!(result.title, "Saipan");
        assert!(result.latitude.is_some());
        assert!(result.longitude.is_some());
    }

    #[test]
    fn colocated_city_rename_alias_keeps_coordinates_for_globe_focus() {
        let canonical = canonical_timezone_name("Asia/Saigon");
        let resolver = TimezoneResolver::new(Some(vec![canonical.clone()]));

        let result = resolver
            .search("Saigon", 8)
            .into_iter()
            .find(|result| result.timezone == canonical)
            .expect("Saigon alias should remain searchable");

        assert_eq!(result.title, "Saigon");
        assert!(result.latitude.is_some());
        assert!(result.longitude.is_some());
    }

    #[test]
    fn exact_place_duplicates_receive_distinct_stable_ids() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let updated = manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();
        let duplicated = manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();

        assert_eq!(updated.timezones.len(), 2);
        assert_eq!(duplicated.timezones.len(), 3);
        assert_eq!(duplicated.timezones[1].place, "Tokyo");
        assert_eq!(duplicated.timezones[2].place, "Tokyo");
        assert_ne!(duplicated.timezones[1].id, duplicated.timezones[2].id);
    }

    #[test]
    fn exact_duplicates_with_equal_personal_labels_remain_addressable_by_id() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        manager
            .add_location_with_coordinate(
                "America/New_York",
                "Boston",
                "Office",
                Some(42.3601),
                Some(-71.0589),
            )
            .unwrap();
        let duplicated = manager
            .add_location_with_coordinate(
                "America/New_York",
                "Boston",
                "Office",
                Some(42.3601),
                Some(-71.0589),
            )
            .unwrap()
            .config;
        let first_id = duplicated.timezones[1].id;
        let second_id = duplicated.timezones[2].id;

        manager.pin_location_by_id(first_id).unwrap();
        manager.pin_location_by_id(second_id).unwrap();
        let renamed = manager.rename_location_by_id(first_id, "Sister").unwrap();
        assert_eq!(renamed.timezones[1].label, "Sister");
        assert_eq!(renamed.timezones[2].label, "Office");
        assert_eq!(renamed.timezones[1].place, "Boston");
        assert_eq!(renamed.timezones[2].place, "Boston");
        assert_eq!(renamed.pinned_locations.len(), 2);

        let removed = manager.remove_location_by_id(first_id).unwrap();
        assert!(!removed.timezones.iter().any(|entry| entry.id == first_id));
        assert!(removed.timezones.iter().any(|entry| entry.id == second_id));
        assert_eq!(
            removed.pinned_locations,
            vec![removed.timezones[1].location_key()]
        );
    }

    #[test]
    fn distinct_places_can_share_a_timezone() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        manager
            .add_timezone("America/New_York", "New York")
            .unwrap();
        let updated = manager
            .add_timezone("America/New_York", "Boston, Massachusetts, United States")
            .unwrap();

        assert_eq!(updated.timezones.len(), 3);
        assert_eq!(updated.timezones[1].place, "New York");
        assert_eq!(
            updated.timezones[2].place,
            "Boston, Massachusetts, United States"
        );
    }

    #[test]
    fn rename_location_preserves_coordinates_and_pinned_identity() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager
            .add_timezone_with_coordinate("Asia/Tokyo", "Tokyo", Some(35.6764), Some(139.65))
            .unwrap();
        manager.pin_location("Asia/Tokyo", Some("Tokyo")).unwrap();
        manager.add_timezone("Europe/Paris", "Rennes").unwrap();
        manager
            .pin_location("Europe/Paris", Some("Rennes"))
            .unwrap();

        let renamed = manager
            .rename_location("Asia/Tokyo", "Tokyo", "  Akiko  ")
            .unwrap();

        assert_eq!(renamed.timezones[1].label, "Akiko");
        assert_eq!(renamed.timezones[1].latitude, Some(35.6764));
        assert_eq!(renamed.timezones[1].longitude, Some(139.65));
        assert_eq!(
            renamed.pinned_locations,
            vec![
                LocationKey {
                    id: 2,
                    timezone: String::new(),
                    label: String::new(),
                },
                LocationKey {
                    id: 3,
                    timezone: String::new(),
                    label: String::new(),
                },
            ]
        );
        assert_eq!(manager.load_with_local_timezone("UTC").unwrap(), renamed);
    }

    #[test]
    fn custom_labels_can_match_without_merging_cards() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager
            .add_timezone("America/New_York", "New York")
            .unwrap();
        manager.add_timezone("America/New_York", "Boston").unwrap();

        manager
            .rename_location("America/New_York", "New York", "Office")
            .unwrap();
        let renamed = manager
            .rename_location("America/New_York", "Boston", "Office")
            .unwrap();

        assert_eq!(renamed.timezones[1].label, "Office");
        assert_eq!(renamed.timezones[2].label, "Office");
        assert_ne!(renamed.timezones[1].id, renamed.timezones[2].id);
        let saved = manager.load_with_local_timezone("UTC").unwrap();
        assert_eq!(saved.timezones[1].place, "New York");
        assert_eq!(saved.timezones[2].place, "Boston");
    }

    #[test]
    fn rename_location_can_restore_the_friendly_timezone_name() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();
        manager
            .rename_location("Asia/Tokyo", "Tokyo", "Akiko")
            .unwrap();

        let renamed = manager
            .rename_location("Asia/Tokyo", "Akiko", "   ")
            .unwrap();

        assert_eq!(renamed.timezones[1].label, "");
        assert_eq!(renamed.timezones[1].display_label(), "Tokyo");
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

        assert!(updated.added);
        assert_eq!(updated.config.timezones[1].place, "Austin");
        assert_eq!(updated.config.timezones[1].latitude, Some(30.2672));
        assert_eq!(updated.config.timezones[1].longitude, Some(-97.7431));

        let duplicate = manager
            .add_timezone_with_coordinate(
                "America/Chicago",
                "Austin",
                Some(30.2672),
                Some(-97.7431),
            )
            .unwrap();
        assert!(duplicate.added);

        let loaded = manager.load_with_local_timezone("UTC").unwrap();
        assert_eq!(loaded.timezones[1].latitude, Some(30.2672));
        assert_eq!(loaded.timezones[1].longitude, Some(-97.7431));
        assert_eq!(loaded.timezones.len(), 3);
        assert_ne!(loaded.timezones[1].id, loaded.timezones[2].id);
    }

    #[test]
    fn add_timezone_has_no_product_card_limit() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let locations = [
            ("America/Vancouver", "Vancouver"),
            ("America/Denver", "Denver"),
            ("America/Chicago", "Chicago"),
            ("America/New_York", "New York"),
            ("Europe/London", "London"),
            ("Europe/Paris", "Paris"),
            ("Asia/Kolkata", "New Delhi"),
            ("Asia/Tokyo", "Tokyo"),
            ("Australia/Sydney", "Sydney"),
        ];
        for (timezone, label) in locations {
            manager
                .add_timezone_with_coordinate_for_local(timezone, label, None, None, "UTC")
                .unwrap();
        }

        let outcome = manager
            .add_timezone_with_coordinate_for_local(
                "Pacific/Auckland",
                "Auckland",
                None,
                None,
                "UTC",
            )
            .unwrap();
        assert!(outcome.added);

        let saved = manager.load_with_local_timezone("UTC").unwrap();
        assert_eq!(saved.timezones.len(), locations.len() + 2);
        assert!(saved
            .timezones
            .iter()
            .any(|entry| entry.place == "Auckland"));
    }

    #[test]
    fn config_mutation_lock_serializes_read_modify_write_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();

        let first_manager = manager.clone();
        let (first_entered_tx, first_entered_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let first = thread::spawn(move || {
            first_manager
                .mutate_with_local_timezone("UTC", move |config| {
                    first_entered_tx.send(()).unwrap();
                    release_first_rx.recv().unwrap();
                    config.timezones.push(TimezoneEntry {
                        id: 0,
                        timezone: "Asia/Tokyo".to_string(),
                        place: "Tokyo".to_string(),
                        label: String::new(),
                        latitude: None,
                        longitude: None,
                    });
                    Ok(true)
                })
                .unwrap();
        });
        first_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        let second_manager = manager.clone();
        let (second_entered_tx, second_entered_rx) = mpsc::channel();
        let second = thread::spawn(move || {
            second_manager
                .mutate_with_local_timezone("UTC", move |config| {
                    second_entered_tx.send(()).unwrap();
                    config.timezones.push(TimezoneEntry {
                        id: 0,
                        timezone: "Europe/Paris".to_string(),
                        place: "Paris".to_string(),
                        label: String::new(),
                        latitude: None,
                        longitude: None,
                    });
                    Ok(true)
                })
                .unwrap();
        });

        assert!(
            second_entered_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err(),
            "the second mutation entered while the first held the config lock"
        );
        release_first_tx.send(()).unwrap();
        first.join().unwrap();
        second_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        second.join().unwrap();

        let saved = manager.load_with_local_timezone("UTC").unwrap();
        assert!(saved.timezones.iter().any(|entry| entry.place == "Tokyo"));
        assert!(saved.timezones.iter().any(|entry| entry.place == "Paris"));
    }

    #[test]
    fn zone_tab_coordinates_parse_minutes_and_seconds() {
        let cancun = parse_zone_tab_coordinate("+2105-08646").unwrap();
        assert!((cancun.0 - 21.0833).abs() < 0.001);
        assert!((cancun.1 + 86.7667).abs() < 0.001);

        let new_york = parse_zone_tab_coordinate("+404251-0740023").unwrap();
        assert!((new_york.0 - 40.7142).abs() < 0.001);
        assert!((new_york.1 + 74.0064).abs() < 0.001);
    }

    #[test]
    fn remote_place_titles_reject_unsafe_display_fields() {
        let safe = RemotePlaceResult {
            timezone: Some("Africa/Sao_Tome".to_string()),
            name: Some("São Tomé & Príncipe".to_string()),
            admin1: Some("Água Grande".to_string()),
            country: Some("São Tomé and Príncipe".to_string()),
            latitude: Some(0.3365),
            longitude: Some(6.7273),
        };
        assert_eq!(
            RemotePlaceSearch::format_title(&safe).as_deref(),
            Some("São Tomé & Príncipe, Água Grande, São Tomé and Príncipe")
        );

        let unsafe_fields = [
            (
                Some("<img src='https://example.invalid/probe'>"),
                None,
                None,
            ),
            (Some("Paris"), Some("Île\0de-France"), Some("France")),
            (Some("Paris"), None, Some("France > Europe")),
        ];
        for (name, admin1, country) in unsafe_fields {
            let result = RemotePlaceResult {
                timezone: Some("Europe/Paris".to_string()),
                name: name.map(str::to_string),
                admin1: admin1.map(str::to_string),
                country: country.map(str::to_string),
                latitude: Some(48.8566),
                longitude: Some(2.3522),
            };
            assert_eq!(RemotePlaceSearch::format_title(&result), None);
        }

        let oversized = RemotePlaceResult {
            timezone: Some("Europe/Paris".to_string()),
            name: Some("x".repeat(257)),
            admin1: None,
            country: Some("France".to_string()),
            latitude: Some(48.8566),
            longitude: Some(2.3522),
        };
        assert_eq!(RemotePlaceSearch::format_title(&oversized), None);
    }

    #[test]
    fn remote_place_payload_retains_at_most_the_requested_result_count() {
        let search = RemotePlaceSearch::new(Some(vec!["Europe/Paris".to_string()]), None);
        let payload = RemotePlaceResponse {
            results: Some(
                (0..20)
                    .map(|index| RemotePlaceResult {
                        timezone: Some("Europe/Paris".to_string()),
                        name: Some(format!("Place {index}")),
                        admin1: None,
                        country: Some("France".to_string()),
                        latitude: Some(48.8566),
                        longitude: Some(2.3522),
                    })
                    .collect(),
            ),
        };

        let results = search.results_from_payload(payload);

        assert_eq!(results.len(), RemotePlaceSearch::MAX_REMOTE_RESULTS);
        assert_eq!(results.last().unwrap().title, "Place 11, France");
    }

    #[test]
    fn remote_geocoding_rejects_an_oversized_unknown_length_response() {
        let body = format!(
            r#"{{"results":[],"padding":"{}"}}"#,
            "x".repeat(MAX_OPEN_METEO_RESPONSE_BYTES)
        )
        .into_bytes();
        let (endpoint, server) = serve_http_response_without_length(body);
        let search = RemotePlaceSearch::new(Some(vec!["Europe/Paris".to_string()]), Some(1.0));

        let error = search.fetch_payload("Paris", &endpoint).unwrap_err();
        server.join().unwrap();

        assert!(format!("{error:#}").contains("exceeds 65536-byte limit"));
    }

    #[test]
    fn remote_geocoding_does_not_follow_redirects() {
        let (endpoint, redirect_server, stop_target, target_server) =
            serve_http_redirect_to_response(br#"{"results":[]}"#.to_vec());
        let search = RemotePlaceSearch::new(Some(vec!["Europe/Paris".to_string()]), Some(1.0));

        let result = search.fetch_payload("Paris", &endpoint);
        redirect_server.join().unwrap();
        let _ = stop_target.send(());
        let target_was_requested = target_server.join().unwrap();

        let error = result.expect_err("redirect response should be rejected");
        assert!(
            !target_was_requested,
            "redirect target should not be requested"
        );
        assert!(format!("{error:#}").contains("HTTP status 302 Found"));
    }

    #[test]
    fn zone_tab_coordinates_merge_missing_entries_without_overwriting() {
        let primary = "US\t+404251-0740023\tAmerica/New_York\n";
        let fallback = "US\t+4100-07500\tAmerica/New_York\nCA\t+4916-12307\tAmerica/Vancouver\n";
        let mut coordinates = BTreeMap::new();

        merge_zone_tab_coordinates(&mut coordinates, primary);
        merge_zone_tab_coordinates(&mut coordinates, fallback);

        let new_york = coordinates.get("America/New_York").unwrap();
        assert!((new_york.0 - 40.7142).abs() < 0.001);
        assert!((new_york.1 + 74.0064).abs() < 0.001);

        let vancouver = coordinates.get("America/Vancouver").unwrap();
        assert!((vancouver.0 - 49.2667).abs() < 0.001);
        assert!((vancouver.1 + 123.1167).abs() < 0.001);
    }

    #[test]
    fn remove_timezone_persists_change() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();

        let updated = manager.remove_timezone("Asia/Tokyo").unwrap();
        assert_eq!(updated.timezones.len(), 1);
        assert_eq!(
            updated.timezones[0].timezone,
            canonical_timezone_name("UTC")
        );

        let error = manager.remove_timezone("UTC").unwrap_err();
        assert!(error.to_string().contains("keep at least one timezone"));
    }

    #[test]
    fn pinned_locations_round_trip_and_can_be_removed_individually() {
        let temp_dir = TempDir::new().unwrap();
        let manager = manager_in(&temp_dir);
        manager.load_with_local_timezone("UTC").unwrap();
        manager.add_timezone("Asia/Tokyo", "Tokyo").unwrap();
        manager.add_timezone("Europe/Paris", "Rennes").unwrap();

        manager.pin_location("Asia/Tokyo", Some("Tokyo")).unwrap();
        let duplicate = manager.pin_location("Asia/Tokyo", Some("Tokyo")).unwrap();
        assert_eq!(duplicate.pinned_locations.len(), 1);
        let pinned = manager
            .pin_location("Europe/Paris", Some("Rennes"))
            .unwrap();
        assert_eq!(
            pinned.pinned_locations,
            vec![
                LocationKey {
                    id: 2,
                    timezone: String::new(),
                    label: String::new(),
                },
                LocationKey {
                    id: 3,
                    timezone: String::new(),
                    label: String::new(),
                },
            ]
        );
        assert_eq!(manager.load_with_local_timezone("UTC").unwrap(), pinned);

        let unpinned = manager.unpin_location("Asia/Tokyo", Some("Tokyo")).unwrap();
        assert_eq!(
            unpinned.pinned_locations,
            vec![LocationKey {
                id: 3,
                timezone: String::new(),
                label: String::new(),
            }]
        );

        let cleared = manager.clear_pinned_locations().unwrap();
        assert!(cleared.pinned_locations.is_empty());
    }

    #[test]
    fn singular_v6_pin_migrates_to_the_ordered_pin_list() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "version": 6,
  "pinned_location": { "timezone": "Asia/Tokyo", "label": "Tokyo" },
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    { "timezone": "Asia/Tokyo", "label": "Tokyo" }
  ]
}
"#,
        )
        .unwrap();
        let manager = ConfigManager::new(Some(path.clone()));

        let migrated = manager.load_with_local_timezone("UTC").unwrap();

        assert_eq!(
            migrated.pinned_locations,
            vec![LocationKey {
                id: 2,
                timezone: String::new(),
                label: String::new(),
            }]
        );
        let stored = fs::read_to_string(path).unwrap();
        assert!(stored.contains("\"version\": 8"));
        assert!(stored.contains("\"pinned_locations\""));
        assert!(stored.contains("\"id\": 2"));
        assert!(!stored.contains("\"pinned_location\":"));
    }

    #[test]
    fn v7_overloaded_labels_migrate_to_places_without_guessing_their_meaning() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "version": 7,
  "pinned_locations": [
    { "timezone": "America/New_York", "label": "Sister" }
  ],
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    {
      "timezone": "America/New_York",
      "label": "Sister",
      "latitude": 42.3601,
      "longitude": -71.0589
    }
  ]
}
"#,
        )
        .unwrap();
        let manager = ConfigManager::new(Some(path.clone()));

        let migrated = manager.load_with_local_timezone("UTC").unwrap();

        assert_eq!(migrated.timezones[0].place, "Home");
        assert_eq!(migrated.timezones[0].label, "");
        assert_eq!(migrated.timezones[1].place, "Sister");
        assert_eq!(migrated.timezones[1].label, "");
        assert_eq!(
            migrated.pinned_locations,
            vec![migrated.timezones[1].location_key()]
        );
        let stored = fs::read_to_string(path).unwrap();
        assert!(stored.contains("\"version\": 8"));
        assert!(stored.contains("\"place\": \"Sister\""));
    }

    #[test]
    fn removing_or_normalizing_a_missing_timezone_clears_the_pin() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "version": 5,
  "pinned_timezone": "Asia/Tokyo",
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    { "timezone": "Asia/Tokyo", "label": "Tokyo" }
  ]
}
"#,
        )
        .unwrap();
        let manager = ConfigManager::new(Some(path));

        let removed = manager.remove_timezone("Asia/Tokyo").unwrap();
        assert!(removed.pinned_locations.is_empty());
    }
}
