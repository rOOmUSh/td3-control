//! Tests for the per-field validators in `env_metadata`.
//!
//! The loader in `app_env.rs` is deliberately permissive for
//! backward-compat with hand-edited files. The Settings UI writes
//! through this stricter gate. These tests pin the stricter ranges so
//! a casual loosening elsewhere can't silently widen what the UI will
//! persist.

use crate::env_metadata::{find, validate_value, FieldKind, FIELDS, SECTIONS};

// ── table invariants ─────────────────────────────────────────────────

#[test]
fn every_field_has_a_known_section() {
    let section_ids: Vec<&str> = SECTIONS.iter().map(|s| s.id).collect();
    for f in FIELDS {
        assert!(
            section_ids.contains(&f.section_id),
            "field {} references unknown section '{}'",
            f.key,
            f.section_id
        );
    }
}

#[test]
fn field_keys_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for f in FIELDS {
        assert!(seen.insert(f.key), "duplicate key in FIELDS: {}", f.key);
    }
}

#[test]
fn find_roundtrips_every_field() {
    for f in FIELDS {
        let meta = find(f.key).unwrap_or_else(|| panic!("find() lost key {}", f.key));
        assert_eq!(meta.key, f.key);
    }
}

#[test]
fn find_rejects_unknown_keys() {
    assert!(find("NOT_A_REAL_KEY").is_none());
    assert!(find("").is_none());
}

// ── unknown keys are rejected ────────────────────────────────────────

#[test]
fn validate_rejects_unknown_key() {
    let err = validate_value("TOTALLY_UNKNOWN", "42").unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("TOTALLY_UNKNOWN"), "message was: {}", msg);
}

// ── String ───────────────────────────────────────────────────────────

#[test]
fn string_rejects_empty_and_whitespace() {
    assert!(validate_value("MIDI_PORT_SUBSTRING", "").is_err());
    assert!(validate_value("MIDI_PORT_SUBSTRING", "   ").is_err());
}

#[test]
fn string_accepts_non_empty() {
    assert!(validate_value("MIDI_PORT_SUBSTRING", "TD-3").is_ok());
    assert!(validate_value("WEB_BIND", "0.0.0.0").is_ok());
}

// ── Integer ──────────────────────────────────────────────────────────

#[test]
fn integer_rejects_non_numeric() {
    assert!(validate_value("WEB_PORT", "abc").is_err());
    assert!(validate_value("WEB_PORT", "").is_err());
    assert!(validate_value("WEB_PORT", "12.5").is_err());
}

#[test]
fn integer_enforces_lower_bound() {
    assert!(validate_value("WEB_PORT", "0").is_err());
    assert!(validate_value("WEB_PORT", "1").is_ok());
}

#[test]
fn integer_enforces_upper_bound() {
    assert!(validate_value("WEB_PORT", "65535").is_ok());
    assert!(validate_value("WEB_PORT", "65536").is_err());
}

#[test]
fn integer_accepts_negative_when_allowed() {
    // MIDI_EXPORT_OCTAVE_OFFSET is signed.
    assert!(validate_value("MIDI_EXPORT_OCTAVE_OFFSET", "-60").is_ok());
    assert!(validate_value("MIDI_EXPORT_OCTAVE_OFFSET", "-61").is_err());
    assert!(validate_value("MIDI_EXPORT_OCTAVE_OFFSET", "0").is_ok());
    assert!(validate_value("MIDI_EXPORT_OCTAVE_OFFSET", "60").is_ok());
    assert!(validate_value("MIDI_EXPORT_OCTAVE_OFFSET", "61").is_err());
}

#[test]
fn integer_ui_rand_percent_clamps_to_0_100() {
    assert!(validate_value("UI_RAND_NOTE_PERCENT", "0").is_ok());
    assert!(validate_value("UI_RAND_NOTE_PERCENT", "100").is_ok());
    assert!(validate_value("UI_RAND_NOTE_PERCENT", "101").is_err());
}

