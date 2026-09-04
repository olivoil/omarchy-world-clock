use crate::config::{
    detect_local_timezone, place_coordinate, resolve_omarchy_weather_unit, system_time_format,
    AppConfig, ConfigManager, TimezoneEntry, TimezoneResolver,
};
use crate::quattro::visible_location_entries;
use crate::time::{
    format_display_time, format_offset, format_timezone_notation, parse_manual_reference_details,
    parse_timezone, zoned_datetime,
};
use crate::weather::{current_weather_for_entry, HourlyWeather, LocationWeather};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Offset, SecondsFormat, TimeZone, Timelike, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub const API_VERSION: u64 = 1;

pub const USAGE: &str = r#"Usage: omarchy-world-clock <command> [options]

Agent-friendly commands (JSON output):
  places [--at <RFC3339>]
  time (--location <name> | --id <id>) [--at <RFC3339>]
  convert --time <value> [--from <name> | --from-id <id>] [--base <RFC3339>]
  forecast (--location <name> | --id <id>) [--at <RFC3339>]
  overlap (--location <name> | --id <id>)... [--date <YYYY-MM-DD>]
          [--days <1-31>] [--work-start <HH:MM>] [--work-end <HH:MM>]
          [--duration-minutes <minutes>]

Use the special location name "local" for the system timezone. Time values for
convert accept the same forms as the panel: HH:MM, 830, 8.5, 3pm, or a local
YYYY-MM-DD HH:MM. Forecast requests contact Open-Meteo unless the World Clock
privacy opt-out is enabled."#;

#[derive(Debug, Clone)]
struct ResolvedEntry {
    entry: TimezoneEntry,
    is_local: bool,
    configured: bool,
}

#[derive(Debug, Clone)]
enum Locator {
    Query(String),
    Id(u64),
}

#[derive(Debug, Default)]
struct Options {
    values: HashMap<String, Vec<String>>,
}

impl Options {
    fn parse(args: &[String], allowed: &[&str]) -> Result<Self> {
        let allowed = allowed.iter().copied().collect::<HashSet<_>>();
        let mut values = HashMap::<String, Vec<String>>::new();
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            if !flag.starts_with("--") || !allowed.contains(flag) {
                bail!("unknown option for agent API: {flag}\n{USAGE}");
            }
            let Some(value) = args.get(index + 1) else {
                bail!("missing value for option {flag}");
            };
            values
                .entry(flag.to_string())
                .or_default()
                .push(value.clone());
            index += 2;
        }
        Ok(Self { values })
    }

    fn one(&self, flag: &str) -> Result<Option<&str>> {
        let Some(values) = self.values.get(flag) else {
            return Ok(None);
        };
        if values.len() > 1 {
            bail!("option {flag} may only be supplied once");
        }
        Ok(values.first().map(String::as_str))
    }

    fn many(&self, flag: &str) -> impl Iterator<Item = &str> {
        self.values
            .get(flag)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentLocation {
    pub id: u64,
    pub is_local: bool,
    pub configured: bool,
    pub timezone: String,
    pub place: String,
    pub label: String,
    pub custom_label: String,
    pub pinned: bool,
    pub local_datetime: String,
    pub date: String,
    pub time: String,
    pub timezone_abbreviation: String,
    pub utc_offset: String,
    pub utc_offset_seconds: i32,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Serialize)]
struct PlacesPayload {
    api_version: u64,
    reference_utc: String,
    local_timezone: String,
    time_format: String,
    locations: Vec<AgentLocation>,
}

#[derive(Debug, Serialize)]
struct TimePayload {
    api_version: u64,
    reference_utc: String,
    location: AgentLocation,
}

#[derive(Debug, Serialize)]
struct ConvertPayload {
    api_version: u64,
    normalized_input: String,
    reference_utc: String,
    source: AgentLocation,
    locations: Vec<AgentLocation>,
}

