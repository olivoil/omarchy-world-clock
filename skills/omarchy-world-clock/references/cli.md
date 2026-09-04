# Agent CLI contract

Invoke commands through `../scripts/world-clock` from this reference directory,
through `scripts/world-clock` from the skill root, or through
`omarchy-world-clock` when the optional system integration is installed.

All successful data commands print one JSON object. `api_version` is currently
`1`. Diagnostics go to stderr and return a nonzero status. `--at` and `--base`
arguments use RFC 3339 unless the command specifically documents a wall-clock
value.

## `places [--at <RFC3339>]`

Returns the system-local summary followed by every saved card. Each location
contains:

- `id`, `configured`, and `is_local`
- `timezone`, geographic `place`, display `label`, and `custom_label`
- `local_datetime`, `date`, display `time`, abbreviation, and UTC offset
- effective coordinates and `pinned`

The transient local summary can have ID `0` when the current system timezone
is not a saved card. Address it as `local`, not by ID.

## `time (--location <name> | --id <id>) [--at <RFC3339>]`

Returns one resolved location at now or at an exact instant. Matching is
case/diacritic insensitive and prioritizes an exact custom label. An ambiguous
match fails and lists candidate IDs.

## `convert`

```text
convert --time <value> [--from <name> | --from-id <id>] [--base <RFC3339>]
```

The source defaults to `local`. `value` accepts `HH:MM`, compact time such as
`830`, quarter-hour decimal shorthand, meridiem time, or
`YYYY-MM-DD HH:MM`. A time without a date uses the source location's calendar
date at `base` (now by default). The response provides `reference_utc`, the
resolved source, and every saved location at that instant.

## `forecast`

```text
forecast (--location <name> | --id <id>) [--at <RFC3339>]
```

Fetches only the selected saved location. `weather` contains current,
next-hourly, and daily Open-Meteo data in Celsius/km/h source units;
`temperature_unit_preference` reports the effective Omarchy display preference.
For `imperial`, convert temperatures with `°F = °C × 9/5 + 32` and wind with
`mph = km/h × 0.621371`; otherwise retain the source units.
When `--at` is supplied, `forecast_at` is the hourly record containing that
instant, or null when it is outside the hourly response.

Status values:

- `ok`: weather data is present
- `disabled`: `disable_open_meteo_geolocation` is enabled
- `no_coordinates`: the selected clock cannot be mapped to a weather point
- `unavailable`: the provider returned no usable conditions for a mapped point

Network/provider failure returns nonzero rather than fabricating data.

## `overlap`

```text
overlap (--location <name> | --id <id>)...
  [--date <YYYY-MM-DD>] [--days <1-31>]
  [--work-start <HH:MM>] [--work-end <HH:MM>]
  [--duration-minutes <minutes>]
```

The date is interpreted in the system-local timezone. Defaults are today,
seven days, 09:00–17:00 for every participant, a 30-minute minimum, and
15-minute calculation granularity. Work-hour ranges may cross midnight.
`windows` contains continuous qualifying ranges in UTC, system-local time, and
every participant's timezone. Empty `windows` means no qualifying range was
found under those constraints.
