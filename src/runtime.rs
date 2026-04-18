use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn runtime_pid_path() -> PathBuf {
    if let Some(path) = env::var_os("OMARCHY_WORLD_CLOCK_PID_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = env::var_os("OMARCHY_WORLD_CLOCK_RS_PID_PATH") {
        return PathBuf::from(path);
    }

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!("/tmp/omarchy-world-clock-{}", unsafe {
                libc::geteuid()
            }))
        });
    runtime_dir.join("omarchy-world-clock.pid")
}

pub fn debug_runtime_log_path() -> PathBuf {
    env::temp_dir().join("owc-popup-runtime.log")
}

pub fn read_pid(pid_path: &Path) -> Option<i32> {
    fs::read_to_string(pid_path)
        .ok()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
}

pub fn is_process_alive(pid: Option<i32>) -> bool {
    let Some(pid) = pid else {
        return false;
    };

    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }

    io::Error::last_os_error()
        .raw_os_error()
        .is_some_and(|code| code == libc::EPERM)
}

pub fn popup_running(pid_path: &Path) -> bool {
    let alive = is_process_alive(read_pid(pid_path));
    if !alive {
        let _ = fs::remove_file(pid_path);
    }
    alive
}

pub fn kill_popup(pid_path: &Path) -> bool {
    let Some(pid) = read_pid(pid_path) else {
        let _ = fs::remove_file(pid_path);
        return false;
    };

    if !is_process_alive(Some(pid)) {
        let _ = fs::remove_file(pid_path);
        return false;
    }

    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    for _ in 0..20 {
        if !is_process_alive(Some(pid)) {
            let _ = fs::remove_file(pid_path);
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }

    true
}

pub fn spawn_popup() -> Result<()> {
    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let mut command = Command::new(current_exe);
    command.arg("popup").stdin(Stdio::null());

    if env::var_os("OMARCHY_WORLD_CLOCK_DEBUG").is_some() {
        let stdout_log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(debug_runtime_log_path())
            .context("failed to open popup runtime log for stdout")?;
        let stderr_log = stdout_log
            .try_clone()
            .context("failed to clone popup runtime log handle")?;
        command.stdout(Stdio::from(stdout_log));
        command.stderr(Stdio::from(stderr_log));
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let _child = command.spawn().context("failed to spawn popup")?;
    Ok(())
}
