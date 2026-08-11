use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable stub");
    let mut permissions = fs::metadata(path)
        .expect("read stub metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make stub executable");
}

#[test]
fn generic_install_detects_waybar_and_preserves_existing_locations() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let waybar_dir = home.join(".config/waybar");
    let config_path = waybar_dir.join("config.jsonc");
    let style_path = waybar_dir.join("style.css");
    let user_config_path = home.join(".config/omarchy-world-clock/config.json");
    let omarchy_path = sandbox.path().join("omarchy-3");
    let existing_user_config = r#"{
  "version": 6,
  "timezones": [
    {
      "timezone": "UTC",
      "label": "Home"
    },
    {
      "timezone": "Asia/Tokyo",
      "label": "Tokyo"
    }
  ]
}
"#;

    fs::create_dir_all(&waybar_dir).expect("create waybar dir");
    fs::create_dir_all(user_config_path.parent().expect("user config parent"))
        .expect("create user config dir");
    fs::write(
        &config_path,
        r#"{
  "modules-center": ["clock", "custom/update"]
}
"#,
    )
    .expect("write waybar config");
    fs::write(
        &style_path,
        r#"#clock {
  color: white;
}
"#,
    )
    .expect("write waybar style");
    fs::write(&user_config_path, existing_user_config).expect("write existing user config");

    let binary = env!("CARGO_BIN_EXE_omarchy-world-clock");
    let install_status = Command::new(binary)
        .arg("install")
        .env("HOME", &home)
        .env("OMARCHY_PATH", &omarchy_path)
        .env("PATH", "")
        .status()
        .expect("run generic install");
    assert!(install_status.success());

    let config_text = fs::read_to_string(&config_path).expect("read patched config");
    let style_text = fs::read_to_string(&style_path).expect("read patched style");
    assert!(config_text.contains("\"custom/world-clock\""));
    assert!(config_text.contains("omarchy-world-clock module"));
    assert!(style_text.contains("#custom-world-clock"));
    assert_eq!(
        fs::read_to_string(&user_config_path).expect("read preserved user config"),
        existing_user_config
    );

    let uninstall_status = Command::new(binary)
        .arg("uninstall-waybar")
        .env("HOME", &home)
        .status()
        .expect("run uninstall-waybar");
    assert!(uninstall_status.success());

    let config_text = fs::read_to_string(&config_path).expect("read unpatched config");
    let style_text = fs::read_to_string(&style_path).expect("read unpatched style");
    assert!(!config_text.contains("\"custom/world-clock\""));
    assert!(!style_text.contains("#custom-world-clock"));
}

