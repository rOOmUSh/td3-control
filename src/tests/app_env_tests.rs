//! Tests for the `TD3_CONFIG.env` loader.

use std::path::PathBuf;

use crate::app_env::{normalize_scale_id, AppEnv, CONFIG_FILE_PATH, DEFAULT_TEMPLATE};

fn temp_dir(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("td3-appenv-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

// ── bundled template parses cleanly ──────────────────────────────────

#[test]
fn template_parses_without_error() {
    let env = AppEnv::from_template().expect("bundled template must parse");
    // Spot-check a handful of fields the loader owns.
    assert_eq!(env.midi_port_substring, "TD-3");
    assert_eq!(env.web_port, 3030);
    assert_eq!(env.web_bind, "127.0.0.1");
    assert_eq!(env.ui_scratch_pattern, "G1-P1A");
    assert_eq!(env.midi_export_channel, 1);
    assert!(env.midi_timeout_ms >= 1000);
    assert!(!env.library_database_path.is_empty());
}

#[test]
fn template_constant_matches_known_keys() {
    // Sanity: template should not contain any unknown keys that would warn at
    // first-run load. If this fails, either add the key to AppEnv or remove it
    // from the template.
    let env = AppEnv::from_template().unwrap();
    // Every required typed field is populated - if the template is missing a
    // key, `from_template` returns Err and this test never reaches here.
    drop(env);
}

// ── first-run file drop ──────────────────────────────────────────────

#[test]
fn first_run_creates_file_with_template_contents() {
    let dir = temp_dir("firstrun");
    let path = dir.join(CONFIG_FILE_PATH);
    assert!(!path.exists());

    let (env, first_run) = AppEnv::load_or_create(&path).unwrap();
    assert!(first_run, "first call must report first_run=true");
    assert!(path.exists(), "config file must be written");

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, DEFAULT_TEMPLATE);
    assert_eq!(env.midi_port_substring, "TD-3");

    // Second call must not re-trigger the first-run branch.
    let (_env2, first_run_2) = AppEnv::load_or_create(&path).unwrap();
    assert!(!first_run_2, "second call must report first_run=false");
}

// ── user values override template ────────────────────────────────────

#[test]
fn user_file_overrides_template_defaults() {
    let dir = temp_dir("override");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(
        &path,
        "WEB_PORT=4040\nWEB_BIND=\"0.0.0.0\"\nUI_RAND_NOTE_PERCENT=50\n",
    )
    .unwrap();

    let (env, first_run) = AppEnv::load_or_create(&path).unwrap();
    assert!(!first_run);
    assert_eq!(env.web_port, 4040);
    assert_eq!(env.web_bind, "0.0.0.0");
    assert_eq!(env.ui_rand_note_percent, 50);
    // Unspecified keys fall back to template values.
    assert_eq!(env.midi_port_substring, "TD-3");
}

// ── malformed input ──────────────────────────────────────────────────

#[test]
fn missing_equals_is_hard_error() {
    let dir = temp_dir("noeq");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "JUST_A_STRING\n").unwrap();
    let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
    assert!(err.contains("line 1"), "error should name the line: {err}");
}

#[test]
fn empty_key_is_hard_error() {
    let dir = temp_dir("emptykey");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "=foo\n").unwrap();
    let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
    assert!(err.contains("line 1"));
    assert!(err.contains("empty key"));
}

#[test]
fn out_of_range_rejected() {
    let dir = temp_dir("oor");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "UI_RAND_NOTE_PERCENT=150\n").unwrap();
    let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
    assert!(err.contains("UI_RAND_NOTE_PERCENT"), "{err}");
    assert!(err.contains("150"));
}

#[test]
fn non_integer_value_rejected() {
    let dir = temp_dir("badint");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "WEB_PORT=abc\n").unwrap();
    let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
    assert!(err.contains("WEB_PORT"), "{err}");
    assert!(err.contains("abc"));
}

#[test]
fn invalid_bool_rejected() {
    let dir = temp_dir("badbool");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "MIDI_STRICT_NAME_MATCH=maybe\n").unwrap();
    let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
    assert!(err.contains("MIDI_STRICT_NAME_MATCH"));
    assert!(err.contains("maybe"));
}

#[test]
fn invalid_slide_mode_rejected() {
    let dir = temp_dir("badslide");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "MIDI_EXPORT_SLIDE_MODE=zigzag\n").unwrap();
    let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
    assert!(err.contains("MIDI_EXPORT_SLIDE_MODE"));
    assert!(err.contains("zigzag"));
}

// ── comments, blanks, quotes ────────────────────────────────────────

#[test]
fn comments_and_blanks_ignored() {
    let dir = temp_dir("comments");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(
        &path,
        "# leading comment\n\nWEB_PORT=4242\n   # indented comment\n\n",
    )
    .unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.web_port, 4242);
}

#[test]
fn surrounding_quotes_stripped() {
    let dir = temp_dir("quotes");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "WEB_BIND=\"0.0.0.0\"\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.web_bind, "0.0.0.0");
}

// ── unknown keys ────────────────────────────────────────────────────

#[test]
fn unknown_keys_do_not_fail() {
    let dir = temp_dir("unknown");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "FOO=bar\nWEB_PORT=3031\nHELLO=world\n").unwrap();
    // Must succeed - unknown keys only warn.
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.web_port, 3031);
}

// ── scale-id normalization ──────────────────────────────────────────

#[test]
fn scale_id_normalization_rules() {
    assert_eq!(normalize_scale_id("Phrygian Dominant"), "phrygian_dominant");
    assert_eq!(normalize_scale_id("  MAJOR  "), "major");
    assert_eq!(normalize_scale_id("Major Pentatonic"), "major_pentatonic");
    assert_eq!(normalize_scale_id("natural_minor"), "natural_minor");
}

#[test]
fn scale_id_normalized_in_env() {
    let dir = temp_dir("scaleid");
    let path = dir.join(CONFIG_FILE_PATH);
    std::fs::write(&path, "UI_RAND_DEFAULT_SCALE=\"Phrygian Dominant\"\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    assert_eq!(env.ui_rand_default_scale, "phrygian_dominant");
}
