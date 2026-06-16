//! Regression tests proving that `TD3_CONFIG.env` values actually flow
//! into runtime behavior - what the env-hardcode audit
//! (`progress/20260424-015-env-hardcode-audit-src-ui.md`) called the
//! "set X in the env file, did it affect the running app" gap.
//!
//! Each test sets a non-default env value via the in-memory parser and
//! asserts that the corresponding runtime artefact (MidiExportOptions,
//! MidiImportOptions, etc.) reflects it. They do NOT touch real MIDI
//! hardware - the goal is to prove the wiring, not the protocol.

use crate::app_env::AppEnv;
use crate::config::MidiRuntime;
use crate::formats::mid::{MidiExportOptions, MidiSlideMode};
use crate::formats::mid_import::MidiImportOptions;
use crate::web::{midi_runtime_config_from_resolved, should_auto_connect_on_server_start};

/// Parse a synthetic env file and return the resolved `AppEnv`. Missing
/// keys fall back to the bundled template, so callers only need to
/// override the keys they're testing.
fn env_with_overrides(overrides: &str) -> AppEnv {
    let mut content = String::new();
    content.push_str(overrides);
    // The bundled template is the second layer in `parse_env` - re-use
    // the public `from_template` constructor and then layer overrides on
    // top by writing them through a fresh load. Simpler: just build the
    // env string from template + overrides.
    let template = crate::app_env::DEFAULT_TEMPLATE;
    let combined = format!("{}\n{}", template, content);
    parse_env_from(&combined)
}

fn parse_env_from(content: &str) -> AppEnv {
    use std::io::Write;
    static NEXT_ENV_FILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = NEXT_ENV_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "td3_env_runtime_wiring_{}_{}_{}.env",
        std::process::id(),
        unique,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let (env, _created) = AppEnv::load_or_create(&path).expect("env should parse");
    let _ = std::fs::remove_file(&path);
    env
}

// ---------------------------------------------------------------------------
// MidiExportOptions::from_env
// ---------------------------------------------------------------------------

#[test]
fn export_options_reflect_env_bpm() {
    let env = env_with_overrides("UI_DEFAULT_BPM=156\n");
    let opts = MidiExportOptions::from_env(&env);
    assert_eq!(opts.bpm, 156, "env BPM must flow into export options");
}

#[test]
fn export_options_reflect_env_velocities() {
    let env =
        env_with_overrides("MIDI_EXPORT_NORMAL_VELOCITY=42\nMIDI_EXPORT_ACCENT_VELOCITY=127\n");
    let opts = MidiExportOptions::from_env(&env);
    assert_eq!(opts.normal_velocity, 42);
    assert_eq!(opts.accent_velocity, 127);
}

#[test]
fn export_options_reflect_env_octave_offset() {
    let env = env_with_overrides("MIDI_EXPORT_OCTAVE_OFFSET=-5\n");
    let opts = MidiExportOptions::from_env(&env);
    assert_eq!(opts.octave_offset, -5);
}

#[test]
fn export_options_reflect_env_loop_count() {
    let env = env_with_overrides("MIDI_EXPORT_LOOP_COUNT=8\n");
    let opts = MidiExportOptions::from_env(&env);
    assert_eq!(opts.loop_count, 8);
}

#[test]
fn export_options_reflect_env_slide_mode() {
    let env = env_with_overrides("MIDI_EXPORT_SLIDE_MODE=\"none\"\n");
    let opts = MidiExportOptions::from_env(&env);
    assert_eq!(opts.slide_mode, MidiSlideMode::None);
}

#[test]
fn export_options_reflect_env_ppqn_and_channel() {
    let env = env_with_overrides("MIDI_EXPORT_PPQN=960\nMIDI_EXPORT_CHANNEL=10\n");
    let opts = MidiExportOptions::from_env(&env);
    assert_eq!(opts.ppqn, 960);
    assert_eq!(opts.channel, 10);
}

