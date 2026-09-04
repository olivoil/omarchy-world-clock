---
name: omarchy-world-clock
description: Query the installed Omarchy World Clock by saved personal label or place for current and converted times, local forecasts, and shared working-hour overlap. Use when a question refers to a person, team, or place that may be represented by a World Clock entry. Do not use for arbitrary unsaved locations unless the user asks to add one.
---

# Omarchy World Clock

Use the packaged `scripts/world-clock` helper. It resolves to the bundled,
version-matched backend and prints a versioned JSON response. Do not parse or
edit `~/.config/omarchy-world-clock/config.json` directly; loading through the
CLI applies the plugin's migrations and canonical timezone rules.

## Resolve people and places

Start with:

```bash
scripts/world-clock places
```

Map the user's wording against `custom_label` first, then `label`, `place`, and
`timezone`. The focused commands perform the same lookup:

```bash
scripts/world-clock time --location "Jeff"
scripts/world-clock time --id 3
```

Use an ID after inspecting `places` when labels are duplicated. Never choose
silently after the CLI reports an ambiguous match. `local` is the explicit
alias for the user's current system timezone.

A group name is not stored as membership metadata. If the requested group's
members are not named in the prompt or unambiguously represented by saved
labels, ask which World Clock entries belong to it rather than guessing.

## Convert a user's reference time

Use `convert` whenever the user gives a wall-clock time. It handles timezone
dates and daylight-saving transitions and returns one canonical
`reference_utc` for every location.

```bash
scripts/world-clock convert --from local --time "2pm"
scripts/world-clock convert --from "Jeff" --time "2026-09-08 09:30"
```

For future-oriented wording with only a time (for example, “tomorrow at 2”),
make the date explicit before calling the CLI. Do not assume that today's past
occurrence is intended.

## Weather at a time

First convert the requested wall-clock time, then pass that response's
`reference_utc` to a forecast query for the resolved location:

```bash
scripts/world-clock forecast --id 3 --at "2026-09-04T19:00:00Z"
```

Use `forecast_at` for the requested hour. If it is null, the instant is outside
the returned hourly horizon; use the daily data only for a day-level answer
and say that an hour-specific forecast is unavailable. Preserve the provider
attribution in an answer based on forecast data. Source values are Celsius and
km/h; format temperatures and wind for `temperature_unit_preference`,
converting to Fahrenheit and mph when it is `imperial`. A `disabled` status
means the user opted out of Open-Meteo; do not bypass that preference with
another network source unless they explicitly ask. For `no_coordinates` or
`unavailable`, explain the limitation instead of inventing a forecast.

## Find shared working time

Pass every participant explicitly. Include `local` when “everyone” includes
the user:

```bash
scripts/world-clock overlap \
  --location local --location "Jeff" --location "Jenny" \
  --date 2026-09-08 --days 5 \
  --work-start 09:00 --work-end 17:00 --duration-minutes 60
```

When the user only says “reasonable work hours,” the command defaults to
09:00–17:00 in each participant's timezone; state that assumption. Report
useful ranges from `windows`, not every 15-minute boundary. The output is
DST-aware and includes each participant's local start and end.

For the complete command and response contract, read
[references/cli.md](references/cli.md).
