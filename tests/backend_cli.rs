use std::fs;
use std::process::Command;

const BACKEND_PROTOCOL_VERSION: u64 = 3;

#[test]
fn package_exposes_only_the_quattro_backend_binary() {
    assert!(
        option_env!("CARGO_BIN_EXE_omarchy-world-clock").is_none(),
        "new releases must not ship the legacy GTK/Waybar executable"
    );
}

#[test]
fn bundled_backend_reports_its_protocol_and_version() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let config_path = sandbox.path().join("config.json");
    fs::write(
        &config_path,
        r#"{
  "version": 6,
  "pinned_location": {
    "timezone": "Asia/Tokyo",
    "label": "Tokyo"
  },
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    { "timezone": "Asia/Tokyo", "label": "Tokyo" }
  ]
}
"#,
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_omarchy-world-clock-backend"))
        .arg("module")
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("run bundled backend");

    assert!(output.status.success());
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse module payload");
    assert_eq!(payload["protocol_version"], BACKEND_PROTOCOL_VERSION);
    assert_eq!(payload["backend_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["pinned_label"], "Tokyo");
    assert!(payload["pinned_time"]
        .as_str()
        .is_some_and(|time| !time.is_empty()));
}

#[test]
fn bundled_backend_supports_the_complete_quattro_command_protocol() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let config_path = sandbox.path().join("config.json");
    let shell_config = home.join(".config/omarchy/shell.json");
    fs::create_dir_all(shell_config.parent().expect("shell config parent"))
        .expect("create shell config directory");
    fs::write(
        &shell_config,
        r#"{
  "version": 1,
  "bar": {
    "layout": {
      "left": [],
      "center": [
        { "id": "omarchy.clock", "format": "HH:mm" },
        { "id": "omarchy.weather", "unit": "metric" }
      ],
      "right": []
    }
  }
}
"#,
    )
    .expect("write shell config");
    fs::write(
        &config_path,
        r#"{
  "version": 6,
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    { "timezone": "Asia/Tokyo", "label": "Tokyo" }
  ]
}
"#,
    )
    .expect("write config");
    let backend = env!("CARGO_BIN_EXE_omarchy-world-clock-backend");

    let pin = Command::new(backend)
        .args(["pin", "Asia/Tokyo", "--label", "Tokyo"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("pin timezone");
    assert!(pin.success());

    let snapshot = Command::new(backend)
        .args(["snapshot", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot");
    assert!(snapshot.status.success());
    let snapshot: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("parse snapshot");
    assert_eq!(snapshot["schema_version"], 1);
    assert_eq!(snapshot["pinned_timezone"], "Asia/Tokyo");
    assert_eq!(snapshot["configured_count"], 2);
    assert_eq!(snapshot["weather_unit"], "metric");
    assert!(snapshot["featured_cities"]
        .as_array()
        .is_some_and(|cities| !cities.is_empty()));
    assert!(!snapshot["featured_cities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|city| city["title"] == "Tokyo"));
    assert!(snapshot["featured_cities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|city| city["timezone"] == "Asia/Tokyo" && city["title"] != "Tokyo"));

    let scrub = Command::new(backend)
        .args(["scrub", "--timezone", "UTC", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render scrub day");
    assert!(scrub.status.success());
    let scrub: serde_json::Value = serde_json::from_slice(&scrub.stdout).expect("parse scrub day");
    assert_eq!(scrub["schema_version"], 1);
    assert_eq!(scrub["source_timezone"], "UTC");
    assert_eq!(scrub["date"], "2026-08-11");
    assert_eq!(scrub["time_format"], "24h");
    assert_eq!(scrub["step_minutes"], 15);
    assert_eq!(scrub["first_day_offset"], -1);
    assert_eq!(scrub["day_count"], 3);
    assert_eq!(scrub["slots"].as_array().map(Vec::len), Some(288));
    assert_eq!(scrub["slots"][140]["day_offset"], 0);
    assert_eq!(scrub["slots"][140]["minute"], 660);
    assert_eq!(scrub["slots"][140]["label"], "11:00");
    assert_eq!(
        scrub["slots"][140]["reference_utc"],
        "2026-08-11T11:00:00+00:00"
    );

    let conversion = Command::new(backend)
        .args([
            "convert",
            "--timezone",
            "Asia/Tokyo",
            "--value",
            "9am",
            "--base",
            "2026-08-11T11:05:00Z",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("convert time");
    assert!(conversion.status.success());
    let conversion: serde_json::Value =
        serde_json::from_slice(&conversion.stdout).expect("parse conversion");
    assert_eq!(conversion["normalized_input"], "09:00");
    assert_eq!(conversion["snapshot"]["weather_unit"], "metric");

    let search = Command::new(backend)
        .args(["search", "Tokyo", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("search timezones");
    assert!(search.status.success());
    let search: serde_json::Value =
        serde_json::from_slice(&search.stdout).expect("parse search results");
    assert_eq!(search[0]["timezone"], "Asia/Tokyo");
    assert_eq!(search[0]["time"], "20:05");
    assert!(search[0]["day"].as_str().is_some_and(|day| !day.is_empty()));
    assert!(search[0]["relative_label"]
        .as_str()
        .is_some_and(|label| !label.is_empty()));

    let locate = Command::new(backend)
        .args([
            "locate",
            "--latitude",
            "48.11109",
            "--longitude",
            "-1.67431",
            "--at",
            "2026-08-11T11:05:00Z",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("locate timezone from map coordinate");
    assert!(locate.status.success());
    let locate: serde_json::Value =
        serde_json::from_slice(&locate.stdout).expect("parse map location");
    assert_eq!(locate["timezone"], "Europe/Paris");
    assert_eq!(locate["latitude"], 48.11109);
    assert_eq!(locate["longitude"], -1.67431);
    assert_eq!(locate["time"], "13:05");

    let add = Command::new(backend)
        .args([
            "add",
            "America/New_York",
            "--label",
            "Boston",
            "--latitude",
            "42.3601",
            "--longitude",
            "-71.0589",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("add location");
    assert!(add.success());

    let add_flag_named_label = Command::new(backend)
        .args(["add", "Europe/London", "--label", "--new-label"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("add location with a flag-like label");
    assert!(add_flag_named_label.success());

    let rename_flag_named_label = Command::new(backend)
        .args([
            "rename",
            "Europe/London",
            "--label",
            "--new-label",
            "--new-label",
            "Office",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("rename location whose label resembles a flag");
    assert!(rename_flag_named_label.success());

    let renamed_flag_snapshot = Command::new(backend)
        .args(["snapshot", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot after renaming a flag-like label");
    assert!(renamed_flag_snapshot.status.success());
    let renamed_flag_snapshot: serde_json::Value =
        serde_json::from_slice(&renamed_flag_snapshot.stdout)
            .expect("parse renamed flag-like label snapshot");
    let renamed_flag_clock = renamed_flag_snapshot["clocks"]
        .as_array()
        .expect("snapshot clocks")
        .iter()
        .find(|clock| clock["timezone"] == "Europe/London")
        .expect("renamed flag-like location clock");
    assert_eq!(renamed_flag_clock["label"], "Office");

    let rename = Command::new(backend)
        .args([
            "rename",
            "America/New_York",
            "--label",
            "Boston",
            "--new-label",
            "Sam",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("rename location");
    assert!(rename.success());

    let reset_label = Command::new(backend)
        .args([
            "rename",
            "America/New_York",
            "--label",
            "Sam",
            "--new-label",
            "",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("reset location label");
    assert!(reset_label.success());

    let reset_snapshot = Command::new(backend)
        .args(["snapshot", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot after resetting location label");
    assert!(reset_snapshot.status.success());
    let reset_snapshot: serde_json::Value =
        serde_json::from_slice(&reset_snapshot.stdout).expect("parse reset snapshot");
    let reset_clock = reset_snapshot["clocks"]
        .as_array()
        .expect("snapshot clocks")
        .iter()
        .find(|clock| clock["timezone"] == "America/New_York")
        .expect("reset location clock");
    assert_eq!(reset_clock["label"], "New York");
    assert_eq!(reset_clock["title"], "New York");

    let remove = Command::new(backend)
        .args(["remove", "America/New_York", "--label", "New York"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("remove location");
    assert!(remove.success());

    let unpin = Command::new(backend)
        .arg("unpin")
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("unpin timezone");
    assert!(unpin.success());
}

#[test]
fn automatic_weather_unit_follows_the_omarchy_weather_location_country() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let config_path = sandbox.path().join("config.json");
    let shell_config = home.join(".config/omarchy/shell.json");
    let weather_state = home.join(".local/state/omarchy/settings/weather.json");
    let zoneinfo = sandbox.path().join("zoneinfo");
    fs::create_dir_all(shell_config.parent().expect("shell config parent"))
        .expect("create shell config directory");
    fs::create_dir_all(weather_state.parent().expect("weather state parent"))
        .expect("create weather state directory");
    fs::create_dir_all(&zoneinfo).expect("create zoneinfo directory");
    fs::write(
        &shell_config,
        r#"{
  "bar": {
    "layout": {
      "center": [
        { "id": "omarchy.weather" }
      ]
    }
  }
}
"#,
    )
    .expect("write shell config");
    fs::write(
        &weather_state,
        r#"{
  "name": "Lagos del Sol",
  "latitude": 21.05417,
  "longitude": -86.84861
}
"#,
    )
    .expect("write weather state");
    fs::write(
        zoneinfo.join("zone.tab"),
        "MX\t+2105-08646\tAmerica/Cancun\tQuintana Roo\n",
    )
    .expect("write zone table");
    fs::write(
        &config_path,
        r#"{
  "version": 6,
  "timezones": [
    { "timezone": "America/Cancun", "label": "Home" }
  ]
}
"#,
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_omarchy-world-clock-backend"))
        .args(["snapshot", "--at", "2026-08-20T20:18:00Z"])
        .env("HOME", &home)
        .env("TZDIR", &zoneinfo)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot");

    assert!(output.status.success());
    let snapshot: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse snapshot");
    assert_eq!(snapshot["weather_unit"], "metric");
}

#[test]
fn weather_command_honors_the_open_meteo_opt_out_without_network_access() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let config_path = sandbox.path().join("config.json");
    fs::write(
        &config_path,
        r#"{
  "version": 6,
  "disable_open_meteo_geolocation": true,
  "timezones": [
    {
      "timezone": "America/Cancun",
      "label": "Home",
      "latitude": 21.1619,
      "longitude": -86.8515
    }
  ]
}
"#,
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_omarchy-world-clock-backend"))
        .arg("weather")
        .args(["--at", "2026-08-21T15:00:00Z"])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("request weather with remote data disabled");

    assert!(output.status.success());
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse weather payload");
    assert_eq!(payload["source"], "Open-Meteo");
    assert_eq!(payload["disabled"], true);
    assert_eq!(payload["locations"], serde_json::json!([]));
}
