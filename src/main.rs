use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use omarchy_world_clock::config::{
    detect_local_timezone, system_time_format, ConfigManager, RemotePlaceSearch, TimezoneResolver,
};
use omarchy_world_clock::omarchy_shell;
use omarchy_world_clock::popup::run_popup;
use omarchy_world_clock::quattro::{build_map_location, build_snapshot, QuattroSnapshot};
use omarchy_world_clock::runtime::{
    debug_runtime_log_path, kill_popup, popup_running, runtime_pid_path, spawn_popup,
};
use omarchy_world_clock::time::parse_manual_reference_details;
use omarchy_world_clock::waybar::{
    module_payload, patch_config_text, patch_style_text, unpatch_config_text, unpatch_style_text,
    MODULE_MARKER_START, STYLE_MARKER_START,
};
use serde::Serialize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tzf_rs::Finder as TimezoneFinder;

const QUATTRO_PLUGIN_ID: &str = "io.github.olivoil.world-clock";
const QUATTRO_PLUGIN_URL: &str = "https://github.com/olivoil/omarchy-world-clock.git";

fn usage() -> &'static str {
    "Usage: omarchy-world-clock <module|snapshot|convert|search|locate|add|remove|pin|unpin|open|close|toggle|status|popup|version|install|uninstall|reload|install-shell|uninstall-shell|install-waybar|uninstall-waybar|restart-waybar>"
}

fn optional_flag(args: &[String], flag: &str) -> Result<Option<String>> {
    let Some(index) = args.iter().position(|arg| arg == flag) else {
        return Ok(None);
    };
    let Some(value) = args.get(index + 1) else {
        bail!("missing value for flag {flag}");
    };
    Ok(Some(value.clone()))
}

fn optional_path(args: &[String], flag: &str, default: &str) -> Result<PathBuf> {
    Ok(
        PathBuf::from(optional_flag(args, flag)?.unwrap_or_else(|| default.to_string()))
            .expanduser(),
    )
}

fn required_flag(args: &[String], flag: &str) -> Result<String> {
    optional_flag(args, flag)?.ok_or_else(|| anyhow::anyhow!("missing required flag {flag}"))
}

fn optional_f64(args: &[String], flag: &str) -> Result<Option<f64>> {
    optional_flag(args, flag)?
        .map(|value| {
            value
                .parse::<f64>()
                .with_context(|| format!("invalid number for {flag}: {value}"))
        })
        .transpose()
}

fn required_f64(args: &[String], flag: &str) -> Result<f64> {
    optional_f64(args, flag)?.ok_or_else(|| anyhow::anyhow!("missing required flag {flag}"))
}

fn parse_reference_utc(raw: Option<String>) -> Result<DateTime<Utc>> {
    raw.map(|value| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .with_context(|| format!("invalid RFC 3339 reference time: {value}"))
    })
    .transpose()
    .map(|value| value.unwrap_or_else(Utc::now))
}

fn current_snapshot(reference_utc: DateTime<Utc>) -> Result<QuattroSnapshot> {
    let config = ConfigManager::new(None).load()?;
    Ok(build_snapshot(
        &config,
        reference_utc,
        &detect_local_timezone(),
        &system_time_format(),
    ))
}

#[derive(Serialize)]
struct ConversionPayload {
    normalized_input: String,
    snapshot: QuattroSnapshot,
}

fn default_command_path(args: &[String]) -> Result<String> {
    if let Some(command_path) = optional_flag(args, "--command-path")? {
        return Ok(command_path);
    }

    let current_exe = env::current_exe().context("failed to resolve current executable path")?;
    Ok(current_exe.to_string_lossy().into_owned())
}

