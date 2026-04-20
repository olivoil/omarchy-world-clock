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
