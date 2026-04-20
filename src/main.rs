use anyhow::{bail, Context, Result};
use omarchy_world_clock::config::ConfigManager;
use omarchy_world_clock::popup::run_popup;
use omarchy_world_clock::runtime::{
    debug_runtime_log_path, kill_popup, popup_running, runtime_pid_path, spawn_popup,
};
use omarchy_world_clock::waybar::{
    module_payload, patch_config_text, patch_style_text, unpatch_config_text, unpatch_style_text,
    MODULE_MARKER_START, STYLE_MARKER_START,
};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn usage() -> &'static str {
    "Usage: omarchy-world-clock <module|toggle|popup|install-waybar|uninstall-waybar|restart-waybar>"
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
        "toggle" => {
            if popup_running(&pid_path) {
                let _ = kill_popup(&pid_path);
            } else {
                spawn_popup()?;
            }
        }
        "popup" => {
            if popup_running(&pid_path) {
                return Ok(());
            }
            run_popup(&pid_path, None)?;
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
