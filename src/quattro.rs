use crate::config::{AppConfig, TimezoneEntry, TimezoneSearchResult};
use crate::time::{
    format_display_time, format_timezone_notation, friendly_timezone_name, zoned_datetime,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SNAPSHOT_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuattroClock {
    pub timezone: String,
    pub label: String,
    pub title: String,
    pub time: String,
    pub day: String,
    pub notation: String,
    pub relative_minutes: i64,
    pub relative_label: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub pinned: bool,
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
    pub configured_count: usize,
    pub pinned_timezone: Option<String>,
    pub summary: QuattroClock,
    pub clocks: Vec<QuattroClock>,
    pub timeline: Vec<QuattroTimelineItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuattroMapLocation {
    pub timezone: String,
    pub title: String,
    pub subtitle: String,
    pub latitude: f64,
    pub longitude: f64,
    pub time: String,
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
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroClock {
    let zoned = zoned_datetime(reference_utc, &entry.timezone);
    let relative_minutes = wall_clock_delta_minutes(reference_utc, local_timezone, &entry.timezone);
    QuattroClock {
        timezone: entry.timezone.clone(),
        label: entry.display_label(),
        title: entry.read_card_title(),
        time: format_display_time(&zoned, time_format),
        day: day_label(reference_utc, local_timezone, &entry.timezone),
        notation: format_timezone_notation(&zoned),
        relative_minutes,
        relative_label: relative_label(relative_minutes),
        latitude: entry.latitude,
        longitude: entry.longitude,
        pinned: config.pinned_timezone.as_deref() == Some(entry.timezone.as_str()),
    }
}

fn local_entry(config: &AppConfig, local_timezone: &str) -> TimezoneEntry {
    config
        .timezones
        .iter()
        .find(|entry| entry.timezone == local_timezone)
        .cloned()
        .unwrap_or_else(|| TimezoneEntry {
            timezone: local_timezone.to_string(),
            label: friendly_timezone_name(local_timezone),
            latitude: None,
            longitude: None,
        })
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
        time: format_display_time(&zoned, time_format),
        notation: format_timezone_notation(&zoned),
        relative_label: relative_label(relative_minutes),
    }
}

pub fn build_snapshot(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> QuattroSnapshot {
    let summary_entry = local_entry(config, local_timezone);
    let summary = clock_from_entry(
        &summary_entry,
        config,
        reference_utc,
        local_timezone,
        time_format,
    );

    let mut entries = config
        .timezones
        .iter()
        .filter(|entry| entry.timezone != local_timezone)
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
    let clocks = entries
        .into_iter()
        .map(|entry| clock_from_entry(entry, config, reference_utc, local_timezone, time_format))
        .collect::<Vec<_>>();
    let timeline = timeline_items(&summary, &clocks);

    QuattroSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        reference_utc: reference_utc.to_rfc3339(),
        local_timezone: local_timezone.to_string(),
        time_format: time_format.to_string(),
        configured_count: config.timezones.len(),
        pinned_timezone: config.pinned_timezone.clone(),
        summary,
        clocks,
        timeline,
    }
}

#[cfg(test)]
mod tests {
    use super::build_snapshot;
    use crate::config::{AppConfig, TimezoneEntry};
    use chrono::{TimeZone, Utc};

    fn entry(timezone: &str, label: &str) -> TimezoneEntry {
        TimezoneEntry {
            timezone: timezone.to_string(),
            label: label.to_string(),
            latitude: None,
            longitude: None,
        }
    }

    #[test]
    fn snapshot_sorts_clocks_and_marks_the_pin() {
        let config = AppConfig {
            timezones: vec![
                entry("Europe/Paris", "Rennes"),
                entry("America/Cancun", "Local"),
                entry("America/Vancouver", "Vancouver"),
            ],
            pinned_timezone: Some("Europe/Paris".to_string()),
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 11, 5, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert_eq!(snapshot.summary.timezone, "America/Cancun");
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
            pinned_timezone: None,
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 23, 45, 0).unwrap();

        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");

        assert_eq!(snapshot.clocks[0].relative_label, "10h 30m ahead");
        assert_eq!(snapshot.clocks[0].day, "Tomorrow");
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
            pinned_timezone: None,
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
