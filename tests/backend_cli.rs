use std::fs;
use std::process::Command;

const BACKEND_PROTOCOL_VERSION: u64 = 7;

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
  "version": 7,
  "pinned_locations": [
    { "timezone": "Asia/Tokyo", "label": "Tokyo" },
    { "timezone": "America/New_York", "label": "New York" }
  ],
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    { "timezone": "Asia/Tokyo", "label": "Tokyo" },
    { "timezone": "America/New_York", "label": "New York" }
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
    let pinned = payload["pinned_clocks"]
        .as_array()
        .expect("pinned clock payload");
    assert_eq!(pinned.len(), 2);
    assert_eq!(pinned[0]["code"], "TOK");
    assert_eq!(pinned[0]["label"], "Tokyo");
    assert!(pinned[0]["time"]
        .as_str()
        .is_some_and(|time| !time.is_empty()));
    assert_eq!(pinned[1]["code"], "NY");
    assert_eq!(pinned[1]["label"], "New York");

    let help = Command::new(env!("CARGO_BIN_EXE_omarchy-world-clock-backend"))
        .arg("--help")
        .output()
        .expect("read backend help");
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("group-add --name NAME"));
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
    assert_eq!(snapshot["schema_version"], 3);
    assert_eq!(snapshot["pinned_timezone"], "Asia/Tokyo");
    assert_eq!(snapshot["pinned_location"]["id"], 2);
    assert_eq!(snapshot["pinned_location"]["timezone"], "Asia/Tokyo");
    assert_eq!(snapshot["pinned_location"]["label"], "Tokyo");
    assert_eq!(snapshot["configured_count"], 2);
    assert_eq!(snapshot["weather_unit"], "metric");
    assert!(snapshot["summary"]["utc_offset_seconds"].is_number());
    assert!(snapshot["summary"]["utc_offset_states"]
        .as_array()
        .is_some_and(|states| !states.is_empty()));
    assert!(snapshot["clocks"].as_array().is_some_and(|clocks| clocks
        .iter()
        .all(|clock| clock["utc_offset_seconds"].is_number())));
    assert!(snapshot["clocks"].as_array().is_some_and(|clocks| clocks
        .iter()
        .all(|clock| clock["utc_offset_states"]
            .as_array()
            .is_some_and(|states| !states.is_empty()))));
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
    assert_eq!(scrub["schema_version"], 3);
    assert_eq!(scrub["source_timezone"], "UTC");
    assert_eq!(scrub["date"], "2026-08-11");
    assert_eq!(scrub["time_format"], "24h");
    let scrub_locations = scrub["locations"].as_array().expect("scrub locations");
    let snapshot_locations = std::iter::once(&snapshot["summary"])
        .chain(snapshot["clocks"].as_array().expect("snapshot clocks"));
    assert_eq!(
        scrub_locations.len(),
        snapshot["clocks"].as_array().unwrap().len() + 1
    );
    for (scrub_location, snapshot_location) in scrub_locations.iter().zip(snapshot_locations) {
        assert_eq!(scrub_location["id"], snapshot_location["id"]);
        assert_eq!(scrub_location["timezone"], snapshot_location["timezone"]);
        assert_eq!(scrub_location["label"], snapshot_location["label"]);
        assert!(scrub_location["states"]
            .as_array()
            .is_some_and(|states| !states.is_empty()));
        assert!(scrub_location["states"][0]["utc_offset_seconds"].is_number());
    }
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
    assert!(scrub["slots"][140].get("summary").is_none());
    assert!(scrub["slots"][140].get("clocks").is_none());

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

    let alias_search = Command::new(backend)
        .args(["search", "Samoa", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("search a timezone link without its own coordinate");
    assert!(alias_search.status.success());
    let alias_search: serde_json::Value =
        serde_json::from_slice(&alias_search.stdout).expect("parse alias search results");
    let samoa = alias_search
        .as_array()
        .and_then(|results| results.iter().find(|result| result["title"] == "Samoa"))
        .expect("Samoa should remain searchable");
    assert!(samoa["latitude"].is_null());
    assert!(samoa["longitude"].is_null());
    assert!(samoa["focus_latitude"].is_number());
    assert!(samoa["focus_longitude"].is_number());

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
            "--place-label",
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

    let add_duplicate = Command::new(backend)
        .args([
            "add",
            "America/New_York",
            "--place-label",
            "Boston",
            "--custom-label",
            "Sister",
            "--latitude",
            "42.3601",
            "--longitude",
            "-71.0589",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("add the same place as a separate card");
    assert!(add_duplicate.success());

    let pin_boston = Command::new(backend)
        .args(["pin", "America/New_York", "--id", "4"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("pin second location");
    assert!(pin_boston.success());

    let module = Command::new(backend)
        .arg("module")
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render module with multiple pins");
    assert!(module.status.success());
    let module: serde_json::Value =
        serde_json::from_slice(&module.stdout).expect("parse multi-pin module");
    let pin_codes = module["pinned_clocks"]
        .as_array()
        .expect("pinned clocks")
        .iter()
        .map(|clock| clock["code"].as_str().expect("pin code"))
        .collect::<Vec<_>>();
    assert_eq!(pin_codes, vec!["TOK", "SIS"]);

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
            "--id",
            "3",
            "--new-label",
            "Sam",
        ])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("rename location");
    assert!(rename.success());

    let reset_label = Command::new(backend)
        .args(["rename", "America/New_York", "--id", "3", "--new-label", ""])
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
        .find(|clock| clock["id"] == 3)
        .expect("reset location clock");
    assert_eq!(reset_clock["label"], "Boston");
    assert_eq!(reset_clock["title"], "Boston");
    assert_eq!(reset_clock["place"], "Boston");
    assert_eq!(reset_clock["place_title"], "Boston");
    assert_eq!(reset_clock["custom_label"], "");

    let sister_clock = reset_snapshot["clocks"]
        .as_array()
        .expect("snapshot clocks")
        .iter()
        .find(|clock| clock["id"] == 4)
        .expect("duplicate location card");
    assert_eq!(sister_clock["label"], "Sister");
    assert_eq!(sister_clock["title"], "Sister");
    assert_eq!(sister_clock["place"], "Boston");
    assert_eq!(sister_clock["place_title"], "Boston");
    assert_eq!(sister_clock["custom_label"], "Sister");

    let remove = Command::new(backend)
        .args(["remove", "America/New_York", "--id", "3"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("remove location");
    assert!(remove.success());

    let after_remove = Command::new(backend)
        .args(["snapshot", "--at", "2026-08-11T11:05:00Z"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot after removing one duplicate");
    assert!(after_remove.status.success());
    let after_remove: serde_json::Value =
        serde_json::from_slice(&after_remove.stdout).expect("parse post-remove snapshot");
    assert!(!after_remove["clocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|clock| clock["id"] == 3));
    assert!(after_remove["clocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|clock| clock["id"] == 4 && clock["title"] == "Sister"));

    let unpin = Command::new(backend)
        .args(["unpin", "Asia/Tokyo", "--label", "Tokyo"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("unpin remaining timezone");
    assert!(unpin.success());

    let unpin_sister = Command::new(backend)
        .args(["unpin", "America/New_York", "--id", "4"])
        .env("HOME", &home)
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("unpin duplicate location");
    assert!(unpin_sister.success());

    let saved: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&config_path).expect("read config after unpinning"),
    )
    .expect("parse config after unpinning");
    assert!(saved.get("pinned_locations").is_none());
    assert_eq!(saved["version"], 9);
    assert!(saved["timezones"].as_array().unwrap().iter().all(|entry| {
        entry["id"].as_u64().is_some_and(|id| id > 0)
            && entry["place"]
                .as_str()
                .is_some_and(|place| !place.is_empty())
    }));
}

#[test]
fn bundled_backend_manages_named_location_groups() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let config_path = sandbox.path().join("config.json");
    fs::create_dir_all(&home).expect("create home");
    fs::write(
        &config_path,
        r#"{
  "version": 9,
  "timezones": [
    { "id": 1, "timezone": "UTC", "place": "Home" },
    { "id": 2, "timezone": "Asia/Tokyo", "place": "Tokyo", "label": "Jeff" },
    { "id": 3, "timezone": "America/New_York", "place": "Cape Cod", "label": "Jeff" }
  ]
}
"#,
    )
    .expect("write config");
    let backend = env!("CARGO_BIN_EXE_omarchy-world-clock-backend");
    let run = |args: &[&str]| {
        Command::new(backend)
            .args(args)
            .env("HOME", &home)
            .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
            .output()
            .expect("run group command")
    };

    let project = run(&["group-add", "--name", "Project"]);
    assert!(project.status.success());
    assert_eq!(String::from_utf8_lossy(&project.stdout).trim(), "1");
    let duplicate = run(&["group-add", "--name", "Project"]);
    assert!(duplicate.status.success());
    assert_eq!(String::from_utf8_lossy(&duplicate.stdout).trim(), "2");
    assert!(run(&[
        "group-set-location",
        "--group-id",
        "1",
        "--location-id",
        "2",
        "--included",
        "true",
    ])
    .status
    .success());
    assert!(
        run(&["group-rename", "--group-id", "1", "--name", "Launch crew",])
            .status
            .success()
    );
    assert!(
        run(&["group-move", "--group-id", "2", "--direction", "left",])
            .status
            .success()
    );

    let snapshot = run(&["snapshot", "--at", "2026-08-11T11:05:00Z"]);
    assert!(snapshot.status.success());
    let snapshot: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("parse grouped snapshot");
    assert_eq!(snapshot["groups"][0]["id"], 2);
    assert_eq!(snapshot["groups"][0]["name"], "Project 2");
    assert_eq!(snapshot["groups"][1]["id"], 1);
    assert_eq!(snapshot["groups"][1]["name"], "Launch crew");
    assert_eq!(
        snapshot["groups"][1]["location_ids"],
        serde_json::json!([2])
    );

    assert!(run(&["remove", "Asia/Tokyo", "--id", "2"]).status.success());
    assert!(run(&["group-remove", "--group-id", "2"]).status.success());
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).expect("read grouped config"))
            .expect("parse grouped config");
    assert_eq!(saved["groups"].as_array().map(Vec::len), Some(1));
    assert!(saved["groups"][0].get("location_ids").is_none());
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
