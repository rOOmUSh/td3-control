use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use crate::web::config_storage::{read_user_config, write_json_atomic_with_temp};
use crate::web::user_config::{KeyboardConfig, ProgressionConfig, ScalesConfig, UserConfigFile};

fn temp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "td3-config-storage-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn keyboard_config_rejects_duplicate_bindings() {
    let raw = crate::web::embedded_ui::read_text("config/keyboard-defaults.json").unwrap();
    let mut config: KeyboardConfig = serde_json::from_str(&raw).unwrap();
    config.actions.insert("accent".into(), "c".into());

    let err = config.validate_and_normalize().unwrap_err().to_string();

    assert!(err.contains("keyboard binding 'c' is used"));
}

#[test]
fn scales_config_normalizes_intervals_and_tags() {
    let raw = r#"{
        "tag_groups": [
            { "label": "Safe", "tag": "safe" },
            { "label": "Dark", "tag": "dark" }
        ],
        "scales": [
            {
                "id": "custom_scale",
                "name": "Custom Scale",
                "intervals": [7, 0, 3],
                "tags": ["dark", "safe"]
            }
        ]
    }"#;
    let mut config: ScalesConfig = serde_json::from_str(raw).unwrap();

    config.validate_and_normalize().unwrap();

    assert_eq!(config.scales[0].intervals, vec![0, 3, 7]);
    let saved = serde_json::to_value(&config).unwrap();
    assert_eq!(saved["scales"][0]["tags"], json!(["safe", "dark"]));
}

#[test]
fn embedded_progression_defaults_validate() {
    let dir = temp_dir("defaults");

    let config = read_user_config::<ProgressionConfig>(&dir).unwrap();

    assert_eq!(config.anchor_steps, vec![0, 4, 8, 12]);
    assert_eq!(config.default_timeline.len(), 16);
}

#[test]
fn failed_atomic_write_preserves_old_config() {
    let dir = temp_dir("preserve");
    let target = dir.join("keyboard-config.json");
    let temp_blocker = dir.join("keyboard-config.tmp");
    std::fs::write(&target, "old config\n").unwrap();
    std::fs::create_dir(&temp_blocker).unwrap();

    let result =
        write_json_atomic_with_temp("keyboard", &target, &temp_blocker, &json!({"new": true}));

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old config\n");
    assert!(temp_blocker.is_dir());
}

#[test]
fn failed_atomic_replace_does_not_activate_temp_file() {
    let dir = temp_dir("replace");
    let target = dir.join("scales-config.json");
    let temp_path = dir.join("scales-config.tmp");
    std::fs::create_dir(&target).unwrap();

    let result = write_json_atomic_with_temp("scales", &target, &temp_path, &json!({"new": true}));

    assert!(result.is_err());
    assert!(target.is_dir());
    assert!(!temp_path.exists());
}
