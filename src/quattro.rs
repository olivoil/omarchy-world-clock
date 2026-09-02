use crate::config::{
    canonical_timezone_name, is_valid_timezone, place_coordinate, AppConfig, PinnedLocationIndex,
    TimezoneEntry, TimezoneSearchResult,
};
use crate::time::{
    format_display_time, format_timezone_notation, friendly_timezone_name, parse_timezone,
    zoned_datetime,
};
use crate::timezone_grid::timezone_at;
use chrono::{DateTime, Duration, LocalResult, Offset, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

pub const SNAPSHOT_SCHEMA_VERSION: u64 = 1;
pub const BACKEND_PROTOCOL_VERSION: u64 = 5;
pub const SCRUB_STEP_MINUTES: u32 = 15;
const MODULE_TOOLTIP_LOCATION_LIMIT: usize = 12;

#[derive(Debug, Deserialize)]
struct FeaturedCity {
    title: String,
    latitude: f64,
    longitude: f64,
    minimum_zoom: f64,
    source_timezone: Option<String>,
}

fn featured_city_catalog() -> &'static [FeaturedCity] {
    static CITIES: OnceLock<Vec<FeaturedCity>> = OnceLock::new();
    CITIES.get_or_init(|| {
        serde_json::from_str(include_str!("../data/featured-cities.json"))
            .expect("bundled featured-city catalogue should be valid")
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroPinnedClock {
    pub code: String,
    pub label: String,
    pub time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroModulePayload {
    pub protocol_version: u64,
    pub backend_version: &'static str,
    pub tooltip: String,
    pub pinned_clocks: Vec<QuattroPinnedClock>,
}

fn short_code(value: &str) -> String {
    let words = value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let characters = if words.len() > 1 {
        words
            .iter()
            .filter_map(|word| word.chars().next())
            .collect::<Vec<_>>()
    } else {
        words
            .first()
            .into_iter()
            .flat_map(|word| word.chars())
            .collect::<Vec<_>>()
    };
    characters
        .into_iter()
        .flat_map(char::to_uppercase)
        .take(3)
        .collect()
}

fn location_short_code(entry: &TimezoneEntry) -> String {
    let label_code = short_code(&entry.read_card_title());
    if !label_code.is_empty() {
        return label_code;
    }
    let timezone_code = short_code(entry.timezone.rsplit('/').next().unwrap_or(&entry.timezone));
    if timezone_code.is_empty() {
        "TZ".to_string()
    } else {
        timezone_code
    }
}

fn pad_right(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.chars().count());
    format!("{value}{}", " ".repeat(padding))
}

fn pad_left(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.chars().count());
    format!("{}{value}", " ".repeat(padding))
}

fn format_tooltip_clock_rows(rows: &[(String, String)]) -> String {
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
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_module_payload(
    config: &AppConfig,
    now: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroModulePayload {
    let pinned_locations = config.pinned_location_index();
    let (_, visible_entries) = visible_location_entries(config, now, local_timezone);
    let additional_location_count = visible_entries.len().saturating_sub(1);
    let tooltip_rows = visible_entries
        .into_iter()
        .skip(1)
        .take(MODULE_TOOLTIP_LOCATION_LIMIT)
        .map(|entry| {
            let time = format_display_time(&zoned_datetime(now, &entry.timezone), time_format);
            (entry.read_card_title(), time)
        })
        .collect::<Vec<_>>();
    let hidden_location_count = additional_location_count.saturating_sub(tooltip_rows.len());
    let tooltip = if tooltip_rows.is_empty() {
        "No additional timezones yet.".to_string()
    } else {
        let mut value = format_tooltip_clock_rows(&tooltip_rows);
        if hidden_location_count > 0 {
            let noun = if hidden_location_count == 1 {
                "location"
            } else {
                "locations"
            };
            value.push_str(&format!("\n+{hidden_location_count} more {noun}"));
        }
        value
    };
    QuattroModulePayload {
        protocol_version: BACKEND_PROTOCOL_VERSION,
        backend_version: env!("CARGO_PKG_VERSION"),
        tooltip,
        pinned_clocks: pinned_locations
            .pinned_entries()
            .map(|entry| QuattroPinnedClock {
                code: location_short_code(entry),
                label: entry.read_card_title(),
                time: format_display_time(&zoned_datetime(now, &entry.timezone), time_format),
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuattroClock {
    pub timezone: String,
    pub label: String,
    pub title: String,
    pub time: String,
    pub date: String,
    pub day: String,
    pub notation: String,
    pub local_minutes: u32,
    pub utc_offset_seconds: i32,
    pub relative_minutes: i64,
    pub relative_label: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroScrubFrame {
    pub day_offset: i64,
    pub minute: u32,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_utc: Option<String>,
    pub ambiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroScrubZoneState {
    pub from_slot: usize,
    pub utc_offset_seconds: i32,
    pub notation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroScrubLocation {
    pub timezone: String,
    pub label: String,
    pub states: Vec<QuattroScrubZoneState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroScrubPayload {
    pub schema_version: u64,
    pub source_timezone: String,
    pub date: String,
    pub date_label: String,
    pub time_format: String,
    pub locations: Vec<QuattroScrubLocation>,
    pub step_minutes: u32,
    pub first_day_offset: i64,
    pub day_count: u32,
    pub slots: Vec<QuattroScrubFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuattroTimelineItem {
    pub relative_minutes: i64,
    pub time: String,
    pub label: String,
    pub count: usize,
    pub lane: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuattroSnapshot {
    pub schema_version: u64,
    pub reference_utc: String,
    pub local_timezone: String,
    pub time_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather_unit: Option<String>,
    pub configured_count: usize,
    pub local_configured: bool,
    pub pinned_timezone: Option<String>,
    pub summary: QuattroClock,
    pub clocks: Vec<QuattroClock>,
    pub timeline: Vec<QuattroTimelineItem>,
    pub featured_cities: Vec<QuattroMapLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuattroMapLocation {
    pub timezone: String,
    pub title: String,
    pub subtitle: String,
    pub latitude: f64,
    pub longitude: f64,
    pub minimum_zoom: f64,
    pub time: String,
    pub day: String,
    pub notation: String,
    pub relative_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuattroSearchLocation {
    #[serde(flatten)]
    pub result: TimezoneSearchResult,
    pub time: String,
    pub day: String,
    pub notation: String,
    pub relative_label: String,
}

fn wall_clock_delta_minutes(
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    timezone: &str,
) -> i64 {
    let anchor = zoned_datetime(reference_utc, local_timezone);
    let value = zoned_datetime(reference_utc, timezone);
    value
        .naive_local()
        .signed_duration_since(anchor.naive_local())
        .num_minutes()
}

fn relative_label(relative_minutes: i64) -> String {
    if relative_minutes == 0 {
        return "Same time".to_string();
    }

    let absolute = relative_minutes.abs();
    let hours = absolute / 60;
    let minutes = absolute % 60;
    let direction = if relative_minutes > 0 {
        "ahead"
    } else {
        "behind"
    };

    match (hours, minutes) {
        (0, minutes) => format!("{minutes} min {direction}"),
        (hours, 0) => format!("{hours}h {direction}"),
        (hours, minutes) => format!("{hours}h {minutes:02}m {direction}"),
    }
}

fn day_label(reference_utc: DateTime<Utc>, local_timezone: &str, timezone: &str) -> String {
    let local_date = zoned_datetime(reference_utc, local_timezone).date_naive();
    let value = zoned_datetime(reference_utc, timezone);
    let day_delta = value
        .date_naive()
        .signed_duration_since(local_date)
        .num_days();

    match day_delta {
        -1 => "Yesterday".to_string(),
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        _ => value.format("%a, %b %-d").to_string(),
    }
}

fn clock_from_entry(
    entry: &TimezoneEntry,
    pinned_locations: &PinnedLocationIndex<'_>,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroClock {
    let zoned = zoned_datetime(reference_utc, &entry.timezone);
    let relative_minutes = wall_clock_delta_minutes(reference_utc, local_timezone, &entry.timezone);
    let (latitude, longitude) = place_coordinate(entry)
        .map(|(latitude, longitude)| (Some(latitude), Some(longitude)))
        .unwrap_or((None, None));
    QuattroClock {
        timezone: entry.timezone.clone(),
        label: entry.display_label(),
        title: entry.read_card_title(),
        time: format_display_time(&zoned, time_format),
        date: zoned.format("%Y-%m-%d").to_string(),
        day: day_label(reference_utc, local_timezone, &entry.timezone),
        notation: format_timezone_notation(&zoned),
        local_minutes: zoned.hour() * 60 + zoned.minute(),
        utc_offset_seconds: zoned.offset().fix().local_minus_utc(),
        relative_minutes,
        relative_label: relative_label(relative_minutes),
        latitude,
        longitude,
        pinned: pinned_locations.contains(entry),
    }
}

pub fn build_scrub_payload(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    source_timezone: &str,
    local_timezone: &str,
    time_format: &str,
) -> Result<QuattroScrubPayload, &'static str> {
    let source = parse_timezone(source_timezone).ok_or("source timezone is invalid")?;
    let source_date = reference_utc.with_timezone(&source).date_naive();
    let (_, visible_entries) = visible_location_entries(config, reference_utc, local_timezone);
    let first_day_offset = -1;
    let day_count = 3;
    let slots_per_day = 24 * 60 / SCRUB_STEP_MINUTES;
    let mut slots = Vec::with_capacity((slots_per_day * day_count) as usize);
    let mut slot_references = Vec::with_capacity((slots_per_day * day_count) as usize);

    for day_offset in first_day_offset..first_day_offset + i64::from(day_count) {
        let slot_date = source_date
            .checked_add_signed(Duration::days(day_offset))
            .ok_or("source date is outside the supported range")?;
        for minute in (0..24 * 60).step_by(SCRUB_STEP_MINUTES as usize) {
            let local = slot_date
                .and_hms_opt(minute / 60, minute % 60, 0)
                .expect("a minute inside a day should be valid");
            let localized = source.from_local_datetime(&local);
            let (selected, ambiguous) = match localized {
                LocalResult::Single(value) => (Some(value), false),
                LocalResult::Ambiguous(first, second) => (Some(first.min(second)), true),
                LocalResult::None => (None, false),
            };
            let Some(reference) = selected.map(|value| value.with_timezone(&Utc)) else {
                slots.push(QuattroScrubFrame {
                    day_offset,
                    minute,
                    label: format!("{:02}:{:02}", minute / 60, minute % 60),
                    reference_utc: None,
                    ambiguous,
                });
                slot_references.push(None);
                continue;
            };

            slots.push(QuattroScrubFrame {
                day_offset,
                minute,
                label: format_display_time(&reference.with_timezone(&source), time_format),
                reference_utc: Some(reference.to_rfc3339()),
                ambiguous,
            });
            slot_references.push(Some(reference));
        }
    }

    // Keep the response proportional to the number of locations rather than
    // repeating every rendered clock in every one of the 288 rail frames.
    // Most zones have one state in this three-day window; a second state is
    // only emitted when their UTC offset or notation changes at a DST edge.
    let locations = visible_entries
        .iter()
        .map(|entry| {
            let timezone = parse_timezone(&entry.timezone)
                .expect("configured timezone should be validated before rendering");
            let mut states = Vec::new();
            let mut previous = None;
            for (slot_index, reference) in slot_references.iter().enumerate() {
                let Some(reference) = reference else {
                    continue;
                };
                let zoned = reference.with_timezone(&timezone);
                let state = (
                    zoned.offset().fix().local_minus_utc(),
                    format_timezone_notation(&zoned),
                );
                if previous.as_ref() == Some(&state) {
                    continue;
                }
                states.push(QuattroScrubZoneState {
                    from_slot: slot_index,
                    utc_offset_seconds: state.0,
                    notation: state.1.clone(),
                });
                previous = Some(state);
            }
            QuattroScrubLocation {
                timezone: entry.timezone.clone(),
                label: entry.display_label(),
                states,
            }
        })
        .collect();

    Ok(QuattroScrubPayload {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        source_timezone: source_timezone.to_string(),
        date: source_date.format("%Y-%m-%d").to_string(),
        date_label: source_date.format("%a, %b %-d").to_string(),
        time_format: time_format.to_string(),
        locations,
        step_minutes: SCRUB_STEP_MINUTES,
        first_day_offset,
        day_count,
        slots,
    })
}

fn local_entry(config: &AppConfig, local_timezone: &str) -> (Option<usize>, TimezoneEntry) {
    let index = config
        .timezones
        .iter()
        .position(|entry| entry.timezone == local_timezone);
    let entry = index
        .map(|index| config.timezones[index].clone())
        .unwrap_or_else(|| TimezoneEntry {
            timezone: local_timezone.to_string(),
            label: friendly_timezone_name(local_timezone),
            original_label: String::new(),
            latitude: None,
            longitude: None,
        });
    (index, entry)
}

pub(crate) fn visible_location_entries(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
) -> (bool, Vec<TimezoneEntry>) {
    let (summary_entry_index, summary_entry) = local_entry(config, local_timezone);
    let mut entries = config
        .timezones
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != summary_entry_index)
        .map(|(_, entry)| entry.clone())
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        wall_clock_delta_minutes(reference_utc, local_timezone, &left.timezone)
            .cmp(&wall_clock_delta_minutes(
                reference_utc,
                local_timezone,
                &right.timezone,
            ))
            .then_with(|| left.display_label().cmp(&right.display_label()))
    });
    let mut visible = Vec::with_capacity(entries.len() + 1);
    visible.push(summary_entry);
    visible.extend(entries);
    (summary_entry_index.is_some(), visible)
}

fn timeline_items(summary: &QuattroClock, clocks: &[QuattroClock]) -> Vec<QuattroTimelineItem> {
    let mut groups = BTreeMap::<i64, (String, Vec<String>)>::new();
    for clock in std::iter::once(summary).chain(clocks.iter()) {
        let group = groups
            .entry(clock.relative_minutes)
            .or_insert_with(|| (clock.time.clone(), Vec::new()));
        if !group.1.iter().any(|label| label == &clock.notation) {
            group.1.push(clock.notation.clone());
        }
    }

    let mut items = groups
        .into_iter()
        .map(|(relative_minutes, (time, mut labels))| {
            labels.sort();
            QuattroTimelineItem {
                relative_minutes,
                time,
                label: labels.join(" / "),
                count: labels.len(),
                lane: 0,
            }
        })
        .collect::<Vec<_>>();

    // Alternate dense offsets above and below the rail. A 110-minute gap is
    // enough for adjacent labels in the native 960px panel without recreating
    // the tall, mirrored three-lane stack this replaced.
    let mut lane_ends = [i64::MIN; 2];
    for item in &mut items {
        let lane = lane_ends
            .iter()
            .position(|last| item.relative_minutes.saturating_sub(*last) >= 110)
            .unwrap_or_else(|| {
                lane_ends
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, last)| **last)
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            });
        item.lane = lane;
        lane_ends[lane] = item.relative_minutes;
    }

    items
}

pub fn build_map_location(
    result: &TimezoneSearchResult,
    latitude: f64,
    longitude: f64,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroMapLocation {
    let zoned = zoned_datetime(reference_utc, &result.timezone);
    let relative_minutes =
        wall_clock_delta_minutes(reference_utc, local_timezone, &result.timezone);

    QuattroMapLocation {
        timezone: result.timezone.clone(),
        title: result.title.clone(),
        subtitle: result.subtitle.clone(),
        latitude,
        longitude,
        minimum_zoom: 0.0,
        time: format_display_time(&zoned, time_format),
        day: day_label(reference_utc, local_timezone, &result.timezone),
        notation: format_timezone_notation(&zoned),
        relative_label: relative_label(relative_minutes),
    }
}

pub fn build_search_location(
    result: TimezoneSearchResult,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroSearchLocation {
    let zoned = zoned_datetime(reference_utc, &result.timezone);
    let relative_minutes =
        wall_clock_delta_minutes(reference_utc, local_timezone, &result.timezone);

    QuattroSearchLocation {
        time: format_display_time(&zoned, time_format),
        day: day_label(reference_utc, local_timezone, &result.timezone),
        notation: format_timezone_notation(&zoned),
        relative_label: relative_label(relative_minutes),
        result,
    }
}

fn build_featured_cities(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> Vec<QuattroMapLocation> {
    featured_city_catalog()
        .iter()
        .filter_map(|city| {
            let timezone = timezone_at(city.latitude, city.longitude)
                .or_else(|| city.source_timezone.clone())?;
            let timezone = canonical_timezone_name(&timezone);
            if !is_valid_timezone(&timezone)
                || config
                    .timezones
                    .iter()
                    .any(|entry| entry.matches_location(&timezone, &city.title))
            {
                return None;
            }
            let result = TimezoneSearchResult {
                timezone: timezone.clone(),
                title: city.title.clone(),
                subtitle: timezone,
                latitude: Some(city.latitude),
                longitude: Some(city.longitude),
                open_meteo_attribution: false,
            };
            let mut location = build_map_location(
                &result,
                city.latitude,
                city.longitude,
                reference_utc,
                local_timezone,
                time_format,
            );
            location.minimum_zoom = city.minimum_zoom;
            Some(location)
        })
        .collect()
}

pub fn build_snapshot(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroSnapshot {
    let pinned_locations = config.pinned_location_index();
    let (local_configured, visible_entries) =
        visible_location_entries(config, reference_utc, local_timezone);
    let mut visible_entries = visible_entries.into_iter();
    let summary_entry = visible_entries
        .next()
        .expect("visible locations always include the local summary");
    let summary = clock_from_entry(
        &summary_entry,
        &pinned_locations,
        reference_utc,
        local_timezone,
        time_format,
    );
    let clocks = visible_entries
        .map(|entry| {
            clock_from_entry(
                &entry,
                &pinned_locations,
                reference_utc,
                local_timezone,
                time_format,
            )
        })
        .collect::<Vec<_>>();
    let timeline = timeline_items(&summary, &clocks);
    let featured_cities = build_featured_cities(config, reference_utc, local_timezone, time_format);

    QuattroSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        reference_utc: reference_utc.to_rfc3339(),
        local_timezone: local_timezone.to_string(),
        time_format: time_format.to_string(),
        weather_unit: None,
        configured_count: config.timezones.len(),
        local_configured,
        pinned_timezone: pinned_locations
            .first_pinned_entry()
            .map(|entry| entry.timezone.clone()),
        summary,
        clocks,
        timeline,
        featured_cities,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_module_payload, build_scrub_payload, build_snapshot};
    use crate::config::{AppConfig, LocationKey, TimezoneEntry};
    use chrono::{TimeZone, Utc};

    fn entry(timezone: &str, label: &str) -> TimezoneEntry {
        TimezoneEntry {
            timezone: timezone.to_string(),
            label: label.to_string(),
            original_label: label.to_string(),
            latitude: None,
            longitude: None,
        }
    }

    #[test]
    fn module_tooltip_lists_non_local_locations_in_popup_order() {
        let config = AppConfig {
            timezones: vec![
                entry("Europe/Paris", "Rennes, Brittany, France"),
                entry("America/Cancun", "Home"),
                entry("America/Vancouver", "Vancouver"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let payload = build_module_payload(&config, now, "America/Cancun", "24h");

        assert_eq!(payload.tooltip, "Vancouver  04:05\nRennes     13:05");
        assert!(!payload.tooltip.contains("Home"));
        assert!(!payload.tooltip.contains("World Clock"));
    }

    #[test]
    fn module_tooltip_explains_when_there_are_no_additional_locations() {
        let config = AppConfig {
            timezones: vec![entry("America/Cancun", "Home")],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let payload = build_module_payload(&config, now, "America/Cancun", "24h");

        assert_eq!(payload.tooltip, "No additional timezones yet.");
        assert!(payload.pinned_clocks.is_empty());
    }

    #[test]
    fn module_tooltip_summarizes_locations_beyond_its_bounded_rows() {
        let config = AppConfig {
            timezones: (0..15)
                .map(|index| entry("UTC", &format!("Zone {index:02}")))
                .collect(),
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let payload = build_module_payload(&config, now, "America/Cancun", "24h");
        let lines = payload.tooltip.lines().collect::<Vec<_>>();

        assert_eq!(
            lines.len(),
            13,
            "twelve clocks plus one summary stay bounded"
        );
        assert!(payload.tooltip.contains("Zone 11"));
        assert!(!payload.tooltip.contains("Zone 12"));
        assert_eq!(lines.last().copied(), Some("+3 more locations"));
    }

    #[test]
    fn module_payload_lists_every_pin_with_a_compact_location_code() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Cancun", "Home"),
                entry("America/New_York", "New York"),
                entry("Asia/Tokyo", "Tokyo"),
            ],
            pinned_locations: vec![
                LocationKey {
                    timezone: "Asia/Tokyo".to_string(),
                    label: "Tokyo".to_string(),
                },
                LocationKey {
                    timezone: "America/New_York".to_string(),
                    label: "New York".to_string(),
                },
            ],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let payload = build_module_payload(&config, now, "America/Cancun", "24h");

        assert_eq!(payload.pinned_clocks.len(), 2);
        assert_eq!(payload.pinned_clocks[0].code, "TOK");
        assert_eq!(payload.pinned_clocks[0].label, "Tokyo");
        assert_eq!(payload.pinned_clocks[0].time, "20:05");
        assert_eq!(payload.pinned_clocks[1].code, "NY");
        assert_eq!(payload.pinned_clocks[1].label, "New York");
        assert_eq!(payload.pinned_clocks[1].time, "07:05");
    }

    #[test]
    fn large_pin_lists_are_resolved_consistently_for_module_and_snapshot_payloads() {
        const LOCATION_COUNT: usize = 500;
        let config = AppConfig {
            timezones: (0..LOCATION_COUNT)
                .map(|index| entry("UTC", &format!("Zone {index:03}")))
                .collect(),
            pinned_locations: (0..LOCATION_COUNT)
                .rev()
                .map(|index| LocationKey {
                    timezone: "UTC".to_string(),
                    label: format!("Zone {index:03}"),
                })
                .collect(),
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let module = build_module_payload(&config, now, "UTC", "24h");
        let snapshot = build_snapshot(&config, now, "UTC", "24h");

        assert_eq!(module.pinned_clocks.len(), LOCATION_COUNT);
        assert_eq!(module.pinned_clocks.first().unwrap().label, "Zone 499");
        assert_eq!(module.pinned_clocks.last().unwrap().label, "Zone 000");
        assert!(snapshot.summary.pinned);
        assert_eq!(snapshot.clocks.len(), LOCATION_COUNT - 1);
        assert!(snapshot.clocks.iter().all(|clock| clock.pinned));
        assert_eq!(snapshot.pinned_timezone.as_deref(), Some("UTC"));
    }

    #[test]
    fn snapshot_sorts_clocks_and_marks_the_pin() {
        let config = AppConfig {
            timezones: vec![
                entry("Europe/Paris", "Rennes"),
                entry("America/Cancun", "Local"),
                entry("America/Vancouver", "Vancouver"),
            ],
            pinned_locations: vec![LocationKey {
                timezone: "Europe/Paris".to_string(),
                label: "Rennes".to_string(),
            }],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert_eq!(snapshot.summary.timezone, "America/Cancun");
        assert!(snapshot.local_configured);
        assert_eq!(snapshot.clocks[0].timezone, "America/Vancouver");
        assert_eq!(snapshot.clocks[1].timezone, "Europe/Paris");
        assert_eq!(snapshot.clocks[1].time, "13:05");
        assert!(snapshot.clocks[1].pinned);
        assert_eq!(snapshot.pinned_timezone.as_deref(), Some("Europe/Paris"));
    }

    #[test]
    fn snapshot_reports_day_and_half_hour_deltas() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Cancun", "Local"),
                entry("Asia/Kolkata", "Delhi"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 23, 45, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert_eq!(snapshot.clocks[0].relative_label, "10h 30m ahead");
        assert_eq!(snapshot.clocks[0].day, "Tomorrow");
        assert_eq!(snapshot.summary.date, "2026-08-11");
        assert_eq!(snapshot.summary.local_minutes, 18 * 60 + 45);
        assert_eq!(snapshot.summary.utc_offset_seconds, -18_000);
        assert_eq!(snapshot.clocks[0].utc_offset_seconds, 19_800);
    }

    #[test]
    fn scrub_payload_describes_adjacent_days_with_compact_timezone_states() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Cancun", "Local"),
                entry("Asia/Tokyo", "Tokyo"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let payload = build_scrub_payload(&config, now, "America/Cancun", "America/Cancun", "24h")
            .expect("build scrub payload");
        let eleven = &payload.slots[96 + 44];

        assert_eq!(payload.date, "2026-08-11");
        assert_eq!(payload.time_format, "24h");
        assert_eq!(payload.locations[0].timezone, "America/Cancun");
        assert_eq!(payload.locations[0].label, "Local");
        assert_eq!(payload.locations[1].timezone, "Asia/Tokyo");
        assert_eq!(payload.locations[1].label, "Tokyo");
        assert_eq!(payload.locations[0].states.len(), 1);
        assert_eq!(payload.locations[0].states[0].from_slot, 0);
        assert_eq!(payload.locations[0].states[0].utc_offset_seconds, -18_000);
        assert_eq!(payload.locations[0].states[0].notation, "EST");
        assert_eq!(payload.locations[1].states.len(), 1);
        assert_eq!(payload.locations[1].states[0].utc_offset_seconds, 32_400);
        assert_eq!(payload.locations[1].states[0].notation, "JST");
        assert_eq!(payload.step_minutes, 15);
        assert_eq!(payload.first_day_offset, -1);
        assert_eq!(payload.day_count, 3);
        assert_eq!(payload.slots.len(), 288);
        assert_eq!(payload.slots[95].day_offset, -1);
        assert_eq!(payload.slots[96].day_offset, 0);
        assert_eq!(payload.slots[192].day_offset, 1);
        assert_eq!(eleven.minute, 11 * 60);
        assert_eq!(eleven.day_offset, 0);
        assert_eq!(eleven.label, "11:00");
        assert_eq!(
            eleven.reference_utc.as_deref(),
            Some("2026-08-11T16:00:00+00:00")
        );
    }

    #[test]
    fn scrub_payload_stays_compact_for_large_location_lists() {
        let config = AppConfig {
            timezones: (0..100)
                .map(|index| entry("America/Cancun", &format!("Location {index}")))
                .collect(),
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let payload = build_scrub_payload(&config, now, "America/Cancun", "America/Cancun", "24h")
            .expect("build scrub payload");
        let encoded = serde_json::to_vec(&payload).expect("serialize scrub payload");

        assert!(
            encoded.len() < 128 * 1024,
            "100 locations should not expand every card into every scrub frame; got {} bytes",
            encoded.len()
        );
    }

    #[test]
    fn scrub_payload_marks_the_spring_clock_change_gap() {
        let config = AppConfig {
            timezones: vec![entry("America/New_York", "New York")],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 3, 8, 12, 0, 0).unwrap();

        let payload =
            build_scrub_payload(&config, now, "America/New_York", "America/New_York", "24h")
                .expect("build spring scrub payload");

        for index in 96 + 8..96 + 12 {
            assert!(payload.slots[index].reference_utc.is_none());
        }
        assert!(payload.slots[96 + 7].reference_utc.is_some());
        assert!(payload.slots[96 + 12].reference_utc.is_some());
        assert_eq!(payload.locations[0].states.len(), 2);
        assert_eq!(payload.locations[0].states[0].notation, "EST");
        assert_eq!(payload.locations[0].states[0].utc_offset_seconds, -18_000);
        assert_eq!(payload.locations[0].states[1].from_slot, 96 + 12);
        assert_eq!(payload.locations[0].states[1].notation, "EDT");
        assert_eq!(payload.locations[0].states[1].utc_offset_seconds, -14_400);
    }

    #[test]
    fn scrub_payload_marks_ambiguous_fall_back_times() {
        let config = AppConfig {
            timezones: vec![entry("America/New_York", "New York")],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap();

        let payload =
            build_scrub_payload(&config, now, "America/New_York", "America/New_York", "24h")
                .expect("build fall scrub payload");

        for index in 96 + 4..96 + 8 {
            assert!(payload.slots[index].ambiguous);
            assert!(payload.slots[index].reference_utc.is_some());
        }
        assert!(!payload.slots[96 + 3].ambiguous);
        assert!(!payload.slots[96 + 8].ambiguous);
    }

    #[test]
    fn scrub_payload_preserves_historical_second_level_offsets() {
        let config = AppConfig {
            timezones: vec![entry("Asia/Kolkata", "Kolkata")],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(1900, 1, 1, 12, 0, 0).unwrap();

        let payload = build_scrub_payload(&config, now, "Asia/Kolkata", "Asia/Kolkata", "24h")
            .expect("build historical scrub payload");
        let noon = &payload.slots[96 + 48];

        assert_eq!(
            noon.reference_utc.as_deref(),
            Some("1900-01-01T06:38:50+00:00")
        );
        assert_eq!(payload.locations[0].states[0].utc_offset_seconds, 19_270);
    }

    #[test]
    fn snapshot_keeps_a_second_place_in_the_local_timezone() {
        let config = AppConfig {
            timezones: vec![
                entry("America/New_York", "New York"),
                entry("America/New_York", "Boston"),
            ],
            pinned_locations: vec![LocationKey {
                timezone: "America/New_York".to_string(),
                label: "Boston".to_string(),
            }],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/New_York", "24h");

        assert_eq!(snapshot.summary.title, "New York");
        assert_eq!(snapshot.clocks.len(), 1);
        assert_eq!(snapshot.clocks[0].title, "Boston");
        assert!(snapshot.clocks[0].pinned);
    }

    #[test]
    fn snapshot_marks_a_pinned_local_location_on_the_summary() {
        let config = AppConfig {
            timezones: vec![entry("America/Cancun", "Cancun")],
            pinned_locations: vec![LocationKey {
                timezone: "America/Cancun".to_string(),
                label: "Cancun".to_string(),
            }],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert!(snapshot.summary.pinned);
        assert!(snapshot.clocks.is_empty());
    }

    #[test]
    fn snapshot_populates_timezone_coordinates_when_the_config_has_none() {
        let config = AppConfig {
            timezones: vec![entry("America/New_York", "New York")],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/New_York", "24h");

        assert!(snapshot.summary.latitude.is_some());
        assert!(snapshot.summary.longitude.is_some());
    }

    #[test]
    fn snapshot_supplies_live_featured_cities_for_the_globe() {
        let config = AppConfig {
            timezones: vec![entry("America/Cancun", "Cancun")],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");
        let tokyo = snapshot
            .featured_cities
            .iter()
            .find(|city| city.title == "Tokyo")
            .expect("Tokyo should be a featured globe city");

        assert_eq!(tokyo.timezone, "Asia/Tokyo");
        assert_eq!(tokyo.time, "20:05");
        assert_eq!(tokyo.notation, "JST");
        assert_eq!(tokyo.latitude, 35.686963);
        assert_eq!(tokyo.longitude, 139.749462);
        assert_eq!(tokyo.minimum_zoom, 0.75);
        assert!(snapshot.featured_cities.len() >= 300);
    }

    #[test]
    fn snapshot_hides_only_the_configured_place_from_a_shared_timezone() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Cancun", "Cancun"),
                entry("Asia/Tokyo", "Tokyo"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert!(!snapshot
            .featured_cities
            .iter()
            .any(|city| city.title == "Tokyo"));
        assert!(snapshot
            .featured_cities
            .iter()
            .any(|city| city.timezone == "Asia/Tokyo" && city.title != "Tokyo"));
    }

    #[test]
    fn snapshot_keeps_a_featured_city_when_a_different_place_shares_its_timezone() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Cancun", "Cancun"),
                entry("Europe/Paris", "Rennes"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert!(snapshot
            .featured_cities
            .iter()
            .any(|city| city.title == "Paris" && city.timezone == "Europe/Paris"));
        assert!(snapshot
            .featured_cities
            .iter()
            .any(|city| city.title == "Prague" && city.timezone == "Europe/Prague"));
    }

    #[test]
    fn snapshot_keeps_every_saved_clock_when_the_current_timezone_is_not_configured() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Vancouver", "Vancouver"),
                entry("America/Los_Angeles", "Los Angeles"),
                entry("America/Denver", "Denver"),
                entry("America/Chicago", "Chicago"),
                entry("America/New_York", "New York"),
                entry("America/Halifax", "Halifax"),
                entry("Europe/London", "London"),
                entry("Europe/Paris", "Paris"),
                entry("Asia/Kolkata", "Delhi"),
                entry("Asia/Tokyo", "Tokyo"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert_eq!(snapshot.configured_count, 10);
        assert!(!snapshot.local_configured);
        assert_eq!(snapshot.clocks.len(), 10);
        assert!(snapshot.clocks.iter().any(|clock| clock.title == "Tokyo"));
    }

    #[test]
    fn snapshot_keeps_every_pin_accessible_when_travel_adds_a_summary() {
        let timezones = vec![
            entry("America/Vancouver", "Vancouver"),
            entry("America/Los_Angeles", "Los Angeles"),
            entry("America/Denver", "Denver"),
            entry("America/Chicago", "Chicago"),
            entry("America/New_York", "New York"),
            entry("America/Halifax", "Halifax"),
            entry("Europe/London", "London"),
            entry("Europe/Paris", "Paris"),
            entry("Asia/Kolkata", "Delhi"),
            entry("Asia/Tokyo", "Tokyo"),
        ];
        let pinned_locations = timezones.iter().map(TimezoneEntry::location_key).collect();
        let config = AppConfig {
            timezones,
            pinned_locations,
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert_eq!(snapshot.clocks.len(), 10);
        assert!(snapshot.clocks.iter().all(|clock| clock.pinned));
    }

    #[test]
    fn timeline_alternates_dense_hourly_offsets_across_two_lanes() {
        let config = AppConfig {
            timezones: vec![
                entry("America/Vancouver", "Vancouver"),
                entry("America/Denver", "Denver"),
                entry("America/Cancun", "Local"),
                entry("America/New_York", "New York"),
            ],
            pinned_locations: vec![],
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");
        let lanes = snapshot
            .timeline
            .iter()
            .map(|item| item.lane)
            .collect::<Vec<_>>();

        assert_eq!(lanes, vec![0, 1, 0, 1]);
    }
}