#[derive(Debug, Serialize)]
struct ForecastPayload {
    api_version: u64,
    generated_at_utc: String,
    requested_at_utc: Option<String>,
    status: &'static str,
    source: &'static str,
    attribution_url: &'static str,
    temperature_unit_preference: Option<String>,
    partial: bool,
    location: AgentLocation,
    weather: Option<LocationWeather>,
    forecast_at: Option<HourlyWeather>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct OverlapParticipantInterval {
    id: u64,
    label: String,
    timezone: String,
    start: String,
    end: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct OverlapWindow {
    start_utc: String,
    end_utc: String,
    duration_minutes: i64,
    local_start: String,
    local_end: String,
    participants: Vec<OverlapParticipantInterval>,
}

#[derive(Debug, Serialize)]
struct OverlapPayload {
    api_version: u64,
    generated_at_utc: String,
    date_from: String,
    date_to_exclusive: String,
    date_timezone: String,
    work_start: String,
    work_end: String,
    minimum_duration_minutes: u32,
    granularity_minutes: u32,
    participants: Vec<AgentLocation>,
    windows: Vec<OverlapWindow>,
}

fn rfc3339_utc(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_reference(raw: Option<&str>) -> Result<DateTime<Utc>> {
    raw.map(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&Utc))
            .with_context(|| format!("invalid RFC 3339 time: {value}"))
    })
    .transpose()
    .map(|value| value.unwrap_or_else(Utc::now))
}

fn parse_number<T>(raw: Option<&str>, flag: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    raw.map(|value| {
        value
            .parse::<T>()
            .map_err(|error| anyhow::anyhow!("invalid value for {flag}: {value} ({error})"))
    })
    .transpose()
    .map(|value| value.unwrap_or(default))
}

fn entries(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
) -> Vec<ResolvedEntry> {
    let (local_configured, entries) =
        visible_location_entries(config, reference_utc, local_timezone);
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| ResolvedEntry {
            entry,
            is_local: index == 0,
            configured: index != 0 || local_configured,
        })
        .collect()
}

fn locator(options: &Options, query_flag: &str, id_flag: &str) -> Result<Option<Locator>> {
    let query = options.one(query_flag)?;
    let id = options.one(id_flag)?;
    match (query, id) {
        (Some(_), Some(_)) => bail!("use either {query_flag} or {id_flag}, not both"),
        (Some(query), None) => Ok(Some(Locator::Query(query.to_string()))),
        (None, Some(id)) => {
            Ok(Some(Locator::Id(id.parse::<u64>().with_context(|| {
                format!("invalid location ID for {id_flag}: {id}")
            })?)))
        }
        (None, None) => Ok(None),
    }
}

fn normalized_fields(entry: &TimezoneEntry) -> [String; 6] {
    [
        TimezoneResolver::normalize(&entry.label),
        TimezoneResolver::normalize(&entry.display_label()),
        TimezoneResolver::normalize(&entry.place_label()),
        TimezoneResolver::normalize(&entry.read_card_title()),
        TimezoneResolver::normalize(&entry.read_place_title()),
        TimezoneResolver::normalize(&entry.timezone),
    ]
}

fn match_score(candidate: &ResolvedEntry, query: &str) -> Option<u8> {
    if candidate.is_local && matches!(query, "local" | "my" | "me" | "my time" | "system time") {
        return Some(0);
    }

    let fields = normalized_fields(&candidate.entry);
    if !fields[0].is_empty() && fields[0] == query {
        return Some(1);
    }
    if fields[1] == query {
        return Some(2);
    }
    if fields[2] == query {
        return Some(3);
    }
    if fields[3] == query || fields[4] == query {
        return Some(4);
    }
    if fields[5] == query {
        return Some(5);
    }
    if fields.iter().any(|field| {
        !field.is_empty()
            && (field.starts_with(&format!("{query} "))
                || field.split_whitespace().any(|word| word == query))
    }) {
        return Some(6);
    }
    if fields
        .iter()
        .any(|field| !field.is_empty() && field.contains(query))
    {
        return Some(7);
    }
    None
}

fn candidate_description(candidate: &ResolvedEntry) -> String {
    let id = if candidate.entry.id == 0 {
        "local".to_string()
    } else {
        candidate.entry.id.to_string()
    };
    format!(
        "{id}: {} — {} ({})",
        candidate.entry.display_label(),
        candidate.entry.place_label(),
        candidate.entry.timezone
    )
}

