use crate::config::{
    detect_local_timezone, resolve_omarchy_weather_unit, system_time_format, ConfigManager,
    RemotePlaceSearch, TimezoneResolver,
};
use crate::quattro::{
    build_map_location, build_module_payload, build_search_location, build_snapshot,
    QuattroSnapshot,
};
use crate::time::parse_manual_reference_details;
use crate::timezone_grid::timezone_at;
use crate::weather::current_weather;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

pub const USAGE: &str = "Usage: omarchy-world-clock-backend <module|snapshot|convert|weather|search|locate|add|rename|remove|pin|unpin|version>";

fn optional_flag(args: &[String], flag: &str) -> Result<Option<String>> {
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if !argument.starts_with("--") {
            index += 1;
            continue;
        }

        let Some(value) = args.get(index + 1) else {
            bail!("missing value for flag {argument}");
        };
        if argument == flag {
            return Ok(Some(value.clone()));
        }
        index += 2;
    }

    Ok(None)
}

fn required_flag(args: &[String], flag: &str) -> Result<String> {
    optional_flag(args, flag)?.ok_or_else(|| anyhow::anyhow!("missing required flag {flag}"))
}

fn optional_f64(args: &[String], flag: &str) -> Result<Option<f64>> {
    optional_flag(args, flag)?
        .map(|value| {
            value
                .parse::<f64>()
                .with_context(|| format!("invalid number for {flag}: {value}"))
        })
        .transpose()
}

fn required_f64(args: &[String], flag: &str) -> Result<f64> {
    optional_f64(args, flag)?.ok_or_else(|| anyhow::anyhow!("missing required flag {flag}"))
}

fn parse_reference_utc(raw: Option<String>) -> Result<DateTime<Utc>> {
    raw.map(|value| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .with_context(|| format!("invalid RFC 3339 reference time: {value}"))
    })
    .transpose()
    .map(|value| value.unwrap_or_else(Utc::now))
}

fn current_snapshot(reference_utc: DateTime<Utc>) -> Result<QuattroSnapshot> {
    let config = ConfigManager::new(None).load()?;
    let local_timezone = detect_local_timezone();
    let mut snapshot = build_snapshot(
        &config,
        reference_utc,
        &local_timezone,
        &system_time_format(),
    );
    snapshot.weather_unit = resolve_omarchy_weather_unit(None, None, None, &local_timezone);
    Ok(snapshot)
}

#[derive(Serialize)]
struct ConversionPayload {
    normalized_input: String,
    snapshot: QuattroSnapshot,
}

pub fn execute(args: &[String]) -> Result<Option<String>> {
    let Some((command, remaining_args)) = args.split_first() else {
        bail!(USAGE);
    };

    let output = match command.as_str() {
        "module" => {
            let config = ConfigManager::new(None).load()?;
            let payload = build_module_payload(&config, Utc::now(), &system_time_format());
            Some(serde_json::to_string(&payload)?)
        }
        "snapshot" => {
            let reference_utc = parse_reference_utc(optional_flag(remaining_args, "--at")?)?;
            Some(serde_json::to_string(&current_snapshot(reference_utc)?)?)
        }
        "convert" => {
            let timezone = required_flag(remaining_args, "--timezone")?;
            let value = required_flag(remaining_args, "--value")?;
            let base = parse_reference_utc(optional_flag(remaining_args, "--base")?)?;
            let parsed = parse_manual_reference_details(&value, &timezone, base)
                .map_err(|message| anyhow::anyhow!(message))?;
            Some(serde_json::to_string(&ConversionPayload {
                normalized_input: parsed.normalized_text,
                snapshot: current_snapshot(parsed.reference_utc)?,
            })?)
        }
        "weather" => {
            let config = ConfigManager::new(None).load()?;
            let reference_utc = parse_reference_utc(optional_flag(remaining_args, "--at")?)?;
            let payload = current_weather(&config, &detect_local_timezone(), reference_utc)?;
            Some(serde_json::to_string(&payload)?)
        }
        "search" => {
            let query = remaining_args
                .first()
                .map(String::as_str)
                .unwrap_or_default();
            let reference_utc = parse_reference_utc(optional_flag(remaining_args, "--at")?)?;
            let config = ConfigManager::new(None).load()?;
            let resolver = TimezoneResolver::new(None);
            let mut results = resolver.search(query, 8);
            if results.is_empty() && !config.disable_open_meteo_geolocation {
                results = RemotePlaceSearch::new(None, None).search(query, 8);
            }
            let local_timezone = detect_local_timezone();
            let time_format = system_time_format();
            let results = results
                .into_iter()
                .map(|result| {
                    build_search_location(result, reference_utc, &local_timezone, &time_format)
                })
                .collect::<Vec<_>>();
            Some(serde_json::to_string(&results)?)
        }
        "locate" => {
            let latitude = required_f64(remaining_args, "--latitude")?;
            let longitude = required_f64(remaining_args, "--longitude")?;
            let reference_utc = parse_reference_utc(optional_flag(remaining_args, "--at")?)?;
            if !latitude.is_finite()
                || !longitude.is_finite()
                || !(-90.0..=90.0).contains(&latitude)
                || !(-180.0..=180.0).contains(&longitude)
            {
                bail!("map coordinates are outside the world extent");
            }

            let timezone = timezone_at(latitude, longitude);
            let location = if let Some(timezone) = timezone {
                TimezoneResolver::new(None)
                    .describe_timezone(&timezone)
                    .map(|result| {
                        build_map_location(
                            &result,
                            latitude,
                            longitude,
                            reference_utc,
                            &detect_local_timezone(),
                            &system_time_format(),
                        )
                    })
            } else {
                None
            };
            Some(serde_json::to_string(&location)?)
        }
        "add" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to add"))?;
            let label = optional_flag(remaining_args, "--label")?.unwrap_or_default();
            let outcome = ConfigManager::new(None).add_timezone_with_coordinate(
                timezone,
                &label,
                optional_f64(remaining_args, "--latitude")?,
                optional_f64(remaining_args, "--longitude")?,
            )?;
            if !outcome.added {
                bail!("location is invalid or already configured: {timezone}");
            }
            None
        }
        "rename" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to rename"))?;
            ConfigManager::new(None).rename_location(
                timezone,
                &required_flag(remaining_args, "--label")?,
                &required_flag(remaining_args, "--new-label")?,
            )?;
            None
        }
        "remove" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to remove"))?;
            ConfigManager::new(None).remove_location(
                timezone,
                optional_flag(remaining_args, "--label")?.as_deref(),
            )?;
            None
        }
        "pin" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to pin"))?;
            ConfigManager::new(None).set_pinned_location(
                Some(timezone),
                optional_flag(remaining_args, "--label")?.as_deref(),
            )?;
            None
        }
        "unpin" => {
            ConfigManager::new(None).set_pinned_timezone(None)?;
            None
        }
        "version" | "--version" | "-V" => Some(env!("CARGO_PKG_VERSION").to_string()),
        _ => bail!("unknown backend command: {command}\n{USAGE}"),
    };

    Ok(output)
}