#[test]
fn generic_install_uses_quattro_plugin_and_migrates_command_module() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let shell_dir = home.join(".config/omarchy");
    let shell_path = shell_dir.join("shell.json");
    let user_config_path = home.join(".config/omarchy-world-clock/config.json");
    let plugin_path = home
        .join(".config/omarchy/plugins")
        .join("io.github.olivoil.world-clock");
    let stubs = sandbox.path().join("stubs");
    let command_log = sandbox.path().join("commands.log");

    fs::create_dir_all(&shell_dir).expect("create shell config dir");
    fs::create_dir_all(&stubs).expect("create stub dir");
    write_executable(
        &stubs.join("omarchy"),
        "#!/bin/sh\nprintf 'omarchy %s\\n' \"$*\" >> \"$TEST_LOG\"\n",
    );
    write_executable(
        &stubs.join("omarchy-shell"),
        "#!/bin/sh\nprintf 'omarchy-shell %s\\n' \"$*\" >> \"$TEST_LOG\"\n",
    );
    fs::write(
        &shell_path,
        r#"{
  "version": 1,
  "bar": {
    "centerAnchor": "omarchy.clock",
    "layout": {
      "left": [],
      "center": [
        { "id": "omarchy.clock" },
        {
          "id": "world-clock",
          "type": "command",
          "exec": "/usr/bin/omarchy-world-clock module"
        },
        { "id": "omarchy.weather" },
        { "id": "omarchy.system-update" }
      ],
      "right": []
    }
  },
  "plugins": []
}
"#,
    )
    .expect("write shell config");

    let binary = env!("CARGO_BIN_EXE_omarchy-world-clock");
    let install_status = Command::new(binary)
        .arg("install")
        .env("HOME", &home)
        .env("PATH", &stubs)
        .env("TEST_LOG", &command_log)
        .status()
        .expect("run generic install");
    assert!(install_status.success());

    let config_text = fs::read_to_string(&shell_path).expect("read migrated shell config");
    let root: serde_json::Value = serde_json::from_str(&config_text).expect("parse shell config");
    let center = root
        .pointer("/bar/layout/center")
        .and_then(serde_json::Value::as_array)
        .expect("read center modules");
    assert_eq!(center[0]["id"], "omarchy.clock");
    assert_eq!(center[1]["id"], "omarchy.weather");
    assert!(!config_text.contains("\"world-clock\""));
    assert!(user_config_path.exists());
    assert!(shell_dir
        .read_dir()
        .expect("read shell config dir")
        .any(|entry| entry
            .expect("read shell config entry")
            .file_name()
            .to_string_lossy()
            .starts_with("shell.json.bak.")));
    let commands = fs::read_to_string(&command_log).expect("read command log");
    assert!(commands
        .contains("omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --yes"));
    assert!(
        commands.contains("omarchy bar put io.github.olivoil.world-clock --after omarchy.clock")
    );
    assert!(commands.contains("omarchy bar set io.github.olivoil.world-clock command "));

    fs::create_dir_all(&plugin_path).expect("create installed plugin dir");
    fs::write(plugin_path.join("manifest.json"), "{}\n").expect("write plugin manifest");

    let reinstall_status = Command::new(binary)
        .arg("install-shell")
        .env("HOME", &home)
        .env("PATH", &stubs)
        .env("TEST_LOG", &command_log)
        .status()
        .expect("rerun install-shell");
    assert!(reinstall_status.success());

    let commands = fs::read_to_string(&command_log).expect("read reinstalled command log");
    assert_eq!(
        commands
            .matches("omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --yes")
            .count(),
        1
    );
    assert_eq!(
        commands.matches("omarchy-shell shell reloadConfig").count(),
        2,
        "each install must reload the backend command into the live widget"
    );

    let uninstall_status = Command::new(binary)
        .arg("uninstall-shell")
        .env("HOME", &home)
        .env("PATH", &stubs)
        .env("TEST_LOG", &command_log)
        .status()
        .expect("run uninstall-shell");
    assert!(uninstall_status.success());

    let commands = fs::read_to_string(&command_log).expect("read updated command log");
    assert!(commands.contains("omarchy plugin remove io.github.olivoil.world-clock --yes"));
}