fn resolve(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    locator: &Locator,
) -> Result<ResolvedEntry> {
    let candidates = entries(config, reference_utc, local_timezone);
    if let Locator::Id(id) = locator {
        let matches = candidates
            .into_iter()
            .filter(|candidate| candidate.entry.id == *id)
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [candidate] => Ok(candidate.clone()),
            [] => bail!("no World Clock location has ID {id}; run `omarchy-world-clock places`"),
            _ => bail!("World Clock location ID {id} is not unique"),
        };
    }

    let Locator::Query(raw_query) = locator else {
        unreachable!();
    };
    if let Some(raw_id) = raw_query.trim().strip_prefix('#') {
        let id = raw_id
            .parse::<u64>()
            .with_context(|| format!("invalid World Clock location ID: {raw_query}"))?;
        return resolve(config, reference_utc, local_timezone, &Locator::Id(id));
    }
    let query = TimezoneResolver::normalize(raw_query);
    if query.is_empty() {
        bail!("location query cannot be empty");
    }

    let mut scored = candidates
        .into_iter()
        .filter_map(|candidate| match_score(&candidate, &query).map(|score| (score, candidate)))
        .collect::<Vec<_>>();
    let Some(best_score) = scored.iter().map(|(score, _)| *score).min() else {
        bail!(
            "no saved World Clock location matches {raw_query:?}; run `omarchy-world-clock places`"
        );
    };
    scored.retain(|(score, _)| *score == best_score);
    if scored.len() > 1 {
        let choices = scored
            .iter()
            .map(|(_, candidate)| candidate_description(candidate))
            .collect::<Vec<_>>()
            .join(", ");
        bail!("location {raw_query:?} is ambiguous; use --id ({choices})");
    }
    Ok(scored.remove(0).1)
}

fn agent_location(
    config: &AppConfig,
    candidate: &ResolvedEntry,
    reference_utc: DateTime<Utc>,
    time_format: &str,
) -> AgentLocation {
    let local = zoned_datetime(reference_utc, &candidate.entry.timezone);
    let offset_seconds = local.offset().fix().local_minus_utc();
    let (latitude, longitude) = place_coordinate(&candidate.entry)
        .map(|(latitude, longitude)| (Some(latitude), Some(longitude)))
        .unwrap_or((None, None));
    AgentLocation {
        id: candidate.entry.id,
        is_local: candidate.is_local,
        configured: candidate.configured,
        timezone: candidate.entry.timezone.clone(),
        place: candidate.entry.place_label(),
        label: candidate.entry.display_label(),
        custom_label: candidate.entry.label.clone(),
        pinned: config.is_pinned(&candidate.entry),
        local_datetime: local.to_rfc3339_opts(SecondsFormat::Secs, true),
        date: local.format("%Y-%m-%d").to_string(),
        time: format_display_time(&local, time_format),
        timezone_abbreviation: format_timezone_notation(&local),
        utc_offset: format_offset(offset_seconds),
        utc_offset_seconds: offset_seconds,
        latitude,
        longitude,
    }
}

fn all_agent_locations(
    config: &AppConfig,
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
    time_format: &str,
) -> Vec<AgentLocation> {
    entries(config, reference_utc, local_timezone)
        .iter()
        .map(|candidate| agent_location(config, candidate, reference_utc, time_format))
        .collect()
}

fn parse_clock_minutes(raw: &str, allow_24: bool) -> Result<u32> {
    let Some((hour, minute)) = raw.trim().split_once(':') else {
        bail!("invalid work-hour time {raw:?}; use HH:MM");
    };
    let hour = hour
        .parse::<u32>()
        .with_context(|| format!("invalid hour in {raw:?}"))?;
    let minute = minute
        .parse::<u32>()
        .with_context(|| format!("invalid minute in {raw:?}"))?;
    if minute > 59 || hour > 23 + u32::from(allow_24) || (hour == 24 && minute != 0) {
        bail!("invalid work-hour time {raw:?}; use HH:MM");
    }
    Ok(hour * 60 + minute)
}

