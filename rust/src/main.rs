use anyhow::Result;
use omarchy_world_clock_rs::popup::run_popup;
use omarchy_world_clock_rs::runtime::{kill_popup, popup_running, runtime_pid_path, spawn_popup};
use omarchy_world_clock_rs::waybar::module_payload;
use std::env;

fn usage() -> &'static str {
    "Usage: omarchy-world-clock-rs <module|toggle|popup>"
}

fn main() -> Result<()> {
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
