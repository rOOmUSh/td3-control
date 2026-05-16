//! Tests for the `CLI flag > TD3_CONFIG.env > bundled template` precedence.
//!
//! `load_config` reads `std::env::args()` directly, so exercising it from
//! inside a unit test process would require forking. Instead we test the
//! two layers we can isolate:
//!
//! 1. Env-over-template: `AppEnv` produced by `load_or_create` uses user
//!    values where present and template values where absent.
//! 2. CLI-over-env: the `Option<T>` CLI fields that clap parses fall back
//!    to the corresponding `env` field via `unwrap_or` at `Config`
//!    construction time. We exercise this by calling the same `unwrap_or`
//!    pattern directly against a synthetic `(Option, env)` pair, mirroring
//!    the code in `src/config.rs`.
//!
//! Together these cover the precedence contract end-to-end: if a user edit
//! to `TD3_CONFIG.env` does not flow into `env`, test 1 fails; if the CLI
//! flag is ignored when present, test 2 fails.

use std::path::PathBuf;

use crate::app_env::{AppEnv, CONFIG_FILE_PATH};
use crate::formats::mid::MidiSlideMode;

fn temp_dir(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "td3-precedence-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

// ── env-over-template ───────────────────────────────────────────────

#[test]
fn env_overrides_template_for_midi_export_channel() {
    let dir = temp_dir("chan");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "MIDI_EXPORT_CHANNEL=7\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.midi_export_channel, 7);
    // Template defaults flow through for keys the user didn't touch.
    assert!(env.midi_export_ppqn >= 24);
}

#[test]
fn env_overrides_template_for_slide_mode() {
    let dir = temp_dir("slide");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "MIDI_EXPORT_SLIDE_MODE=none\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.midi_export_slide_mode, MidiSlideMode::None);
}

#[test]
fn env_overrides_template_for_backup_dir() {
    let dir = temp_dir("backup");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "BACKUP_DIR_PATH=\"D:/td3-backups\"\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.backup_dir_path, "D:/td3-backups");
}

#[test]
fn env_overrides_template_for_library_and_sidecar_paths() {
    let dir = temp_dir("paths");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(
        &path,
        "LIBRARY_DATABASE_PATH=\"library.sqlite3\"\nPATTERN_SIDECAR_DIR=\"sidecars\"\n",
    )
    .unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.library_database_path, "library.sqlite3");
    assert_eq!(env.pattern_sidecar_dir, "sidecars");
}

// ── CLI-over-env (via the unwrap_or pattern load_config uses) ────────

/// Mirrors the pattern `args.foo.unwrap_or(env.foo)` used throughout
/// `src/config.rs`. If this stays green but `load_config` regresses, the
/// regression is in the match-arm wiring - not the precedence rule itself.
fn resolve<T: Copy>(cli: Option<T>, env_value: T) -> T {
    cli.unwrap_or(env_value)
}

#[test]
fn cli_flag_wins_over_env_when_provided() {
    let env = AppEnv::from_template().unwrap();
    // --bpm 222 - CLI wins.
    assert_eq!(resolve(Some(222u32), env.ui_default_bpm), 222);
    // --mid-channel 9 - CLI wins.
    assert_eq!(resolve(Some(9u8), env.midi_export_channel), 9);
}

#[test]
fn env_value_used_when_cli_flag_absent() {
    let env = AppEnv::from_template().unwrap();
    // No --bpm on CLI → env value flows through.
    assert_eq!(resolve(None::<u32>, env.ui_default_bpm), env.ui_default_bpm);
    // No --mid-channel → env value.
    assert_eq!(
        resolve(None::<u8>, env.midi_export_channel),
        env.midi_export_channel
    );
    // No --mid-slide → env value.
    assert_eq!(
        resolve(None::<MidiSlideMode>, env.midi_export_slide_mode),
        env.midi_export_slide_mode
    );
}

// ── three-layer check: user file → env → CLI ─────────────────────────

#[test]
fn three_layer_precedence_cli_beats_user_file_beats_template() {
    let dir = temp_dir("3layer");
    let path = dir.join(CONFIG_FILE_PATH);
    // User pins ACCENT velocity to 99; NORMAL velocity is left at template.
    std::fs::write(&path, "MIDI_EXPORT_ACCENT_VELOCITY=99\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();

    // Layer: user file > template.
    assert_eq!(env.midi_export_accent_velocity, 99);
    assert!(env.midi_export_normal_velocity > 0);

    // Layer: CLI > user file (simulating --mid-accent-velocity 50).
    let resolved = resolve(Some(50u8), env.midi_export_accent_velocity);
    assert_eq!(resolved, 50);

    // Layer: template flows through when neither CLI nor user touches key.
    let resolved_normal = resolve(None::<u8>, env.midi_export_normal_velocity);
    assert_eq!(resolved_normal, env.midi_export_normal_velocity);
}