fn within_work_hours(reference: DateTime<Utc>, timezone: &str, start: u32, end: u32) -> bool {
    let local = zoned_datetime(reference, timezone);
    let minute = local.hour() * 60 + local.minute();
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

fn local_iso(reference: DateTime<Utc>, timezone: &str) -> String {
    zoned_datetime(reference, timezone).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn overlap_window(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    local_timezone: &str,
    participants: &[ResolvedEntry],
) -> OverlapWindow {
    OverlapWindow {
        start_utc: rfc3339_utc(start),
        end_utc: rfc3339_utc(end),
        duration_minutes: end.signed_duration_since(start).num_minutes(),
        local_start: local_iso(start, local_timezone),
        local_end: local_iso(end, local_timezone),
        participants: participants
            .iter()
            .map(|participant| OverlapParticipantInterval {
                id: participant.entry.id,
                label: participant.entry.display_label(),
                timezone: participant.entry.timezone.clone(),
                start: local_iso(start, &participant.entry.timezone),
                end: local_iso(end, &participant.entry.timezone),
            })
            .collect(),
    }
}

fn build_overlap_windows(
    participants: &[ResolvedEntry],
    local_timezone: &str,
    start_date: NaiveDate,
    days: u32,
    work_start: u32,
    work_end: u32,
    minimum_duration: u32,
) -> Result<(NaiveDate, Vec<OverlapWindow>)> {
    let end_date = start_date
        .checked_add_days(chrono::Days::new(u64::from(days)))
        .context("meeting search date is outside the supported range")?;
    let midnight = start_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be valid as a naive time");
    let end_midnight = end_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be valid as a naive time");
    // Every current IANA offset fits inside this padding. Filtering by the
    // system-local calendar below makes the date range exact even around DST.
    let mut reference = Utc.from_utc_datetime(&(midnight - Duration::days(1)));
    let scan_end = Utc.from_utc_datetime(&(end_midnight + Duration::days(1)));
    let step = Duration::minutes(15);
    let mut open_start = None;
    let mut windows = Vec::new();

    while reference <= scan_end {
        let system_date = zoned_datetime(reference, local_timezone).date_naive();
        let inside_dates = system_date >= start_date && system_date < end_date;
        let acceptable = inside_dates
            && participants.iter().all(|participant| {
                within_work_hours(reference, &participant.entry.timezone, work_start, work_end)
            });

        if acceptable && open_start.is_none() {
            open_start = Some(reference);
        } else if !acceptable {
            if let Some(start) = open_start.take() {
                if reference.signed_duration_since(start).num_minutes()
                    >= i64::from(minimum_duration)
                {
                    windows.push(overlap_window(
                        start,
                        reference,
                        local_timezone,
                        participants,
                    ));
                }
            }
        }
        reference += step;
    }
    Ok((end_date, windows))
}

fn forecast_at(weather: &LocationWeather, requested: DateTime<Utc>) -> Option<HourlyWeather> {
    weather.hourly_forecast.iter().find_map(|hour| {
        let start = DateTime::parse_from_rfc3339(&hour.reference_utc)
            .ok()?
            .with_timezone(&Utc);
        (requested >= start && requested < start + Duration::hours(1)).then(|| hour.clone())
    })
}

fn command_places(args: &[String]) -> Result<String> {
    let options = Options::parse(args, &["--at"])?;
    let reference = parse_reference(options.one("--at")?)?;
    let config = ConfigManager::new(None).load()?;
    let local_timezone = detect_local_timezone();
    let time_format = system_time_format();
    serde_json::to_string(&PlacesPayload {
        api_version: API_VERSION,
        reference_utc: rfc3339_utc(reference),
        local_timezone: local_timezone.clone(),
        time_format: time_format.clone(),
        locations: all_agent_locations(&config, reference, &local_timezone, &time_format),
    })
    .context("could not serialize World Clock places")
}

fn command_time(args: &[String]) -> Result<String> {
    let options = Options::parse(args, &["--location", "--id", "--at"])?;
    let Some(locator) = locator(&options, "--location", "--id")? else {
        bail!("time requires --location <name> or --id <id>");
    };
    let reference = parse_reference(options.one("--at")?)?;
    let config = ConfigManager::new(None).load()?;
    let local_timezone = detect_local_timezone();
    let time_format = system_time_format();
    let candidate = resolve(&config, reference, &local_timezone, &locator)?;
    serde_json::to_string(&TimePayload {
        api_version: API_VERSION,
        reference_utc: rfc3339_utc(reference),
        location: agent_location(&config, &candidate, reference, &time_format),
    })
    .context("could not serialize World Clock time")
}

fn command_convert(args: &[String]) -> Result<String> {
    let options = Options::parse(args, &["--time", "--from", "--from-id", "--base"])?;
    let value = options
        .one("--time")?
        .context("convert requires --time <value>")?;
    let source_locator = locator(&options, "--from", "--from-id")?
        .unwrap_or_else(|| Locator::Query("local".to_string()));
    let base = parse_reference(options.one("--base")?)?;
    let config = ConfigManager::new(None).load()?;
    let local_timezone = detect_local_timezone();
    let source = resolve(&config, base, &local_timezone, &source_locator)?;
    let parsed = parse_manual_reference_details(value, &source.entry.timezone, base)
        .map_err(|message| anyhow::anyhow!(message))?;
    let time_format = system_time_format();
    let source_at_reference = ResolvedEntry {
        entry: source.entry,
        is_local: source.is_local,
        configured: source.configured,
    };
    serde_json::to_string(&ConvertPayload {
        api_version: API_VERSION,
        normalized_input: parsed.normalized_text,
        reference_utc: rfc3339_utc(parsed.reference_utc),
        source: agent_location(
            &config,
            &source_at_reference,
            parsed.reference_utc,
            &time_format,
        ),
        locations: all_agent_locations(
            &config,
            parsed.reference_utc,
            &local_timezone,
            &time_format,
        ),
    })
    .context("could not serialize World Clock conversion")
}

fn command_forecast(args: &[String]) -> Result<String> {
    let options = Options::parse(args, &["--location", "--id", "--at"])?;
    let Some(locator) = locator(&options, "--location", "--id")? else {
        bail!("forecast requires --location <name> or --id <id>");
    };
    let requested = options
        .one("--at")?
        .map(|value| parse_reference(Some(value)))
        .transpose()?;
    let generated_at = Utc::now();
    let config = ConfigManager::new(None).load()?;
    let local_timezone = detect_local_timezone();
    let time_format = system_time_format();
    let candidate = resolve(&config, generated_at, &local_timezone, &locator)?;
    let location = agent_location(&config, &candidate, generated_at, &time_format);
    let payload = current_weather_for_entry(
        &candidate.entry,
        config.disable_open_meteo_geolocation,
        generated_at,
    )?;
    let weather = payload
        .locations
        .into_iter()
        .find(|weather| weather.id == candidate.entry.id);
    let selected = requested.and_then(|at| {
        weather
            .as_ref()
            .and_then(|location_weather| forecast_at(location_weather, at))
    });
    let has_coordinates = location.latitude.is_some() && location.longitude.is_some();
    let status = if payload.disabled {
        "disabled"
    } else if weather.is_some() {
        "ok"
    } else if has_coordinates {
        "unavailable"
    } else {
        "no_coordinates"
    };
    serde_json::to_string(&ForecastPayload {
        api_version: API_VERSION,
        generated_at_utc: rfc3339_utc(generated_at),
        requested_at_utc: requested.map(rfc3339_utc),
        status,
        source: payload.source,
        attribution_url: payload.attribution_url,
        temperature_unit_preference: resolve_omarchy_weather_unit(
            None,
            None,
            None,
            &local_timezone,
        ),
        partial: payload.partial,
        location,
        weather,
        forecast_at: selected,
    })
    .context("could not serialize World Clock forecast")
}

fn command_overlap(args: &[String]) -> Result<String> {
    let options = Options::parse(
        args,
        &[
            "--location",
            "--id",
            "--date",
            "--days",
            "--work-start",
            "--work-end",
            "--duration-minutes",
        ],
    )?;
    let mut locators = options
        .many("--location")
        .map(|query| Locator::Query(query.to_string()))
        .collect::<Vec<_>>();
    for id in options.many("--id") {
        locators
            .push(Locator::Id(id.parse::<u64>().with_context(|| {
                format!("invalid location ID for --id: {id}")
            })?));
    }
    if locators.is_empty() {
        bail!("overlap requires at least one --location <name> or --id <id>");
    }

    let now = Utc::now();
    let config = ConfigManager::new(None).load()?;
    let local_timezone = detect_local_timezone();
    let local_zone = parse_timezone(&local_timezone).context("local timezone is invalid")?;
    let start_date = options
        .one("--date")?
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .with_context(|| format!("invalid date for --date: {value}"))
        })
        .transpose()?
        .unwrap_or_else(|| now.with_timezone(&local_zone).date_naive());
    let days = parse_number(options.one("--days")?, "--days", 7_u32)?;
    if !(1..=31).contains(&days) {
        bail!("--days must be between 1 and 31");
    }
    let minimum_duration = parse_number(
        options.one("--duration-minutes")?,
        "--duration-minutes",
        30_u32,
    )?;
    if minimum_duration == 0 || minimum_duration > days * 24 * 60 {
        bail!("--duration-minutes is outside the requested search range");
    }
    let work_start_text = options.one("--work-start")?.unwrap_or("09:00");
    let work_end_text = options.one("--work-end")?.unwrap_or("17:00");
    let work_start = parse_clock_minutes(work_start_text, false)?;
    let work_end = parse_clock_minutes(work_end_text, true)?;
    if work_start == work_end {
        bail!("work-start and work-end must describe a non-empty range");
    }

    let mut participants = Vec::new();
    let mut seen = HashSet::new();
    for locator in locators {
        let participant = resolve(&config, now, &local_timezone, &locator)?;
        let key = if participant.entry.id == 0 {
            format!("local:{}", participant.entry.timezone)
        } else {
            format!("id:{}", participant.entry.id)
        };
        if seen.insert(key) {
            participants.push(participant);
        }
    }
    let (end_date, windows) = build_overlap_windows(
        &participants,
        &local_timezone,
        start_date,
        days,
        work_start,
        work_end,
        minimum_duration,
    )?;
    let time_format = system_time_format();
    let participant_locations = participants
        .iter()
        .map(|participant| agent_location(&config, participant, now, &time_format))
        .collect();
    serde_json::to_string(&OverlapPayload {
        api_version: API_VERSION,
        generated_at_utc: rfc3339_utc(now),
        date_from: start_date.format("%Y-%m-%d").to_string(),
        date_to_exclusive: end_date.format("%Y-%m-%d").to_string(),
        date_timezone: local_timezone,
        work_start: format!("{:02}:{:02}", work_start / 60, work_start % 60),
        work_end: format!("{:02}:{:02}", work_end / 60, work_end % 60),
        minimum_duration_minutes: minimum_duration,
        granularity_minutes: 15,
        participants: participant_locations,
        windows,
    })
    .context("could not serialize World Clock meeting overlap")
}

