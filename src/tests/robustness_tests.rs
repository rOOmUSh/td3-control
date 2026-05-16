//! P8.3 - Property/fuzz-style robustness tests.
//! P8.4 - Round-trip invariant matrix.
//!
//! Tests parser resilience against malformed input and verifies
//! that all format conversion chains preserve pattern semantics.

use crate::formats::{json, steps_txt, syx, toml_fmt};
use crate::pattern::{pattern_to_sysex, sysex_to_pattern, Pattern};
use crate::step::{Accent, Slide, Step, Time, Transpose};

use super::fixtures;

// ===========================================================================
// Helper
// ===========================================================================

fn assert_patterns_equal(a: &Pattern, b: &Pattern) {
    assert_eq!(a.active_steps, b.active_steps, "active_steps");
    assert_eq!(a.triplet, b.triplet, "triplet");
    for i in 0..16 {
        assert_eq!(a.step[i].note, b.step[i].note, "step {} note", i + 1);
        assert_eq!(
            a.step[i].transpose,
            b.step[i].transpose,
            "step {} transpose",
            i + 1
        );
        assert_eq!(a.step[i].accent, b.step[i].accent, "step {} accent", i + 1);
        assert_eq!(a.step[i].slide, b.step[i].slide, "step {} slide", i + 1);
        assert_eq!(a.step[i].time, b.step[i].time, "step {} time", i + 1);
    }
}

/// Build a pattern that exercises note/transpose/accent variety and is
/// compatible with 303 packed-note SysEx round-trips.
///
/// Constraints for SysEx round-trip fidelity:
///   - Time: only Normal and Rest (TIE is a SysEx-internal concept)
///   - Slide on step N requires step N+1 to have the same note/transpose
///     (because SLIDE→TIE causes step N+1 to hold step N's note)
///   - Rest steps carry the last Normal step's note/transpose
fn varied_pattern() -> Pattern {
    let mut steps: [Step; 16] = Default::default();
    let notes = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0];
    let transposes = [Transpose::Down, Transpose::Normal, Transpose::Up];

    for i in 0..16 {
        steps[i] = Step {
            note: notes[i % notes.len()],
            transpose: transposes[i % transposes.len()],
            accent: if i % 3 == 0 { Accent::On } else { Accent::Off },
            slide: Slide::Off,
            time: if i % 5 == 4 { Time::Rest } else { Time::Normal },
        };
    }

    // Add slides where round-trip safe: step N has slide, step N+1 is Normal
    // with the same note (held via TIE in SysEx).
    steps[0].slide = Slide::On;
    steps[1].note = steps[0].note;
    steps[1].transpose = steps[0].transpose;

    steps[7].slide = Slide::On;
    steps[8].note = steps[7].note;
    steps[8].transpose = steps[7].transpose;

    // Fix Rest steps LAST: they must carry the preceding Normal step's note/transpose
    // (303 packed format doesn't store notes for Rest steps).
    // Accent and slide on Rest steps are also cleared - the packed format only
    // stores accent/slide for Normal steps (same packing as notes).
    // Must run after all note adjustments above.
    let mut last_note = 0u8;
    let mut last_transpose = Transpose::Normal;
    for step in steps.iter_mut() {
        if step.time == Time::Normal {
            last_note = step.note;
            last_transpose = step.transpose;
        } else {
            step.note = last_note;
            step.transpose = last_transpose;
            step.accent = Accent::Off;
            step.slide = Slide::Off;
        }
    }

    Pattern::new(true, 16, steps).unwrap()
}

// ===========================================================================
// P8.3: SysEx decode robustness
// ===========================================================================

#[test]
fn sysex_garbage_bytes_rejected() {
    // Pure garbage
    assert!(sysex_to_pattern(&[0xFF; 115]).is_err());
    assert!(sysex_to_pattern(&[0x00; 115]).is_err());
}

#[test]
fn sysex_all_zeros_except_msg_id() {
    let mut payload = vec![0u8; 115];
    payload[0] = 0x78;
    // All-zero note bytes produce note=0 → octave=0 → transpose underflows.
    // The decoder must return an error (not panic).
    let result = sysex_to_pattern(&payload);
    assert!(
        result.is_err(),
        "all-zero payload must be rejected, not panic"
    );
}

#[test]
fn sysex_max_valid_values() {
    // Build a payload that pushes boundaries: note=12 (C^), Up transpose, all accents.
    // All Normal steps (no slides - slide on last step can't round-trip since there's
    // no step 17 to become TIE).
    let pat = Pattern::new(
        true, // triplet
        16,
        [Step {
            note: 12,
            transpose: Transpose::Up,
            accent: Accent::On,
            slide: Slide::Off,
            time: Time::Normal,
        }; 16],
    )
    .unwrap();
    let sysex = pattern_to_sysex(&pat, 3, 7, 1).unwrap(); // max patgroup/slot/side
    let decoded = sysex_to_pattern(&sysex).unwrap();
    assert_patterns_equal(&pat, &decoded);
}