// ---------------------------------------------------------------------------
// MidiImportOptions::from_env - reuses MIDI_EXPORT_* keys
// ---------------------------------------------------------------------------

#[test]
fn import_options_reuse_export_octave_offset() {
    let env = env_with_overrides("MIDI_EXPORT_OCTAVE_OFFSET=7\n");
    let opts = MidiImportOptions::from_env(&env);
    assert_eq!(opts.octave_offset, 7, "import shares export octave offset");
}

#[test]
fn import_options_threshold_is_velocity_midpoint() {
    let env =
        env_with_overrides("MIDI_EXPORT_NORMAL_VELOCITY=40\nMIDI_EXPORT_ACCENT_VELOCITY=120\n");
    let opts = MidiImportOptions::from_env(&env);
    assert_eq!(opts.accent_threshold, 80, "midpoint of 40 and 120 is 80");
}

// ---------------------------------------------------------------------------
// AppEnv: full round-trip of MIDI keys
// ---------------------------------------------------------------------------

#[test]
fn env_carries_midi_runtime_keys() {
    let env = env_with_overrides(
        "MIDI_PORT_SUBSTRING=\"MyDevice\"\nMIDI_STRICT_NAME_MATCH=1\nMIDI_TIMEOUT_MS=12345\n",
    );
    assert_eq!(env.midi_port_substring, "MyDevice");
    assert!(env.midi_strict_name_match);
    assert_eq!(env.midi_timeout_ms, 12345);
}

#[test]
fn env_carries_ui_keys() {
    let env = env_with_overrides(
        "UI_DEFAULT_BPM=200\nUI_AUTO_CONNECT_TO_MIDI=0\nUI_AUTO_SET_LIVE_UPDATE=0\nUI_MAX_BANK_HISTORY_SIZE=42\n",
    );
    assert_eq!(env.ui_default_bpm, 200);
    assert!(!env.ui_auto_connect_to_midi);
    assert!(!env.ui_auto_set_live_update);
    assert_eq!(env.ui_max_bank_history_size, 42);
}

#[test]
fn startup_midi_gate_follows_env_false() {
    let env = env_with_overrides("UI_AUTO_CONNECT_TO_MIDI=0\n");

    assert!(!crate::should_run_startup_midi(&env));
    assert!(!should_auto_connect_on_server_start(&env));
}

#[test]
fn startup_midi_gate_follows_env_true() {
    let env = env_with_overrides("UI_AUTO_CONNECT_TO_MIDI=1\n");

    assert!(crate::should_run_startup_midi(&env));
    assert!(should_auto_connect_on_server_start(&env));
}

// ---------------------------------------------------------------------------
// Template-only path: a bare TD3_CONFIG.env (template defaults)
// ---------------------------------------------------------------------------

#[test]
fn template_only_yields_template_defaults() {
    // No overrides - the resolved env should match the bundled template
    // exactly. This is the contract that "first run with default file"
    // reproduces the shipped behaviour.
    let env = AppEnv::from_template().unwrap();
    let opts = MidiExportOptions::from_env(&env);
    // Spot-check a couple of values that the template fixes.
    assert!(opts.ppqn > 0);
    assert!(opts.channel >= 1 && opts.channel <= 16);
    assert!(opts.normal_velocity <= 127);
    assert!(opts.accent_velocity <= 127);
}

#[test]
fn web_midi_runtime_uses_resolved_midi_names() {
    let midi = MidiRuntime {
        input_port_name: "Exact TD-3 In".to_string(),
        output_port_name: "Exact TD-3 Out".to_string(),
        request_timeout: std::time::Duration::from_millis(2222),
        strict_name_match: true,
        retry_count: 3,
    };

    let runtime = midi_runtime_config_from_resolved(&midi);

    assert_eq!(runtime.input_port_name, "Exact TD-3 In");
    assert_eq!(runtime.output_port_name, "Exact TD-3 Out");
    assert_eq!(runtime.timeout, std::time::Duration::from_millis(2222));
    assert!(runtime.strict_name_match);
}