pub fn execute(args: &[String]) -> Result<String> {
    let Some((command, remaining)) = args.split_first() else {
        bail!(USAGE);
    };
    match command.as_str() {
        "places" => command_places(remaining),
        "time" => command_time(remaining),
        "convert" => command_convert(remaining),
        "forecast" => command_forecast(remaining),
        "overlap" => command_overlap(remaining),
        "help" | "--help" | "-h" => Ok(USAGE.to_string()),
        _ => bail!("unknown agent API command: {command}\n{USAGE}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_overlap_windows, resolve, AgentLocation, Locator, ResolvedEntry, API_VERSION,
    };
    use crate::config::{AppConfig, LocationKey, TimezoneEntry};
    use chrono::{NaiveDate, TimeZone, Utc};

    fn entry(id: u64, timezone: &str, place: &str, label: &str) -> TimezoneEntry {
        TimezoneEntry {
            id,
            timezone: timezone.to_string(),
            place: place.to_string(),
            label: label.to_string(),
            latitude: None,
            longitude: None,
        }
    }

    fn config() -> AppConfig {
        AppConfig {
            timezones: vec![
                entry(1, "America/Cancun", "Cancun", "Home"),
                entry(2, "America/New_York", "Boston", "Jeff"),
                entry(3, "Europe/Paris", "Rennes", "Jenny"),
            ],
            pinned_locations: vec![LocationKey {
                id: 2,
                timezone: String::new(),
                label: String::new(),
            }],
            disable_open_meteo_geolocation: false,
        }
    }

    #[test]
    fn resolves_a_person_by_custom_label_before_place_names() {
        let mut config = config();
        config
            .timezones
            .push(entry(4, "America/Chicago", "Jeff", "Office"));
        let reference = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let result = resolve(
            &config,
            reference,
            "America/Cancun",
            &Locator::Query("Jeff".into()),
        )
        .unwrap();
        assert_eq!(result.entry.id, 2);
        assert_eq!(result.entry.place, "Boston");
    }

    #[test]
    fn duplicate_custom_labels_are_reported_as_ambiguous() {
        let mut config = config();
        config
            .timezones
            .push(entry(4, "America/Chicago", "Austin", "Jeff"));
        let reference = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let error = resolve(
            &config,
            reference,
            "America/Cancun",
            &Locator::Query("Jeff".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("ambiguous"));
        assert!(error.contains("2: Jeff"));
        assert!(error.contains("4: Jeff"));
    }

    #[test]
    fn overlap_returns_each_participants_local_range() {
        let participants = vec![
            ResolvedEntry {
                entry: entry(1, "America/New_York", "Boston", "Jeff"),
                is_local: false,
                configured: true,
            },
            ResolvedEntry {
                entry: entry(2, "Europe/Paris", "Rennes", "Jenny"),
                is_local: false,
                configured: true,
            },
        ];
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let (_, windows) = build_overlap_windows(
            &participants,
            "America/Cancun",
            date,
            1,
            9 * 60,
            17 * 60,
            30,
        )
        .unwrap();

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].duration_minutes, 120);
        assert!(windows[0].participants[0].start.contains("09:00:00-04:00"));
        assert!(windows[0].participants[0].end.contains("11:00:00-04:00"));
        assert!(windows[0].participants[1].start.contains("15:00:00+02:00"));
        assert!(windows[0].participants[1].end.contains("17:00:00+02:00"));
    }

    #[test]
    fn overlap_tracks_mismatched_daylight_saving_transitions() {
        let participants = vec![
            ResolvedEntry {
                entry: entry(1, "America/New_York", "New York", "Jeff"),
                is_local: false,
                configured: true,
            },
            ResolvedEntry {
                entry: entry(2, "Europe/London", "London", "Jenny"),
                is_local: false,
                configured: true,
            },
        ];
        let before_us_change = NaiveDate::from_ymd_opt(2026, 3, 7).unwrap();
        let after_us_change = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();

        let (_, before) = build_overlap_windows(
            &participants,
            "UTC",
            before_us_change,
            1,
            9 * 60,
            17 * 60,
            30,
        )
        .unwrap();
        let (_, after) = build_overlap_windows(
            &participants,
            "UTC",
            after_us_change,
            1,
            9 * 60,
            17 * 60,
            30,
        )
        .unwrap();

        assert_eq!(before[0].duration_minutes, 180);
        assert_eq!(after[0].duration_minutes, 240);
        assert!(before[0].participants[0].start.contains("09:00:00-05:00"));
        assert!(after[0].participants[0].start.contains("09:00:00-04:00"));
    }

    #[test]
    fn public_agent_location_remains_json_friendly() {
        let location = AgentLocation {
            id: 1,
            is_local: true,
            configured: true,
            timezone: "UTC".into(),
            place: "UTC".into(),
            label: "Home".into(),
            custom_label: "Home".into(),
            pinned: false,
            local_datetime: "2026-09-04T12:00:00Z".into(),
            date: "2026-09-04".into(),
            time: "12:00".into(),
            timezone_abbreviation: "UTC".into(),
            utc_offset: "UTC+00:00".into(),
            utc_offset_seconds: 0,
            latitude: None,
            longitude: None,
        };
        let json = serde_json::to_value(location).unwrap();
        assert_eq!(API_VERSION, 1);
        assert_eq!(json["custom_label"], "Home");
        assert_eq!(json["utc_offset"], "UTC+00:00");
    }
}
