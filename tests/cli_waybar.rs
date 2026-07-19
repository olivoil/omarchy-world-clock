use std::fs;
use std::process::Command;

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
  "version": 4,
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
fn generic_install_detects_omarchy_shell_with_default_paths() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let shell_dir = home.join(".config/omarchy");
    let shell_path = shell_dir.join("shell.json");
    let user_config_path = home.join(".config/omarchy-world-clock/config.json");

    fs::create_dir_all(&shell_dir).expect("create shell config dir");
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
        .args(["install", "--command-path", "/usr/bin/omarchy-world-clock"])
        .env("HOME", &home)
        .env("PATH", "")
        .status()
        .expect("run generic install");
    assert!(install_status.success());

    let config_text = fs::read_to_string(&shell_path).expect("read patched shell config");
    let root: serde_json::Value = serde_json::from_str(&config_text).expect("parse shell config");
    let center = root
        .pointer("/bar/layout/center")
        .and_then(serde_json::Value::as_array)
        .expect("read center modules");
    assert_eq!(center[0]["id"], "omarchy.clock");
    assert_eq!(center[1]["id"], "world-clock");
    assert_eq!(center[2]["id"], "omarchy.weather");
    assert!(user_config_path.exists());
    assert!(shell_dir
        .read_dir()
        .expect("read shell config dir")
        .any(|entry| entry
            .expect("read shell config entry")
            .file_name()
            .to_string_lossy()
            .starts_with("shell.json.bak.")));

    let uninstall_status = Command::new(binary)
        .arg("uninstall-shell")
        .env("HOME", &home)
        .status()
        .expect("run uninstall-shell");
    assert!(uninstall_status.success());

    let config_text = fs::read_to_string(&shell_path).expect("read unpatched shell config");
    assert!(!config_text.contains("world-clock"));
    assert!(config_text.contains("omarchy.clock"));
    assert!(config_text.contains("omarchy.weather"));
}
