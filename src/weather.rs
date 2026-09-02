use crate::config::{place_coordinate, AppConfig};
use crate::quattro::visible_location_entries;
use crate::remote_response::{
    open_meteo_client, read_json_response, MAX_OPEN_METEO_RESPONSE_BYTES,
};
use crate::time::{parse_timezone, zoned_datetime};
use anyhow::{bail, Context, Result};
use chrono::{
    DateTime, Duration as ChronoDuration, FixedOffset, LocalResult, NaiveDateTime, TimeZone, Utc,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPEN_METEO_FORECAST_ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";
const OPEN_METEO_ATTRIBUTION_URL: &str = "https://open-meteo.com/";
// Rich current, hourly, and daily forecasts are still comfortably below the
// shared 64 KiB response ceiling at this batch size. Keeping the batch bounded
// also prevents one unusually verbose upstream response from discarding every
// visible location at once.
const OPEN_METEO_BATCH_SIZE: usize = 20;
const HOURLY_FORECAST_COUNT: usize = 8;
const DAILY_FORECAST_COUNT: usize = 4;

#[derive(Debug, Clone, PartialEq)]
struct WeatherLocation {
    timezone: String,
    label: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DailyWeather {
    pub date: String,
    pub temperature_max_celsius: f64,
    pub temperature_min_celsius: f64,
    pub weather_code: i64,
    pub condition: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precipitation_probability_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunrise: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv_index_max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourlyWeather {
    pub time: String,
    pub reference_utc: String,
    pub temperature_celsius: f64,
    pub weather_code: i64,
    pub condition: &'static str,
    pub is_day: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precipitation_probability_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocationWeather {
    pub timezone: String,
    pub label: String,
    pub temperature_celsius: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apparent_temperature_celsius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_humidity_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_speed_kmh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_direction_degrees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_gusts_kmh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precipitation_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure_hpa: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility_meters: Option<f64>,
    pub weather_code: i64,
    pub condition: &'static str,
    pub is_day: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today: Option<DailyWeather>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hourly_forecast: Vec<HourlyWeather>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forecast: Vec<DailyWeather>,
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
    apparent_temperature: Option<f64>,
    relative_humidity_2m: Option<f64>,
    wind_speed_10m: Option<f64>,
    wind_direction_10m: Option<f64>,
    wind_gusts_10m: Option<f64>,
    precipitation: Option<f64>,
    pressure_msl: Option<f64>,
    visibility: Option<f64>,
    weather_code: i64,
    is_day: i64,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    temperature_2m: Vec<f64>,
    weather_code: Vec<i64>,
    is_day: Vec<i64>,
    #[serde(default)]
    precipitation_probability: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoDaily {
    time: Vec<String>,
    weather_code: Vec<i64>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
    #[serde(default)]
    sunrise: Vec<Option<String>>,
    #[serde(default)]
    sunset: Vec<Option<String>>,
    #[serde(default)]
    uv_index_max: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoForecast {
    utc_offset_seconds: Option<i32>,
    current: Option<OpenMeteoCurrent>,
    hourly: Option<OpenMeteoHourly>,
    daily: Option<OpenMeteoDaily>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenMeteoResponse {
    One(Box<OpenMeteoForecast>),
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

    let client = open_meteo_client(Duration::from_secs(5))
        .context("could not initialize the weather client")?;
    for batch in locations.chunks(OPEN_METEO_BATCH_SIZE) {
        let latitudes = batch
            .iter()
            .map(|location| location.latitude.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let longitudes = batch
            .iter()
            .map(|location| location.longitude.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let response = fetch_weather_response(
            &client,
            OPEN_METEO_FORECAST_ENDPOINT,
            &latitudes,
            &longitudes,
        )?;
        payload
            .locations
            .extend(weather_from_response(response, batch, reference_utc)?);
    }
    Ok(payload)
}

fn fetch_weather_response(
    client: &Client,
    endpoint: &str,
    latitudes: &str,
    longitudes: &str,
) -> Result<OpenMeteoResponse> {
    let response = client
        .get(endpoint)
        .query(&[
            ("latitude", latitudes),
            ("longitude", longitudes),
            (
                "current",
                "temperature_2m,apparent_temperature,relative_humidity_2m,precipitation,pressure_msl,visibility,wind_speed_10m,wind_direction_10m,wind_gusts_10m,weather_code,is_day",
            ),
            (
                "hourly",
                "temperature_2m,precipitation_probability,weather_code,is_day",
            ),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset,uv_index_max",
            ),
            ("temperature_unit", "celsius"),
            ("wind_speed_unit", "kmh"),
            ("forecast_hours", "8"),
            ("forecast_days", "5"),
            ("timezone", "auto"),
        ])
        .send()
        .context("could not reach Open-Meteo")?;
    if !response.status().is_success() {
        bail!("Open-Meteo returned HTTP status {}", response.status());
    }
    let response = read_json_response::<OpenMeteoResponse>(response, MAX_OPEN_METEO_RESPONSE_BYTES)
        .context("could not read the Open-Meteo response")?;

    Ok(response)
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

#[cfg(test)]
fn parse_weather_response(
    raw: &str,
    locations: &[WeatherLocation],
    reference_utc: DateTime<Utc>,
) -> Result<Vec<LocationWeather>> {
    let response = serde_json::from_str::<OpenMeteoResponse>(raw)
        .context("Open-Meteo returned invalid weather data")?;
    weather_from_response(response, locations, reference_utc)
}

fn weather_from_response(
    response: OpenMeteoResponse,
    locations: &[WeatherLocation],
    reference_utc: DateTime<Utc>,
) -> Result<Vec<LocationWeather>> {
    let forecasts = match response {
        OpenMeteoResponse::One(forecast) => vec![*forecast],
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
            let utc_offset_seconds = forecast.utc_offset_seconds;
            let today = forecast
                .daily
                .as_ref()
                .and_then(|daily| daily_weather_at(daily, 0));
            let daily_forecast = forecast
                .daily
                .as_ref()
                .map(|daily| {
                    (1..daily.time.len())
                        .take(DAILY_FORECAST_COUNT)
                        .filter_map(|index| daily_weather_at(daily, index))
                        .collect()
                })
                .unwrap_or_default();
            let hourly_forecast = forecast
                .hourly
                .as_ref()
                .map(|hourly| {
                    let count = hourly
                        .time
                        .len()
                        .min(hourly.temperature_2m.len())
                        .min(hourly.weather_code.len())
                        .min(hourly.is_day.len());
                    (0..count)
                        .take(HOURLY_FORECAST_COUNT)
                        .filter_map(|index| {
                            hourly_weather_at(
                                hourly,
                                index,
                                &location.timezone,
                                utc_offset_seconds,
                                reference_utc,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(LocationWeather {
                timezone: location.timezone.clone(),
                label: location.label.clone(),
                temperature_celsius: current.temperature_2m,
                apparent_temperature_celsius: current
                    .apparent_temperature
                    .filter(|value| value.is_finite()),
                relative_humidity_percent: current
                    .relative_humidity_2m
                    .filter(|value| value.is_finite()),
                wind_speed_kmh: current.wind_speed_10m.filter(|value| value.is_finite()),
                wind_direction_degrees: current
                    .wind_direction_10m
                    .filter(|value| value.is_finite()),
                wind_gusts_kmh: current.wind_gusts_10m.filter(|value| value.is_finite()),
                precipitation_mm: current.precipitation.filter(|value| value.is_finite()),
                pressure_hpa: current.pressure_msl.filter(|value| value.is_finite()),
                visibility_meters: current.visibility.filter(|value| value.is_finite()),
                weather_code: current.weather_code,
                condition: weather_condition(current.weather_code),
                is_day: current.is_day != 0,
                today,
                hourly_forecast,
                forecast: daily_forecast,
            })
        })
        .collect())
}

fn optional_finite(values: &[Option<f64>], index: usize) -> Option<f64> {
    values
        .get(index)
        .copied()
        .flatten()
        .filter(|value| value.is_finite())
}

fn daily_weather_at(daily: &OpenMeteoDaily, index: usize) -> Option<DailyWeather> {
    let maximum = *daily.temperature_2m_max.get(index)?;
    let minimum = *daily.temperature_2m_min.get(index)?;
    let weather_code = *daily.weather_code.get(index)?;
    if !maximum.is_finite() || !minimum.is_finite() {
        return None;
    }
    Some(DailyWeather {
        date: daily.time.get(index)?.clone(),
        temperature_max_celsius: maximum,
        temperature_min_celsius: minimum,
        weather_code,
        condition: weather_condition(weather_code),
        precipitation_probability_percent: optional_finite(
            &daily.precipitation_probability_max,
            index,
        ),
        sunrise: daily
            .sunrise
            .get(index)
            .and_then(Option::as_ref)
            .filter(|value| !value.is_empty())
            .cloned(),
        sunset: daily
            .sunset
            .get(index)
            .and_then(Option::as_ref)
            .filter(|value| !value.is_empty())
            .cloned(),
        uv_index_max: optional_finite(&daily.uv_index_max, index),
    })
}

fn hourly_weather_at(
    hourly: &OpenMeteoHourly,
    index: usize,
    timezone: &str,
    utc_offset_seconds: Option<i32>,
    reference_utc: DateTime<Utc>,
) -> Option<HourlyWeather> {
    let temperature = *hourly.temperature_2m.get(index)?;
    let weather_code = *hourly.weather_code.get(index)?;
    if !temperature.is_finite() {
        return None;
    }
    let expected_utc = reference_utc + ChronoDuration::hours(i64::try_from(index).ok()?);
    let reference_utc = hourly_reference_utc(
        hourly.time.get(index)?,
        timezone,
        utc_offset_seconds,
        expected_utc,
    )?;
    Some(HourlyWeather {
        time: zoned_datetime(reference_utc, timezone)
            .format("%Y-%m-%dT%H:%M")
            .to_string(),
        reference_utc: reference_utc.to_rfc3339(),
        temperature_celsius: temperature,
        weather_code,
        condition: weather_condition(weather_code),
        is_day: *hourly.is_day.get(index)? != 0,
        precipitation_probability_percent: optional_finite(
            &hourly.precipitation_probability,
            index,
        ),
    })
}

fn hourly_reference_utc(
    local_time: &str,
    timezone: &str,
    utc_offset_seconds: Option<i32>,
    expected_utc: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let local = NaiveDateTime::parse_from_str(local_time, "%Y-%m-%dT%H:%M").ok()?;

    // Open-Meteo renders every ISO timestamp in one response with the
    // response's applied offset. Reversing that offset preserves fractional
    // zones and the elapsed-hour sequence across a clock change. Fall back to
    // tzdb for fixtures or compatible responses that omit the offset.
    if let Some(offset) = utc_offset_seconds.and_then(FixedOffset::east_opt) {
        if let LocalResult::Single(value) = offset.from_local_datetime(&local) {
            return Some(value.with_timezone(&Utc));
        }
    }

    match parse_timezone(timezone)?.from_local_datetime(&local) {
        LocalResult::Single(value) => Some(value.with_timezone(&Utc)),
        LocalResult::Ambiguous(earlier, later) => {
            let earlier = earlier.with_timezone(&Utc);
            let later = later.with_timezone(&Utc);
            let earlier_distance = (earlier.timestamp() - expected_utc.timestamp()).abs();
            let later_distance = (later.timestamp() - expected_utc.timestamp()).abs();
            Some(if earlier_distance <= later_distance {
                earlier
            } else {
                later
            })
        }
        LocalResult::None => None,
    }
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
    use super::{
        fetch_weather_response, parse_weather_response, weather_condition, weather_locations,
        WeatherLocation,
    };
    use crate::config::{AppConfig, LocationKey, TimezoneEntry};
    use crate::quattro::build_snapshot;
    use crate::remote_response::{
        open_meteo_client, serve_http_redirect_to_response, serve_http_response_without_length,
        MAX_OPEN_METEO_RESPONSE_BYTES,
    };
    use chrono::{TimeZone, Utc};
    use reqwest::blocking::Client;
    use std::time::Duration;

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
            r#"{
              "utc_offset_seconds": -18000,
              "current": {
                "temperature_2m": 29.4,
                "apparent_temperature": 33.1,
                "relative_humidity_2m": 72,
                "wind_speed_10m": 14.8,
                "wind_direction_10m": 82,
                "wind_gusts_10m": 24.1,
                "precipitation": 0.2,
                "pressure_msl": 1013.6,
                "visibility": 24140,
                "weather_code": 1,
                "is_day": 1
              },
              "hourly": {
                "time": [
                  "2026-08-21T10:00", "2026-08-21T11:00",
                  "2026-08-21T12:00", "2026-08-21T13:00"
                ],
                "temperature_2m": [29.4, 30.1, 30.8, 31.0],
                "precipitation_probability": [8, 12, 20, 18],
                "weather_code": [1, 2, 61, 2],
                "is_day": [1, 1, 1, 1]
              },
              "daily": {
                "time": ["2026-08-21", "2026-08-22", "2026-08-23", "2026-08-24", "2026-08-25"],
                "weather_code": [1, 2, 61, 3, 80],
                "temperature_2m_max": [31.2, 30.4, 28.8, 29.1, 30.0],
                "temperature_2m_min": [26.0, 25.5, 24.9, 25.1, 25.0],
                "precipitation_probability_max": [18, 35, 82, 14, 65],
                "sunrise": ["2026-08-21T06:28", "2026-08-22T06:28", "2026-08-23T06:29", "2026-08-24T06:29", "2026-08-25T06:29"],
                "sunset": ["2026-08-21T19:12", "2026-08-22T19:11", "2026-08-23T19:10", "2026-08-24T19:09", "2026-08-25T19:08"],
                "uv_index_max": [8.2, 7.8, 4.1, 8.0, 6.3]
              }
            }"#,
            &locations,
            Utc.with_ymd_and_hms(2026, 8, 21, 15, 45, 0).unwrap(),
        )
        .unwrap();

        assert_eq!(weather.len(), 1);
        assert_eq!(weather[0].timezone, "America/Cancun");
        assert_eq!(weather[0].label, "Home");
        assert_eq!(weather[0].temperature_celsius, 29.4);
        assert_eq!(weather[0].apparent_temperature_celsius, Some(33.1));
        assert_eq!(weather[0].relative_humidity_percent, Some(72.0));
        assert_eq!(weather[0].wind_speed_kmh, Some(14.8));
        assert_eq!(weather[0].wind_direction_degrees, Some(82.0));
        assert_eq!(weather[0].wind_gusts_kmh, Some(24.1));
        assert_eq!(weather[0].precipitation_mm, Some(0.2));
        assert_eq!(weather[0].pressure_hpa, Some(1013.6));
        assert_eq!(weather[0].visibility_meters, Some(24140.0));
        assert_eq!(weather[0].condition, "Mostly clear");
        assert!(weather[0].is_day);
        let today = weather[0].today.as_ref().unwrap();
        assert_eq!(today.temperature_max_celsius, 31.2);
        assert_eq!(today.precipitation_probability_percent, Some(18.0));
        assert_eq!(today.sunrise.as_deref(), Some("2026-08-21T06:28"));
        assert_eq!(today.sunset.as_deref(), Some("2026-08-21T19:12"));
        assert_eq!(today.uv_index_max, Some(8.2));
        assert_eq!(weather[0].hourly_forecast.len(), 4);
        assert_eq!(weather[0].hourly_forecast[0].time, "2026-08-21T10:00");
        assert_eq!(
            weather[0].hourly_forecast[0].reference_utc,
            "2026-08-21T15:00:00+00:00"
        );
        assert_eq!(
            weather[0].hourly_forecast[2].precipitation_probability_percent,
            Some(20.0)
        );
        assert_eq!(weather[0].hourly_forecast[2].condition, "Rain");
        assert_eq!(weather[0].forecast.len(), 4);
        assert_eq!(weather[0].forecast[0].date, "2026-08-22");
        assert_eq!(weather[0].forecast[0].temperature_max_celsius, 30.4);
        assert_eq!(weather[0].forecast[3].weather_code, 80);
    }

    #[test]
    fn hourly_weather_preserves_fractional_timezone_boundaries() {
        let locations = vec![location("Asia/Kolkata", "New Delhi")];
        let weather = parse_weather_response(
            r#"{
              "utc_offset_seconds": 19800,
              "current": {
                "temperature_2m": 31.0,
                "weather_code": 2,
                "is_day": 1
              },
              "hourly": {
                "time": ["2026-08-21T16:00", "2026-08-21T17:00"],
                "temperature_2m": [31.0, 30.0],
                "weather_code": [2, 3],
                "is_day": [1, 1]
              }
            }"#,
            &locations,
            Utc.with_ymd_and_hms(2026, 8, 21, 10, 45, 0).unwrap(),
        )
        .unwrap();

        let hourly = &weather[0].hourly_forecast;
        assert_eq!(hourly[0].time, "2026-08-21T16:00");
        assert_eq!(hourly[0].reference_utc, "2026-08-21T10:30:00+00:00");
        assert_eq!(hourly[1].time, "2026-08-21T17:00");
        assert_eq!(hourly[1].reference_utc, "2026-08-21T11:30:00+00:00");
    }

    #[test]
    fn daily_weather_accepts_missing_sunrise_and_sunset() {
        let locations = vec![location("Arctic/Longyearbyen", "Longyearbyen")];
        let weather = parse_weather_response(
            r#"{
              "current": {
                "temperature_2m": -8.0,
                "weather_code": 3,
                "is_day": 0
              },
              "daily": {
                "time": ["2026-12-21"],
                "weather_code": [3],
                "temperature_2m_max": [-6.0],
                "temperature_2m_min": [-12.0],
                "sunrise": [null],
                "sunset": [null]
              }
            }"#,
            &locations,
            Utc.with_ymd_and_hms(2026, 12, 21, 12, 0, 0).unwrap(),
        )
        .unwrap();

        let today = weather[0].today.as_ref().unwrap();
        assert_eq!(today.sunrise, None);
        assert_eq!(today.sunset, None);
    }

    #[test]
    fn hourly_weather_disambiguates_a_repeated_fall_back_hour() {
        let locations = vec![location("America/New_York", "New York")];
        let weather = parse_weather_response(
            r#"{
              "utc_offset_seconds": -14400,
              "current": {
                "temperature_2m": 12.0,
                "weather_code": 2,
                "is_day": 0
              },
              "hourly": {
                "time": [
                  "2026-11-01T00:00", "2026-11-01T01:00",
                  "2026-11-01T02:00", "2026-11-01T03:00"
                ],
                "temperature_2m": [12.0, 11.0, 10.0, 9.0],
                "weather_code": [2, 2, 3, 3],
                "is_day": [0, 0, 0, 0]
              }
            }"#,
            &locations,
            Utc.with_ymd_and_hms(2026, 11, 1, 4, 30, 0).unwrap(),
        )
        .unwrap();

        let hourly = &weather[0].hourly_forecast;
        assert_eq!(
            hourly
                .iter()
                .map(|item| item.time.as_str())
                .collect::<Vec<_>>(),
            [
                "2026-11-01T00:00",
                "2026-11-01T01:00",
                "2026-11-01T01:00",
                "2026-11-01T02:00",
            ]
        );
        assert_eq!(hourly[1].reference_utc, "2026-11-01T05:00:00+00:00");
        assert_eq!(hourly[2].reference_utc, "2026-11-01T06:00:00+00:00");
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
            Utc.with_ymd_and_hms(2026, 8, 21, 15, 0, 0).unwrap(),
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
            Utc.with_ymd_and_hms(2026, 8, 21, 15, 0, 0).unwrap(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("1 locations for 2 requests"));
    }

    #[test]
    fn weather_rejects_an_oversized_unknown_length_response() {
        let body = format!(
            r#"{{"padding":"{}"}}"#,
            "x".repeat(MAX_OPEN_METEO_RESPONSE_BYTES)
        )
        .into_bytes();
        let (endpoint, server) = serve_http_response_without_length(body);
        let client = Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();

        let error = fetch_weather_response(&client, &endpoint, "0", "0").unwrap_err();
        server.join().unwrap();

        assert!(format!("{error:#}").contains("exceeds 65536-byte limit"));
    }

    #[test]
    fn weather_does_not_follow_redirects() {
        let (endpoint, redirect_server, stop_target, target_server) =
            serve_http_redirect_to_response(
                br#"{"current":{"temperature_2m":20,"weather_code":0,"is_day":1}}"#.to_vec(),
            );
        let client = open_meteo_client(Duration::from_secs(1)).unwrap();

        let result = fetch_weather_response(&client, &endpoint, "0", "0");
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
    fn weather_requests_match_the_complete_visible_snapshot() {
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
            pinned_locations: vec![LocationKey {
                timezone: "Asia/Tokyo".to_string(),
                label: "Tokyo".to_string(),
            }],
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
        assert_eq!(requested.len(), config.timezones.len());
        assert!(requested.iter().any(|location| location.label == "Tokyo"));
    }
}
