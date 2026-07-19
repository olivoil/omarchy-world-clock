use std::fs;
use std::process::Command;

#[test]
fn install_and_uninstall_waybar_with_default_paths() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let home = sandbox.path().join("home");
    let waybar_dir = home.join(".config/waybar");
    let config_path = waybar_dir.join("config.jsonc");
    let style_path = waybar_dir.join("style.css");
    let user_config_path = home.join(".config/omarchy-world-clock/config.json");

    fs::create_dir_all(&waybar_dir).expect("create waybar dir");
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

    let binary = env!("CARGO_BIN_EXE_omarchy-world-clock");
    let install_status = Command::new(binary)
        .arg("install-waybar")
        .env("HOME", &home)
        .status()
        .expect("run install-waybar");
    assert!(install_status.success());

    let config_text = fs::read_to_string(&config_path).expect("read patched config");
    let style_text = fs::read_to_string(&style_path).expect("read patched style");
    assert!(config_text.contains("\"custom/world-clock\""));
    assert!(config_text.contains("omarchy-world-clock module"));
    assert!(style_text.contains("#custom-world-clock"));
    assert!(user_config_path.exists());

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
fn install_and_uninstall_shell_with_default_paths() {
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
        .args([
            "install-shell",
            "--command-path",
            "/usr/bin/omarchy-world-clock",
        ])
        .env("HOME", &home)
        .status()
        .expect("run install-shell");
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
