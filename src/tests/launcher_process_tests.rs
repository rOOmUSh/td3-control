#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "macos")]
use crate::launcher::process::{macos_child_log_path, macos_child_working_dir};

#[cfg(target_os = "macos")]
fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "td3-control-{}-{}-{}",
        name,
        std::process::id(),
        nanos
    ))
}

#[cfg(target_os = "macos")]
#[test]
fn macos_child_working_dir_uses_release_folder_with_template() {
    let dir = unique_temp_dir("release-template");
    fs::create_dir_all(dir.join("config")).unwrap();
    fs::write(dir.join("config/default_env.template"), "WEB_PORT=3030\n").unwrap();
    let exe = dir.join("td3-control");

    assert_eq!(macos_child_working_dir(&exe), Some(dir.clone()));
    assert_eq!(
        macos_child_log_path(&dir),
        dir.join("td3-control-launcher-child.log")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn macos_child_working_dir_uses_release_folder_with_user_env() {
    let dir = unique_temp_dir("release-env");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("TD3_CONFIG.env"), "WEB_PORT=3030\n").unwrap();
    let exe = dir.join("td3-control");

    assert_eq!(macos_child_working_dir(&exe), Some(dir.clone()));

    fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn macos_child_working_dir_ignores_non_release_build_folder() {
    let dir = unique_temp_dir("target-folder");
    fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("td3-control");

    assert_eq!(macos_child_working_dir(&exe), None);

    fs::remove_dir_all(dir).unwrap();
}