#[test]
fn sysex_min_active_steps() {
    let pat = Pattern::new(false, 1, Default::default()).unwrap();
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let decoded = sysex_to_pattern(&sysex).unwrap();
    assert_eq!(decoded.active_steps, 1);
}

#[test]
fn sysex_extra_trailing_bytes_rejected() {
    let mut payload = fixtures::REAL_G1_P4A_PAYLOAD.to_vec();
    payload.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
    let err = sysex_to_pattern(&payload).unwrap_err().to_string();
    assert!(
        err.contains("payload length mismatch"),
        "expected payload length error, got: {}",
        err
    );
}

// ===========================================================================
// P8.3: Steps DSL additional parser robustness
// ===========================================================================

#[test]
fn steps_only_whitespace() {
    assert!(steps_txt::import("   \n\n\n  \n").is_err());
}

#[test]
fn steps_only_comments() {
    assert!(steps_txt::import("# this is a comment\n# another\n").is_err());
}

#[test]
fn steps_header_only_no_body() {
    assert!(
        steps_txt::import("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n").is_err()
    );
}

#[test]
fn steps_garbage_between_header_and_rows() {
    let canonical = steps_txt::export(&Pattern::default());
    // Insert garbage after the blank line separator
    let modified = canonical.replacen("\n\n", "\n\nGARBAGE LINE HERE\n", 1);
    let result = steps_txt::import(&modified);
    assert!(result.is_err());
}

// ===========================================================================
// P8.3: Steps DSL parser robustness
// ===========================================================================

#[test]
fn steps_empty_input() {
    assert!(steps_txt::import("").is_err());
}

#[test]
fn steps_only_header_no_steps() {
    let data = "format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n";
    assert!(steps_txt::import(data).is_err());
}

#[test]
fn steps_duplicate_step_index() {
    // Step 1 appears twice, step 16 missing
    let mut lines = String::from("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n");
    lines.push_str("01  C:---:N\n");
    lines.push_str("01  D:---:N\n"); // duplicate
    for i in 2..=15 {
        lines.push_str(&format!("{:02}  C:---:N\n", i));
    }
    // Missing step 16 → error
    assert!(steps_txt::import(&lines).is_err());
}

#[test]
fn steps_note_with_hash_not_comment() {
    // C# contains '#' - must not be treated as comment
    let mut lines = String::from("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n");
    for i in 1..=16 {
        lines.push_str(&format!("{:02} C#:---:N\n", i));
    }
    let result = steps_txt::import(&lines);
    assert!(result.is_ok());
    let pat = result.unwrap();
    assert_eq!(pat.step[0].note, 1); // C# = note 1
}

// ===========================================================================
// P8.3: JSON parser robustness
// ===========================================================================

#[test]
fn json_empty_input() {
    assert!(json::import("").is_err());
}

