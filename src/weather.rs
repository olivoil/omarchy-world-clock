use crate::config::{place_coordinate, AppConfig};
use crate::quattro::visible_location_entries;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPEN_METEO_FORECAST_ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";
const OPEN_METEO_ATTRIBUTION_URL: &str = "https://open-meteo.com/";

#[derive(Debug, Clone, PartialEq)]
struct WeatherLocation {
    timezone: String,
    label: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocationWeather {
    pub timezone: String,
    pub label: String,
    pub temperature_celsius: f64,
    pub weather_code: i64,
    pub condition: &'static str,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeatherPayload {
    pub source: &'static str,
    pub attribution_url: &'static str,
    pub disabled: bool,
    pub locations: Vec<LocationWeather>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    weather_code: i64,
    is_day: i64,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoForecast {
    current: Option<OpenMeteoCurrent>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenMeteoResponse {
    One(OpenMeteoForecast),
    Many(Vec<OpenMeteoForecast>),
}

pub fn current_weather(
    config: &AppConfig,
    local_timezone: &str,
    reference_utc: DateTime<Utc>,
) -> Result<WeatherPayload> {
    let mut payload = WeatherPayload {
        source: "Open-Meteo",
        attribution_url: OPEN_METEO_ATTRIBUTION_URL,
        disabled: config.disable_open_meteo_geolocation,
        locations: Vec::new(),
    };
    if payload.disabled {
        return Ok(payload);
    }

    let locations = weather_locations(config, local_timezone, reference_utc);
    if locations.is_empty() {
        return Ok(payload);
    }

    let latitudes = locations
        .iter()
        .map(|location| location.latitude.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let longitudes = locations
        .iter()
        .map(|location| location.longitude.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("could not initialize the weather client")?;
    let response = client
        .get(OPEN_METEO_FORECAST_ENDPOINT)
        .query(&[
            ("latitude", latitudes.as_str()),
            ("longitude", longitudes.as_str()),
            ("current", "temperature_2m,weather_code,is_day"),
            ("temperature_unit", "celsius"),
            ("forecast_days", "1"),
        ])
        .send()
        .context("could not reach Open-Meteo")?
        .error_for_status()
        .context("Open-Meteo returned an error")?
        .text()
        .context("could not read the Open-Meteo response")?;

    payload.locations = parse_weather_response(&response, &locations)?;
    Ok(payload)
}

fn weather_locations(
    config: &AppConfig,
    local_timezone: &str,
    reference_utc: DateTime<Utc>,
) -> Vec<WeatherLocation> {
    let (_, entries) = visible_location_entries(config, reference_utc, local_timezone);
    entries
        .into_iter()
        .filter_map(|entry| {
            let (latitude, longitude) = place_coordinate(&entry)?;
            let label = entry.display_label();
            Some(WeatherLocation {
                timezone: entry.timezone,
                label,
                latitude,
                longitude,
            })
        })
        .collect()
}

fn parse_weather_response(
    raw: &str,
    locations: &[WeatherLocation],
) -> Result<Vec<LocationWeather>> {
    let response = serde_json::from_str::<OpenMeteoResponse>(raw)
        .context("Open-Meteo returned invalid weather data")?;
    let forecasts = match response {
        OpenMeteoResponse::One(forecast) => vec![forecast],
        OpenMeteoResponse::Many(forecasts) => forecasts,
    };
    if forecasts.len() != locations.len() {
        bail!(
            "Open-Meteo returned {} locations for {} requests",
            forecasts.len(),
            locations.len()
        );
    }

    Ok(locations
        .iter()
        .zip(forecasts)
        .filter_map(|(location, forecast)| {
            let current = forecast.current?;
            if !current.temperature_2m.is_finite() {
                return None;
            }
            Some(LocationWeather {
                timezone: location.timezone.clone(),
                label: location.label.clone(),
                temperature_celsius: current.temperature_2m,
                weather_code: current.weather_code,
                condition: weather_condition(current.weather_code),
                is_day: current.is_day != 0,
            })
        })
        .collect())
}

fn weather_condition(code: i64) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 | 77 => "Snow",
        80..=82 => "Showers",
        85..=86 => "Snow showers",
        95 | 96 | 99 => "Thunderstorm",
        _ => "Conditions unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_weather_response, weather_condition, weather_locations, WeatherLocation};
    use crate::config::{AppConfig, LocationKey, TimezoneEntry, CLOCK_CARD_LIMIT};
    use crate::quattro::build_snapshot;
    use chrono::{TimeZone, Utc};

    fn location(timezone: &str, label: &str) -> WeatherLocation {
        WeatherLocation {
            timezone: timezone.to_string(),
            label: label.to_string(),
            latitude: 0.0,
            longitude: 0.0,
        }
    }

    #[test]
    fn weather_codes_have_compact_condition_labels() {
        assert_eq!(weather_condition(0), "Clear");
        assert_eq!(weather_condition(2), "Partly cloudy");
        assert_eq!(weather_condition(63), "Rain");
        assert_eq!(weather_condition(86), "Snow showers");
        assert_eq!(weather_condition(95), "Thunderstorm");
        assert_eq!(weather_condition(500), "Conditions unavailable");
    }

    #[test]
    fn single_location_weather_preserves_location_identity() {
        let locations = vec![location("America/Cancun", "Home")];
        let weather = parse_weather_response(
            r#"{"current":{"temperature_2m":29.4,"weather_code":1,"is_day":1}}"#,
            &locations,
        )
        .unwrap();

        assert_eq!(weather.len(), 1);
        assert_eq!(weather[0].timezone, "America/Cancun");
        assert_eq!(weather[0].label, "Home");
        assert_eq!(weather[0].temperature_celsius, 29.4);
        assert_eq!(weather[0].condition, "Mostly clear");
        assert!(weather[0].is_day);
    }

    #[test]
    fn batched_weather_maps_responses_by_request_order_and_skips_missing_current_data() {
        let locations = vec![
            location("America/Cancun", "Home"),
            location("Europe/Paris", "Rennes"),
            location("Asia/Tokyo", "Tokyo"),
        ];
        let weather = parse_weather_response(
            r#"[
              {"current":{"temperature_2m":30.1,"weather_code":2,"is_day":1}},
              {"current":{"temperature_2m":18.6,"weather_code":61,"is_day":0}},
              {}
            ]"#,
            &locations,
        )
        .unwrap();

        assert_eq!(weather.len(), 2);
        assert_eq!(weather[0].label, "Home");
        assert_eq!(weather[1].label, "Rennes");
        assert_eq!(weather[1].condition, "Rain");
        assert!(!weather[1].is_day);
    }

    #[test]
    fn batched_weather_rejects_a_response_count_mismatch() {
        let locations = vec![
            location("America/Cancun", "Home"),
            location("Europe/Paris", "Rennes"),
        ];
        let error = parse_weather_response(
            r#"{"current":{"temperature_2m":29.4,"weather_code":1,"is_day":1}}"#,
            &locations,
        )
        .unwrap_err();

        assert!(error.to_string().contains("1 locations for 2 requests"));
    }

    #[test]
    fn weather_requests_match_the_bounded_visible_snapshot() {
        let configured = [
            ("America/Cancun", "Cancun"),
            ("America/Vancouver", "Vancouver"),
            ("America/Los_Angeles", "Los Angeles"),
            ("America/Denver", "Denver"),
            ("America/Chicago", "Chicago"),
            ("America/New_York", "New York"),
            ("America/Halifax", "Halifax"),
            ("Europe/London", "London"),
            ("Europe/Paris", "Paris"),
            ("Asia/Kolkata", "Delhi"),
            ("Asia/Tokyo", "Tokyo"),
        ];
        let config = AppConfig {
            timezones: configured
                .iter()
                .enumerate()
                .map(|(index, (timezone, label))| TimezoneEntry {
                    timezone: (*timezone).to_string(),
                    label: (*label).to_string(),
                    latitude: Some(index as f64),
                    longitude: Some(index as f64),
                })
                .collect(),
            pinned_location: Some(LocationKey {
                timezone: "Asia/Tokyo".to_string(),
                label: "Tokyo".to_string(),
            }),
            disable_open_meteo_geolocation: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 21, 15, 0, 0).unwrap();

        let requested = weather_locations(&config, "America/Cancun", now);
        let snapshot = build_snapshot(&config, now, "America/Cancun", "24h");
        let expected = std::iter::once(&snapshot.summary)
            .chain(snapshot.clocks.iter())
            .map(|clock| (clock.timezone.clone(), clock.label.clone()))
            .collect::<Vec<_>>();
        let actual = requested
            .iter()
            .map(|location| (location.timezone.clone(), location.label.clone()))
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert_eq!(requested.len(), CLOCK_CARD_LIMIT + 1);
        assert!(requested.len() < config.timezones.len());
        assert!(requested.iter().any(|location| location.label == "Tokyo"));
    }
}