#[test]
fn integer_bpm_range_is_20_to_300() {
    assert!(validate_value("UI_DEFAULT_BPM", "19").is_err());
    assert!(validate_value("UI_DEFAULT_BPM", "20").is_ok());
    assert!(validate_value("UI_DEFAULT_BPM", "300").is_ok());
    assert!(validate_value("UI_DEFAULT_BPM", "301").is_err());
}

// ── Bool ─────────────────────────────────────────────────────────────

#[test]
fn bool_accepts_documented_forms() {
    for v in &["0", "1", "true", "false", "TRUE", "FALSE", "yes", "no"] {
        assert!(
            validate_value("UI_AUTO_CONNECT_TO_MIDI", v).is_ok(),
            "expected '{}' to validate as bool",
            v
        );
    }
}

#[test]
fn bool_rejects_garbage() {
    assert!(validate_value("UI_AUTO_CONNECT_TO_MIDI", "maybe").is_err());
    assert!(validate_value("UI_AUTO_CONNECT_TO_MIDI", "2").is_err());
    assert!(validate_value("UI_AUTO_CONNECT_TO_MIDI", "").is_err());
}

// ── Enum ─────────────────────────────────────────────────────────────

#[test]
fn enum_slide_mode_accepts_documented_options() {
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "td3").is_ok());
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "generic").is_ok());
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "none").is_ok());
}

#[test]
fn enum_slide_mode_is_case_insensitive() {
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "TD3").is_ok());
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "Generic").is_ok());
}

#[test]
fn enum_slide_mode_rejects_unknown() {
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "").is_err());
    assert!(validate_value("MIDI_EXPORT_SLIDE_MODE", "weird").is_err());
}

// ── ScaleId (non-empty string gate; deeper check is UI-side) ─────────

#[test]
fn scale_id_rejects_empty() {
    assert!(validate_value("UI_RAND_DEFAULT_SCALE", "").is_err());
}

#[test]
fn scale_id_accepts_non_empty() {
    // Server-side guard is non-empty; the UI restricts via dropdown.
    assert!(validate_value("UI_RAND_DEFAULT_SCALE", "minor").is_ok());
}

// ── UI_SCRATCH_PATTERN (custom validator calls config::parse_scratch_pattern) ─

#[test]
fn scratch_pattern_accepts_valid_forms() {
    assert!(validate_value("UI_SCRATCH_PATTERN", "G1-P2A").is_ok());
    assert!(validate_value("UI_SCRATCH_PATTERN", "G4-P8B").is_ok());
}

#[test]
fn scratch_pattern_rejects_out_of_range() {
    assert!(validate_value("UI_SCRATCH_PATTERN", "G5-P1A").is_err());
    assert!(validate_value("UI_SCRATCH_PATTERN", "G1-P9A").is_err());
    assert!(validate_value("UI_SCRATCH_PATTERN", "G1-P1C").is_err());
}

#[test]
fn scratch_pattern_rejects_garbage() {
    assert!(validate_value("UI_SCRATCH_PATTERN", "nope").is_err());
    assert!(validate_value("UI_SCRATCH_PATTERN", "").is_err());
}

// ── FieldKind spot checks - guard against accidental range edits ─────

#[test]
fn web_port_kind_range() {
    let meta = find("WEB_PORT").unwrap();
    match &meta.kind {
        FieldKind::Integer { min, max } => {
            assert_eq!(*min, 1);
            assert_eq!(*max, 65_535);
        }
        _ => panic!("WEB_PORT must be Integer"),
    }
}

#[test]
fn midi_export_velocity_kind_range() {
    for k in ["MIDI_EXPORT_NORMAL_VELOCITY", "MIDI_EXPORT_ACCENT_VELOCITY"] {
        let meta = find(k).unwrap();
        match &meta.kind {
            FieldKind::Integer { min, max } => {
                assert_eq!(*min, 0, "{}", k);
                assert_eq!(*max, 127, "{}", k);
            }
            _ => panic!("{} must be Integer", k),
        }
    }
}
