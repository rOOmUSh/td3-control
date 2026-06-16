//! Child process spawning for launcher-started control sessions.

use std::process::Command;

#[cfg(target_os = "macos")]
use std::fs::{File, OpenOptions};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Stdio;

use super::child_args::build_control_args;
use super::choice::LauncherChoice;
use crate::browser::{AUTO_OPEN_BROWSER_ENV, SKIP_SCRATCH_CONFIRM_ENV};

pub(crate) fn spawn_control_child(choice: &LauncherChoice) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current executable: {}", err);
            return;
        }
    };
    let mut cmd = Command::new(&exe);
    cmd.args(build_control_args(
        &choice.scratch,
        &choice.midi,
        choice.web_port,
    ))
    .env(SKIP_SCRATCH_CONFIRM_ENV, "1")
    .env(AUTO_OPEN_BROWSER_ENV, "1");
    configure_platform_child(&mut cmd, &exe);
    match cmd.spawn() {
        Ok(_child) => {
            eprintln!(
                "Spawned td3-control child process with scratch slot {} on web port {}.",
                choice.scratch, choice.web_port
            );
        }
        Err(err) => {
            eprintln!("error: failed to spawn control process: {}", err);
        }
    }
}

#[cfg(target_os = "macos")]
fn configure_platform_child(cmd: &mut Command, exe: &Path) {
    let Some(dir) = macos_child_working_dir(exe) else {
        return;
    };
    cmd.current_dir(&dir);

    let log_path = macos_child_log_path(&dir);
    match open_macos_child_log(&log_path) {
        Ok((stdout, stderr)) => {
            cmd.stdout(Stdio::from(stdout));
            cmd.stderr(Stdio::from(stderr));
            eprintln!("Launcher child log: {}", log_path.display());
        }
        Err(err) => {
            eprintln!(
                "warning: could not open launcher child log {}: {}",
                log_path.display(),
                err
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_platform_child(_cmd: &mut Command, _exe: &std::path::Path) {}

#[cfg(target_os = "macos")]
pub(crate) fn macos_child_working_dir(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    if dir.join("TD3_CONFIG.env").is_file() || dir.join("config/default_env.template").is_file() {
        return Some(dir.to_path_buf());
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_child_log_path(dir: &Path) -> PathBuf {
    dir.join("td3-control-launcher-child.log")
}

#[cfg(target_os = "macos")]
fn open_macos_child_log(path: &Path) -> Result<(File, File), std::io::Error> {
    let stdout = OpenOptions::new().create(true).append(true).open(path)?;
    let stderr = OpenOptions::new().create(true).append(true).open(path)?;
    Ok((stdout, stderr))
}