#[test]
fn json_valid_json_wrong_structure() {
    assert!(json::import(r#"{"hello": "world"}"#).is_err());
}

#[test]
fn json_null_input() {
    assert!(json::import("null").is_err());
}

#[test]
fn json_array_input() {
    assert!(json::import("[]").is_err());
}

// ===========================================================================
// P8.3: TOML parser robustness
// ===========================================================================

#[test]
fn toml_empty_input() {
    assert!(toml_fmt::import("").is_err());
}

#[test]
fn toml_valid_toml_wrong_structure() {
    assert!(toml_fmt::import("[package]\nname = \"hello\"\n").is_err());
}

// ===========================================================================
// P8.3: SYX file import robustness
// ===========================================================================

#[test]
fn syx_empty_file() {
    assert!(syx::import(&[]).is_err());
}

#[test]
fn syx_only_start_byte() {
    assert!(syx::import(&[0xF0]).is_err());
}

#[test]
fn syx_header_but_no_payload() {
    let data = &[0xF0, 0x00, 0x20, 0x32, 0x00, 0x01, 0x0A, 0xF7];
    assert!(syx::import(data).is_err());
}

#[test]
fn syx_wrong_start_byte() {
    let mut data = fixtures::REAL_G1_P4A_SYX_FILE.to_vec();
    data[0] = 0x90; // Note On instead of SysEx start
    assert!(syx::import(&data).is_err());
}

// ===========================================================================
// P8.4: Round-trip invariant matrix
// ===========================================================================
//
// Invariant definitions:
//   SEMANTIC: Pattern field values are identical after round-trip.
//   CANONICAL: Exported text is identical after round-trip (implies SEMANTIC).
//   BYTE-EXACT: Byte output is identical (only for sysex→sysex with same address).

// --- Single-format round-trips (all CANONICAL) ---

#[test]
fn matrix_sysex_roundtrip() {
    // sysex → Pattern → sysex (SEMANTIC - unknown1 bytes may differ)
    let pat = varied_pattern();
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let decoded = sysex_to_pattern(&sysex).unwrap();
    let re_encoded = pattern_to_sysex(&decoded, 0, 0, 0).unwrap();
    assert_eq!(sysex, re_encoded, "sysex round-trip must be byte-exact");
    assert_patterns_equal(&pat, &decoded);
}

#[test]
fn matrix_steps_roundtrip_canonical_varied() {
    // steps → Pattern → steps (CANONICAL) - varied pattern
    let pat = varied_pattern();
    let steps1 = steps_txt::export(&pat);
    let imported = steps_txt::import(&steps1).unwrap();
    let steps2 = steps_txt::export(&imported);
    assert_eq!(
        steps1, steps2,
        "Steps round-trip must be canonical (varied)"
    );
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn matrix_steps_roundtrip_canonical() {
    // steps → Pattern → steps (CANONICAL)
    let pat = varied_pattern();
    let steps1 = steps_txt::export(&pat);
    let imported = steps_txt::import(&steps1).unwrap();
    let steps2 = steps_txt::export(&imported);
    assert_eq!(steps1, steps2, "Steps round-trip must be canonical");
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn matrix_toml_roundtrip_canonical() {
    // toml → Pattern → toml (CANONICAL)
    let pat = varied_pattern();
    let toml1 = toml_fmt::export(&pat).unwrap();
    let imported = toml_fmt::import(&toml1).unwrap();
    let toml2 = toml_fmt::export(&imported).unwrap();
    assert_eq!(toml1, toml2, "TOML round-trip must be canonical");
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn matrix_json_roundtrip_canonical() {
    // json → Pattern → json (CANONICAL)
    let pat = varied_pattern();
    let json1 = json::export(&pat).unwrap();
    let imported = json::import(&json1).unwrap();
    let json2 = json::export(&imported).unwrap();
    assert_eq!(json1, json2, "JSON round-trip must be canonical");
    assert_patterns_equal(&pat, &imported);
}

// --- Cross-format chains (all SEMANTIC) ---

#[test]
fn matrix_steps_to_sysex_to_steps_varied() {
    let pat = varied_pattern();
    let steps1 = steps_txt::export(&pat);
    let p1 = steps_txt::import(&steps1).unwrap();
    let sysex = pattern_to_sysex(&p1, 0, 0, 0).unwrap();
    let p2 = sysex_to_pattern(&sysex).unwrap();
    let steps2 = steps_txt::export(&p2);
    assert_eq!(steps1, steps2, "Steps → SysEx → Steps must be canonical");
}

#[test]
fn matrix_toml_to_json() {
    let pat = varied_pattern();
    let toml_str = toml_fmt::export(&pat).unwrap();
    let p1 = toml_fmt::import(&toml_str).unwrap();
    let json_str = json::export(&p1).unwrap();
    let p2 = json::import(&json_str).unwrap();
    assert_patterns_equal(&pat, &p2);
}

#[test]
fn matrix_steps_to_sysex_to_steps() {
    let pat = varied_pattern();
    let steps1 = steps_txt::export(&pat);
    let p1 = steps_txt::import(&steps1).unwrap();
    let sysex = pattern_to_sysex(&p1, 0, 0, 0).unwrap();
    let p2 = sysex_to_pattern(&sysex).unwrap();
    let steps2 = steps_txt::export(&p2);
    assert_eq!(steps1, steps2, "Steps → SysEx → Steps must be canonical");
}

#[test]
fn matrix_all_formats_chain() {
    // steps → toml → json → sysex → steps
    let pat = varied_pattern();
    let steps1 = steps_txt::export(&pat);
    let p1 = steps_txt::import(&steps1).unwrap();

    let toml_str = toml_fmt::export(&p1).unwrap();
    let p2 = toml_fmt::import(&toml_str).unwrap();

    let json_str = json::export(&p2).unwrap();
    let p3 = json::import(&json_str).unwrap();

    let sysex = pattern_to_sysex(&p3, 0, 0, 0).unwrap();
    let p4 = sysex_to_pattern(&sysex).unwrap();

    let steps2 = steps_txt::export(&p4);
    assert_eq!(
        steps1, steps2,
        "full 4-format chain must preserve canonical Steps"
    );
}

// --- Edge-case patterns through the matrix ---

#[test]
fn matrix_default_pattern_all_formats() {
    let pat = Pattern::default();
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let p1 = sysex_to_pattern(&sysex).unwrap();
    let steps_str = steps_txt::export(&p1);
    let p2 = steps_txt::import(&steps_str).unwrap();
    let toml_str = toml_fmt::export(&p2).unwrap();
    let p3 = toml_fmt::import(&toml_str).unwrap();
    let json_str = json::export(&p3).unwrap();
    let p4 = json::import(&json_str).unwrap();
    assert_patterns_equal(&pat, &p4);
}

#[test]
fn matrix_single_active_step_roundtrip() {
    let pat = Pattern::new(false, 1, Default::default()).unwrap();
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let decoded = sysex_to_pattern(&sysex).unwrap();
    assert_eq!(decoded.active_steps, 1);
    assert_patterns_equal(&pat, &decoded);
}

#[test]
fn matrix_all_time_states_through_all_formats() {
    // After 303 refactoring, TIE is a SysEx-internal concept.
    // The Pattern model uses Normal and Rest (plus slide for forward-looking ties).
    // Test Normal and Rest cycling through all formats including SysEx.
    let mut steps: [Step; 16] = Default::default();
    for (i, step) in steps.iter_mut().enumerate() {
        step.time = if i % 3 == 2 { Time::Rest } else { Time::Normal };
    }
    // Fix Rest steps: carry preceding Normal step's note
    let mut last_note = 0u8;
    let mut last_transpose = Transpose::Normal;
    for step in steps.iter_mut() {
        if step.time == Time::Normal {
            last_note = step.note;
            last_transpose = step.transpose;
        } else {
            step.note = last_note;
            step.transpose = last_transpose;
        }
    }
    let pat = Pattern::new(false, 16, steps).unwrap();

    // SysEx round-trip
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let p1 = sysex_to_pattern(&sysex).unwrap();
    assert_patterns_equal(&pat, &p1);

    // Steps round-trip
    let steps_str = steps_txt::export(&p1);
    let p2 = steps_txt::import(&steps_str).unwrap();
    assert_patterns_equal(&pat, &p2);

    // TOML round-trip
    let toml_str = toml_fmt::export(&p2).unwrap();
    let p3 = toml_fmt::import(&toml_str).unwrap();
    assert_patterns_equal(&pat, &p3);

    // JSON round-trip
    let json_str = json::export(&p3).unwrap();
    let p4 = json::import(&json_str).unwrap();
    assert_patterns_equal(&pat, &p4);
}

#[test]
fn text_formats_preserve_tie_and_tierest() {
    // Text formats (Steps, TOML, JSON) preserve Tie and TieRest values
    // even though SysEx round-trip converts them. This tests text-only preservation.
    let mut steps: [Step; 16] = Default::default();
    let times = [Time::Normal, Time::Tie, Time::Rest, Time::TieRest];
    for i in 0..16 {
        steps[i].time = times[i % 4];
    }
    let pat = Pattern::new(false, 16, steps).unwrap();

    // Steps round-trip
    let steps_str = steps_txt::export(&pat);
    let p1 = steps_txt::import(&steps_str).unwrap();
    assert_patterns_equal(&pat, &p1);

    // TOML round-trip
    let toml_str = toml_fmt::export(&pat).unwrap();
    let p2 = toml_fmt::import(&toml_str).unwrap();
    assert_patterns_equal(&pat, &p2);

    // JSON round-trip
    let json_str = json::export(&pat).unwrap();
    let p3 = json::import(&json_str).unwrap();
    assert_patterns_equal(&pat, &p3);
}

#[test]
fn matrix_upper_c_through_all_formats() {
    // C^ (note=12) with Up transpose is the highest pitch.
    // Slide on steps 0-14 (not 15 - slide on the last step can't convert to TIE
    // because there is no step 16).
    let mut steps = [Step {
        note: 12,
        transpose: Transpose::Up,
        accent: Accent::On,
        slide: Slide::On,
        time: Time::Normal,
    }; 16];
    steps[15].slide = Slide::Off; // last step: no slide (can't round-trip)
    let pat = Pattern::new(true, 16, steps).unwrap();

    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let p1 = sysex_to_pattern(&sysex).unwrap();
    assert_patterns_equal(&pat, &p1);

    let steps_str = steps_txt::export(&p1);
    let p2 = steps_txt::import(&steps_str).unwrap();
    assert_patterns_equal(&pat, &p2);

    let toml_str = toml_fmt::export(&p2).unwrap();
    let p3 = toml_fmt::import(&toml_str).unwrap();
    assert_patterns_equal(&pat, &p3);
}