fn write_text(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn backup_if_needed(path: &Path, marker: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if contents.contains(marker) {
        return Ok(());
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = path.with_file_name(format!(
        "{}.bak.{timestamp}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("backup")
    ));
    fs::copy(path, &backup_path).with_context(|| {
        format!(
            "failed to create backup {} from {}",
            backup_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn backup(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = path.with_file_name(format!(
        "{}.bak.{timestamp}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("backup")
    ));
    fs::copy(path, &backup_path).with_context(|| {
        format!(
            "failed to create backup {} from {}",
            backup_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn shell_config_path(args: &[String]) -> Result<PathBuf> {
    optional_path(args, "--shell-config", "~/.config/omarchy/shell.json")
}

fn quattro_plugin_path(args: &[String]) -> Result<PathBuf> {
    optional_path(
        args,
        "--plugin-dir",
        &format!("~/.config/omarchy/plugins/{QUATTRO_PLUGIN_ID}"),
    )
}

fn remove_legacy_shell_module(args: &[String]) -> Result<bool> {
    let shell_config = shell_config_path(args)?;
    if !shell_config.exists() {
        return Ok(false);
    }

    let config_text = fs::read_to_string(&shell_config)
        .with_context(|| format!("failed to read {}", shell_config.display()))?;
    if !omarchy_shell::contains_module(&config_text) {
        return Ok(false);
    }

    backup(&shell_config)?;
    write_text(
        &shell_config,
        &omarchy_shell::unpatch_config_text(&config_text)?,
    )?;
    Ok(true)
}

fn run_checked(program: &str, args: &[&str], action: &str) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to {action}"))?;
    if !status.success() {
        bail!("failed to {action}: {program} exited with {status}");
    }
    Ok(())
}

fn quattro_shell_running() -> bool {
    Command::new("omarchy-shell")
        .args(["shell", "ping"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn call_quattro_panel(method: &str) -> Result<Option<String>> {
    if !quattro_shell_running() {
        return Ok(None);
    }

    let output = Command::new("omarchy-shell")
        .args([QUATTRO_PLUGIN_ID, method])
        .output()
        .with_context(|| format!("failed to call the native World Clock {method} command"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            bail!("failed to call the native World Clock {method} command");
        }
        bail!("failed to call the native World Clock {method} command: {detail}");
    }

    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

fn quattro_plugin_revision(args: &[String]) -> Result<String> {
    let revision = optional_flag(args, "--plugin-revision")?
        .or_else(|| env::var("OMARCHY_WORLD_CLOCK_PLUGIN_REVISION").ok())
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));
    let revision = revision.trim();
    if revision.is_empty() || revision.starts_with('-') {
        bail!("invalid Quattro plugin revision: {revision:?}");
    }
    Ok(revision.to_string())
}

fn pin_quattro_plugin(plugin_path: &Path, plugin_url: &str, revision: &str) -> Result<()> {
    if !plugin_path.join(".git").exists() {
        bail!(
            "Quattro plugin is not a git checkout and cannot be pinned to {revision}: {}",
            plugin_path.display()
        );
    }
    let plugin_path = plugin_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Quattro plugin path is not valid UTF-8"))?;
    run_checked(
        "git",
        &[
            "-C",
            plugin_path,
            "fetch",
            "--quiet",
            "--",
            plugin_url,
            revision,
        ],
        "fetch the matching Quattro plugin revision",
    )?;
    let manifest_ref = "FETCH_HEAD:manifest.json";
    let manifest_check = Command::new("git")
        .args(["-C", plugin_path, "cat-file", "-e", manifest_ref])
        .output()
        .context("failed to inspect the matching Quattro plugin revision")?;
    if !manifest_check.status.success() {
        bail!(
            "Quattro plugin revision {revision} does not contain manifest.json; the installed plugin was left unchanged"
        );
    }
    run_checked(
        "git",
        &[
            "-C",
            plugin_path,
            "checkout",
            "--quiet",
            "--detach",
            "FETCH_HEAD",
        ],
        "check out the matching Quattro plugin revision",
    )?;
    run_checked(
        "omarchy",
        &["plugin", "validate", plugin_path],
        "validate the pinned Quattro plugin",
    )
}

fn install_shell(args: &[String]) -> Result<()> {
    if !quattro_shell_running() {
        bail!("Omarchy shell is not running; start it before installing the Quattro plugin");
    }

    let user_config = optional_path(
        args,
        "--user-config",
        "~/.config/omarchy-world-clock/config.json",
    )?;
    let command_path = default_command_path(args)?;
    ConfigManager::new(Some(user_config)).load()?;

    let plugin_path = quattro_plugin_path(args)?;
    let plugin_revision = quattro_plugin_revision(args)?;
    let plugin_url =
        optional_flag(args, "--plugin-url")?.unwrap_or_else(|| QUATTRO_PLUGIN_URL.to_string());
    if plugin_path.exists() && !plugin_path.join("manifest.json").is_file() {
        bail!(
            "Quattro plugin directory exists without a manifest: {}",
            plugin_path.display()
        );
    }

    if !plugin_path.exists() {
        run_checked(
            "omarchy",
            &["plugin", "add", &plugin_url, "--yes"],
            "add the Quattro plugin",
        )?;
    }

    // `omarchy plugin add` currently clones the default branch and exposes no
    // revision option. Pin immediately afterwards so the QML frontend always
    // matches this backend release, including package downgrades.
    pin_quattro_plugin(&plugin_path, &plugin_url, &plugin_revision)?;

    run_checked(
        "omarchy",
        &["bar", "put", QUATTRO_PLUGIN_ID, "--after", "omarchy.clock"],
        "put the World Clock plugin on the bar",
    )?;
    run_checked(
        "omarchy",
        &["bar", "set", QUATTRO_PLUGIN_ID, "command", &command_path],
        "set the World Clock backend command",
    )?;

    // Only remove the working command widget after its native replacement is
    // installed and live. The timestamped backup makes the migration easy to
    // reverse by hand as well.
    remove_legacy_shell_module(args)?;
    reload_shell();

    println!("Installed Quattro plugin {QUATTRO_PLUGIN_ID}.");
    Ok(())
}

fn uninstall_shell(args: &[String]) -> Result<()> {
    let removed_legacy_module = remove_legacy_shell_module(args)?;
    let plugin_path = quattro_plugin_path(args)?;
    if plugin_path.exists() {
        run_checked(
            "omarchy",
            &["plugin", "remove", QUATTRO_PLUGIN_ID, "--yes"],
            "remove the Quattro plugin",
        )?;
        println!("Removed Quattro plugin {QUATTRO_PLUGIN_ID}.");
    } else if removed_legacy_module {
        println!("Removed the legacy Quattro command module.");
    }
    if removed_legacy_module {
        reload_shell();
    }
    Ok(())
}

fn omarchy_shell_available(args: &[String]) -> bool {
    shell_config_path(args).is_ok_and(|path| path.exists())
        || env::var_os("OMARCHY_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/share/omarchy"))
            .join("shell")
            .is_dir()
}

fn install_waybar(args: &[String]) -> Result<()> {
    let waybar_config = optional_path(args, "--waybar-config", "~/.config/waybar/config.jsonc")?;
    let waybar_style = optional_path(args, "--waybar-style", "~/.config/waybar/style.css")?;
    let command_path = default_command_path(args)?;
    let user_config = optional_path(
        args,
        "--user-config",
        "~/.config/omarchy-world-clock/config.json",
    )?;

    backup_if_needed(&waybar_config, MODULE_MARKER_START)?;
    backup_if_needed(&waybar_style, STYLE_MARKER_START)?;

    let config_text = fs::read_to_string(&waybar_config)
        .with_context(|| format!("failed to read {}", waybar_config.display()))?;
    let style_text = fs::read_to_string(&waybar_style)
        .with_context(|| format!("failed to read {}", waybar_style.display()))?;

    write_text(
        &waybar_config,
        &patch_config_text(&config_text, &command_path)?,
    )?;
    write_text(&waybar_style, &patch_style_text(&style_text))?;
    ConfigManager::new(Some(user_config)).load()?;
    Ok(())
}

fn uninstall_waybar(args: &[String]) -> Result<()> {
    let waybar_config = optional_path(args, "--waybar-config", "~/.config/waybar/config.jsonc")?;
    let waybar_style = optional_path(args, "--waybar-style", "~/.config/waybar/style.css")?;

    if waybar_config.exists() {
        let config_text = fs::read_to_string(&waybar_config)
            .with_context(|| format!("failed to read {}", waybar_config.display()))?;
        write_text(&waybar_config, &unpatch_config_text(&config_text)?)?;
    }
    if waybar_style.exists() {
        let style_text = fs::read_to_string(&waybar_style)
            .with_context(|| format!("failed to read {}", waybar_style.display()))?;
        write_text(&waybar_style, &unpatch_style_text(&style_text))?;
    }
    Ok(())
}

fn restart_waybar() {
    match Command::new("omarchy-restart-waybar").status() {
        Ok(_) => return,
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return,
        Err(_) => {}
    }
    let _ = Command::new("pkill").args(["-SIGUSR2", "waybar"]).status();
}

fn reload_shell() {
    if Command::new("omarchy-shell")
        .args(["shell", "reloadConfig"])
        .status()
        .is_ok_and(|status| status.success())
    {
        return;
    }
    let _ = Command::new("omarchy").args(["restart", "shell"]).status();
}

fn main() -> Result<()> {
    if env::var_os("OMARCHY_WORLD_CLOCK_DEBUG").is_some() {
        std::panic::set_hook(Box::new(|panic_info| {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(debug_runtime_log_path())
            {
                let _ = writeln!(file, "panic: {panic_info}");
            }
        }));
    }

    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    let remaining_args = args.collect::<Vec<_>>();

    let pid_path = runtime_pid_path();
    match command.as_str() {
        "module" => {
            let payload = module_payload(&pid_path)?;
            println!("{}", serde_json::to_string(&payload)?);
        }
        "snapshot" => {
            let reference_utc = parse_reference_utc(optional_flag(&remaining_args, "--at")?)?;
            println!(
                "{}",
                serde_json::to_string(&current_snapshot(reference_utc)?)?
            );
        }
        "convert" => {
            let timezone = required_flag(&remaining_args, "--timezone")?;
            let value = required_flag(&remaining_args, "--value")?;
            let base = parse_reference_utc(optional_flag(&remaining_args, "--base")?)?;
            let parsed = parse_manual_reference_details(&value, &timezone, base)
                .map_err(|message| anyhow::anyhow!(message))?;
            let payload = ConversionPayload {
                normalized_input: parsed.normalized_text,
                snapshot: current_snapshot(parsed.reference_utc)?,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        "search" => {
            let query = remaining_args
                .first()
                .map(String::as_str)
                .unwrap_or_default();
            let config = ConfigManager::new(None).load()?;
            let resolver = TimezoneResolver::new(None);
            let mut results = resolver.search(query, 8);
            if results.is_empty() && !config.disable_open_meteo_geolocation {
                results = RemotePlaceSearch::new(None, None).search(query, 8);
            }
            println!("{}", serde_json::to_string(&results)?);
        }
        "locate" => {
            let latitude = required_f64(&remaining_args, "--latitude")?;
            let longitude = required_f64(&remaining_args, "--longitude")?;
            let reference_utc = parse_reference_utc(optional_flag(&remaining_args, "--at")?)?;
            if !latitude.is_finite()
                || !longitude.is_finite()
                || !(-90.0..=90.0).contains(&latitude)
                || !(-180.0..=180.0).contains(&longitude)
            {
                bail!("map coordinates are outside the world extent");
            }

            let timezone_finder = TimezoneFinder::new();
            let timezone = timezone_finder.get_tz_name(longitude, latitude);
            let location = if timezone.is_empty() || timezone.starts_with("Etc/") {
                None
            } else {
                let resolver = TimezoneResolver::new(None);
                let result = resolver.describe_timezone(timezone);
                result.map(|result| {
                    build_map_location(
                        &result,
                        latitude,
                        longitude,
                        reference_utc,
                        &detect_local_timezone(),
                        &system_time_format(),
                    )
                })
            };
            println!("{}", serde_json::to_string(&location)?);
        }
        "add" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to add"))?;
            let label = optional_flag(&remaining_args, "--label")?.unwrap_or_default();
            let manager = ConfigManager::new(None);
            let outcome = manager.add_timezone_with_coordinate(
                timezone,
                &label,
                optional_f64(&remaining_args, "--latitude")?,
                optional_f64(&remaining_args, "--longitude")?,
            )?;
            if !outcome.added {
                bail!("location is invalid or already configured: {timezone}");
            }
        }
        "remove" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to remove"))?;
            let manager = ConfigManager::new(None);
            manager.remove_location(
                timezone,
                optional_flag(&remaining_args, "--label")?.as_deref(),
            )?;
        }
        "pin" => {
            let timezone = remaining_args
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing timezone to pin"))?;
            ConfigManager::new(None).set_pinned_location(
                Some(timezone),
                optional_flag(&remaining_args, "--label")?.as_deref(),
            )?;
        }
        "unpin" => {
            ConfigManager::new(None).set_pinned_timezone(None)?;
        }
        "open" => {
            if call_quattro_panel("open")?.is_none() && !popup_running(&pid_path) {
                spawn_popup()?;
            }
        }
        "close" => {
            if call_quattro_panel("close")?.is_none() {
                let _ = kill_popup(&pid_path);
            }
        }
        "toggle" => {
            if call_quattro_panel("toggle")?.is_none() {
                if popup_running(&pid_path) {
                    let _ = kill_popup(&pid_path);
                } else {
                    spawn_popup()?;
                }
            }
        }
        "status" => {
            if let Some(status) = call_quattro_panel("status")? {
                if !matches!(status.as_str(), "open" | "closed") {
                    bail!("native World Clock returned an invalid status: {status}");
                }
                println!("{status}");
            } else {
                println!(
                    "{}",
                    if popup_running(&pid_path) {
                        "open"
                    } else {
                        "closed"
                    }
                );
            }
        }
        "popup" => {
            if popup_running(&pid_path) {
                return Ok(());
            }
            run_popup(&pid_path, None)?;
        }
        "version" | "--version" | "-V" => {
            println!(env!("CARGO_PKG_VERSION"));
        }
        "install" => {
            if omarchy_shell_available(&remaining_args) {
                install_shell(&remaining_args)?;
            } else {
                install_waybar(&remaining_args)?;
                restart_waybar();
            }
        }
        "uninstall" => {
            uninstall_shell(&remaining_args)?;
            uninstall_waybar(&remaining_args)?;
            if omarchy_shell_available(&remaining_args) {
                reload_shell();
            } else {
                restart_waybar();
            }
        }
        "reload" => {
            if omarchy_shell_available(&remaining_args) {
                reload_shell();
            } else {
                restart_waybar();
            }
        }
        "install-shell" => {
            install_shell(&remaining_args)?;
        }
        "uninstall-shell" => {
            uninstall_shell(&remaining_args)?;
        }
        "install-waybar" => {
            install_waybar(&remaining_args)?;
        }
        "uninstall-waybar" => {
            uninstall_waybar(&remaining_args)?;
        }
        "restart-waybar" => {
            restart_waybar();
        }
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }

    Ok(())
}

trait ExpandUser {
    fn expanduser(self) -> PathBuf;
}

impl ExpandUser for PathBuf {
    fn expanduser(self) -> PathBuf {
        let rendered = self.to_string_lossy();
        if rendered == "~" {
            return env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
        }
        if let Some(suffix) = rendered.strip_prefix("~/") {
            return env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(suffix);
        }
        self
    }
}
