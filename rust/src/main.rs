use anyhow::Result;
use omarchy_world_clock_rs::popup::run_popup;
use omarchy_world_clock_rs::runtime::{
    debug_runtime_log_path, kill_popup, popup_running, runtime_pid_path, spawn_popup,
};
use omarchy_world_clock_rs::waybar::module_payload;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;

fn usage() -> &'static str {
    "Usage: omarchy-world-clock-rs <module|toggle|popup>"
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
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }

    Ok(())
}