#[test]
fn popup_lifecycle_commands_report_closed_state_without_a_popup() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let pid_path = sandbox.path().join("world-clock.pid");
    let binary = env!("CARGO_BIN_EXE_omarchy-world-clock");

    let status = Command::new(binary)
        .arg("status")
        .env("OMARCHY_WORLD_CLOCK_PID_PATH", &pid_path)
        .output()
        .expect("run status command");
    assert!(status.status.success());
    assert_eq!(String::from_utf8_lossy(&status.stdout), "closed\n");

    let close_status = Command::new(binary)
        .arg("close")
        .env("OMARCHY_WORLD_CLOCK_PID_PATH", &pid_path)
        .status()
        .expect("run close command");
    assert!(close_status.success());

    let version = Command::new(binary)
        .arg("version")
        .output()
        .expect("run version command");
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn quattro_backend_commands_snapshot_convert_search_and_pin() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let config_path = sandbox.path().join("config.json");
    fs::write(
        &config_path,
        r#"{
  "version": 5,
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    { "timezone": "Asia/Tokyo", "label": "Tokyo" }
  ]
}
"#,
    )
    .expect("write config");
    let binary = env!("CARGO_BIN_EXE_omarchy-world-clock");

    let pin = Command::new(binary)
        .args(["pin", "Asia/Tokyo"])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("pin timezone");
    assert!(pin.success());

    let snapshot = Command::new(binary)
        .args(["snapshot", "--at", "2026-08-11T11:05:00Z"])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot");
    assert!(snapshot.status.success());
    let snapshot: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("parse snapshot");
    assert_eq!(snapshot["schema_version"], 1);
    assert_eq!(snapshot["pinned_timezone"], "Asia/Tokyo");
    assert_eq!(snapshot["configured_count"], 2);

    let module = Command::new(binary)
        .arg("module")
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render module");
    let module: serde_json::Value =
        serde_json::from_slice(&module.stdout).expect("parse module payload");
    assert_eq!(module["pinned_label"], "Tokyo");
    assert!(module["text"].as_str().unwrap().starts_with("  "));

    let conversion = Command::new(binary)
        .args([
            "convert",
            "--timezone",
            "Asia/Tokyo",
            "--value",
            "9am",
            "--base",
            "2026-08-11T11:05:00Z",
        ])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("convert time");
    let conversion: serde_json::Value =
        serde_json::from_slice(&conversion.stdout).expect("parse conversion");
    assert_eq!(conversion["normalized_input"], "09:00");

    let search = Command::new(binary)
        .args(["search", "Tokyo"])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("search timezones");
    let search: serde_json::Value = serde_json::from_slice(&search.stdout).expect("parse search");
    assert_eq!(search[0]["timezone"], "Asia/Tokyo");

    let locate = Command::new(binary)
        .args([
            "locate",
            "--latitude",
            "48.11109",
            "--longitude",
            "-1.67431",
            "--at",
            "2026-08-11T11:05:00Z",
        ])
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

    let unpin = Command::new(binary)
        .arg("unpin")
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("unpin timezone");
    assert!(unpin.success());
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(config_path).expect("read config"))
            .expect("parse config");
    assert!(config.get("pinned_timezone").is_none());
}

#[test]
fn quattro_backend_keeps_distinct_places_that_share_a_timezone() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let config_path = sandbox.path().join("config.json");
    fs::write(
        &config_path,
        r#"{
  "version": 5,
  "timezones": [
    { "timezone": "UTC", "label": "Home" },
    {
      "timezone": "America/New_York",
      "label": "New York",
      "latitude": 40.7128,
      "longitude": -74.0060
    }
  ]
}
"#,
    )
    .expect("write config");
    let binary = env!("CARGO_BIN_EXE_omarchy-world-clock");
    let boston_label = "Boston, Massachusetts, United States";

    let add = Command::new(binary)
        .args([
            "add",
            "America/New_York",
            "--label",
            boston_label,
            "--latitude",
            "42.3601",
            "--longitude",
            "-71.0589",
        ])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("add Boston");
    assert!(add.success(), "Boston should coexist with New York");

    let pin = Command::new(binary)
        .args(["pin", "America/New_York", "--label", boston_label])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("pin Boston");
    assert!(pin.success());

    let snapshot = Command::new(binary)
        .args(["snapshot", "--at", "2026-08-11T11:05:00Z"])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render snapshot");
    assert!(snapshot.status.success());
    let snapshot: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("parse snapshot");
    let same_zone = snapshot["clocks"]
        .as_array()
        .expect("clock list")
        .iter()
        .filter(|clock| clock["timezone"] == "America/New_York")
        .collect::<Vec<_>>();
    assert_eq!(same_zone.len(), 2);
    assert!(same_zone
        .iter()
        .any(|clock| clock["label"] == boston_label && clock["pinned"] == true));
    assert!(same_zone
        .iter()
        .any(|clock| clock["label"] == "New York" && clock["pinned"] == false));

    let remove = Command::new(binary)
        .args(["remove", "America/New_York", "--label", boston_label])
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .status()
        .expect("remove Boston");
    assert!(remove.success());

    let snapshot = Command::new(binary)
        .arg("snapshot")
        .env("OMARCHY_WORLD_CLOCK_CONFIG", &config_path)
        .output()
        .expect("render updated snapshot");
    let snapshot: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("parse updated snapshot");
    let same_zone = snapshot["clocks"]
        .as_array()
        .expect("clock list")
        .iter()
        .filter(|clock| clock["timezone"] == "America/New_York")
        .collect::<Vec<_>>();
    assert_eq!(same_zone.len(), 1);
    assert_eq!(same_zone[0]["label"], "New York");
}
