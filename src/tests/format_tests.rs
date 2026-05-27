use crate::formats;
use crate::formats::{json, steps_txt, syx, toml_fmt, Format};
use crate::pattern::{pattern_to_sysex, sysex_to_pattern, Pattern};

use super::fixtures;

// ---------------------------------------------------------------------------
// Helper: build a non-trivial test pattern
// ---------------------------------------------------------------------------

fn test_pattern() -> Pattern {
    let text = include_str!("../../tests/fixtures/all_features.steps.txt");
    steps_txt::import(text).expect("fixture parse failed")
}

/// A pattern that survives SysEx round-trip (no Tie/TieRest, Rest steps carry
/// preceding Normal note, slides followed by matching held note).
fn sysex_safe_pattern() -> Pattern {
    use crate::step::{Accent, Slide, Step, Time, Transpose};
    let mut steps: [Step; 16] = Default::default();
    let notes: [u8; 13] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let transposes = [Transpose::Down, Transpose::Normal, Transpose::Up];
    for i in 0..16 {
        steps[i] = Step {
            note: notes[i % 13],
            transpose: transposes[i % 3],
            accent: if i % 3 == 0 { Accent::On } else { Accent::Off },
            slide: Slide::Off,
            time: if i % 5 == 4 { Time::Rest } else { Time::Normal },
        };
    }
    // Fix Rest steps: carry preceding Normal step's note, clear accent/slide
    // (303 packed format only stores accent/slide for Normal steps)
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
    // Add a slide: step 0 → step 1 (step 1 note matches step 0 via TIE hold)
    steps[0].slide = Slide::On;
    steps[1].note = steps[0].note;
    steps[1].transpose = steps[0].transpose;
    Pattern::new(true, 16, steps).unwrap()
}

// ===========================================================================
// TOML format
// ===========================================================================

#[test]
fn toml_roundtrip() {
    let pat = test_pattern();
    let exported = toml_fmt::export(&pat).unwrap();
    let imported = toml_fmt::import(&exported).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn toml_roundtrip_default() {
    let pat = Pattern::default();
    let exported = toml_fmt::export(&pat).unwrap();
    let imported = toml_fmt::import(&exported).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn toml_contains_expected_fields() {
    let pat = test_pattern();
    let exported = toml_fmt::export(&pat).unwrap();
    assert!(exported.contains("format = \"td3-control\""));
    assert!(exported.contains("format_version = 1"));
    assert!(exported.contains("device = \"TD-3\""));
    assert!(exported.contains("active_steps = 16"));
    assert!(exported.contains("triplet_time = true"));
    assert!(exported.contains("[[steps]]"));
}

#[test]
fn toml_rejects_wrong_format() {
    let bad = "format = \"not-td3\"\nformat_version = 1\ndevice = \"TD-3\"\n\
        active_steps = 16\ntriplet_time = false\nsteps = []\n";
    assert!(toml_fmt::import(bad).is_err());
}

#[test]
fn toml_rejects_invalid_active_steps() {
    let pat = test_pattern();
    let mut exported = toml_fmt::export(&pat).unwrap();
    exported = exported.replace("active_steps = 16", "active_steps = 0");
    assert!(toml_fmt::import(&exported).is_err());
}

// ===========================================================================
// JSON format
// ===========================================================================

#[test]
fn json_roundtrip() {
    let pat = test_pattern();
    let exported = json::export(&pat).unwrap();
    let imported = json::import(&exported).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn json_roundtrip_default() {
    let pat = Pattern::default();
    let exported = json::export(&pat).unwrap();
    let imported = json::import(&exported).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn json_contains_expected_fields() {
    let pat = test_pattern();
    let exported = json::export(&pat).unwrap();
    assert!(exported.contains("\"format\": \"td3-control\""));
    assert!(exported.contains("\"format_version\": 1"));
    assert!(exported.contains("\"steps\""));
}

#[test]
fn json_rejects_wrong_format() {
    let bad = r#"{"format":"wrong","format_version":1,"device":"TD-3","active_steps":16,"triplet_time":false,"steps":[]}"#;
    assert!(json::import(bad).is_err());
}

#[test]
fn json_rejects_duplicate_step_index() {
    let step = r#"{"index":1,"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let mut steps = vec![step.to_string(), step.to_string()];
    for i in 2..=15 {
        steps.push(format!(
            r#"{{"index":{},"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}}"#,
            i
        ));
    }
    let body = steps.join(",");
    let bad = format!(
        r#"{{"format":"td3-control","format_version":1,"device":"TD-3","active_steps":16,"triplet_time":false,"steps":[{}]}}"#,
        body
    );
    assert!(json::import(&bad).is_err());
}

// ===========================================================================
// Steps DSL format
// ===========================================================================

#[test]
fn steps_roundtrip() {
    let pat = test_pattern();
    let exported = steps_txt::export(&pat);
    let imported = steps_txt::import(&exported).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn steps_roundtrip_default() {
    let pat = Pattern::default();
    let exported = steps_txt::export(&pat);
    let imported = steps_txt::import(&exported).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn steps_contains_header() {
    let pat = test_pattern();
    let exported = steps_txt::export(&pat);
    assert!(exported.starts_with("format=td3-stepdsl-v1\n"));
    assert!(exported.contains("active_steps=16\n"));
    assert!(exported.contains("triplet_time=on\n"));
}

#[test]
fn steps_contains_all_16_lines() {
    let pat = test_pattern();
    let exported = steps_txt::export(&pat);
    for i in 1..=16 {
        assert!(
            exported.contains(&format!("{:02} ", i)),
            "missing step {}",
            i
        );
    }
}

#[test]
fn steps_rejects_missing_steps() {
    let data = "format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n\
        01  C:---:N\n02  C:---:N\n";
    assert!(steps_txt::import(data).is_err());
}

#[test]
fn steps_accepts_rows_only_through_active_steps() {
    let data = include_str!("../../tests/fixtures/eight_step_active_only.steps.txt");
    let pat = steps_txt::import(data).unwrap();
    assert_eq!(pat.active_steps, 8);
    assert!(!pat.triplet);
    assert_eq!(pat.step[4].slide, crate::step::Slide::On);
    assert_eq!(pat.step[8].note, 0);
    assert_eq!(pat.step[8].time, crate::step::Time::Normal);
}

#[test]
fn steps_rejects_missing_declared_active_step() {
    let data = "format=td3-stepdsl-v1\nactive_steps=8\ntriplet_time=off\n\n\
        01  C:---:N\n02  C:---:N\n03  C:---:N\n04  C:---:N\n\
        05  C:---:N\n06  C:---:N\n07  C:---:N\n";
    let err = steps_txt::import(data).unwrap_err().to_string();
    assert!(err.contains("missing steps: [8]"), "got: {}", err);
}

#[test]
fn steps_rejects_invalid_time() {
    let mut lines = String::from("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n");
    for i in 1..=16 {
        lines.push_str(&format!("{:02}  C:---:X\n", i)); // X is not valid
    }
    assert!(steps_txt::import(&lines).is_err());
}

#[test]
fn steps_rejects_invalid_active_steps() {
    let mut lines = String::from("format=td3-stepdsl-v1\nactive_steps=0\ntriplet_time=off\n\n");
    for i in 1..=16 {
        lines.push_str(&format!("{:02}  C:---:N\n", i));
    }
    assert!(steps_txt::import(&lines).is_err());
}

// ===========================================================================
// SYX format
// ===========================================================================

#[test]
fn syx_roundtrip() {
    let pat = sysex_safe_pattern();
    let syx_data = syx::export(&pat, 0, 0, 0).unwrap();
    let imported = syx::import(&syx_data).unwrap();
    assert_patterns_equal(&pat, &imported);
}

#[test]
fn syx_roundtrip_raw() {
    let payload = fixtures::simple_sysex();
    let syx_data = syx::export_raw(&payload);
    let imported = syx::import(&syx_data).unwrap();
    let decoded = sysex_to_pattern(&payload).unwrap();
    assert_patterns_equal(&decoded, &imported);
}

#[test]
fn syx_rejects_truncated() {
    assert!(syx::import(&[0xF0, 0x00]).is_err());
}

#[test]
fn syx_rejects_wrong_header() {
    let mut data = syx::export(&Pattern::default(), 0, 0, 0).unwrap();
    data[1] = 0xFF; // corrupt manufacturer
    assert!(syx::import(&data).is_err());
}

#[test]
fn syx_rejects_missing_terminator() {
    let mut data = syx::export(&Pattern::default(), 0, 0, 0).unwrap();
    let len = data.len();
    data[len - 1] = 0x00; // corrupt F7
    assert!(syx::import(&data).is_err());
}

#[test]
fn syx_rejects_trailing_byte_after_complete_frame() {
    let mut data = syx::export(&Pattern::default(), 0, 0, 0).unwrap();
    data.push(0x00);
    let err = syx::import(&data).unwrap_err().to_string();
    assert!(
        err.contains("unexpected length"),
        "expected length error, got: {}",
        err
    );
}

#[test]
fn syx_rejects_concatenated_frames() {
    let mut data = syx::export(&Pattern::default(), 0, 0, 0).unwrap();
    let second = syx::export(&Pattern::default(), 0, 0, 0).unwrap();
    data.extend_from_slice(&second);
    let err = syx::import(&data).unwrap_err().to_string();
    assert!(
        err.contains("unexpected length"),
        "expected length error, got: {}",
        err
    );
}

// ===========================================================================
// Cross-format conversion tests
// ===========================================================================

#[test]
fn sysex_to_toml_to_sysex() {
    let payload = fixtures::simple_sysex();
    let pat = sysex_to_pattern(&payload).unwrap();
    let toml_str = toml_fmt::export(&pat).unwrap();
    let pat2 = toml_fmt::import(&toml_str).unwrap();
    let sysex2 = pattern_to_sysex(&pat2, 0, 0, 0).unwrap();
    let pat3 = sysex_to_pattern(&sysex2).unwrap();
    assert_patterns_equal(&pat, &pat3);
}

#[test]
fn sysex_to_json_to_sysex() {
    let payload = fixtures::simple_sysex();
    let pat = sysex_to_pattern(&payload).unwrap();
    let json_str = json::export(&pat).unwrap();
    let pat2 = json::import(&json_str).unwrap();
    let sysex2 = pattern_to_sysex(&pat2, 0, 0, 0).unwrap();
    let pat3 = sysex_to_pattern(&sysex2).unwrap();
    assert_patterns_equal(&pat, &pat3);
}

#[test]
fn sysex_to_steps_to_sysex() {
    let payload = fixtures::simple_sysex();
    let pat = sysex_to_pattern(&payload).unwrap();
    let steps_str = steps_txt::export(&pat);
    let pat2 = steps_txt::import(&steps_str).unwrap();
    let sysex2 = pattern_to_sysex(&pat2, 0, 0, 0).unwrap();
    let pat3 = sysex_to_pattern(&sysex2).unwrap();
    assert_patterns_equal(&pat, &pat3);
}

#[test]
fn toml_to_json_cross_conversion() {
    let pat = test_pattern();
    let toml_str = toml_fmt::export(&pat).unwrap();
    let pat_from_toml = toml_fmt::import(&toml_str).unwrap();
    let json_str = json::export(&pat_from_toml).unwrap();
    let pat_from_json = json::import(&json_str).unwrap();
    assert_patterns_equal(&pat, &pat_from_json);
}

#[test]
fn steps_to_toml_cross_conversion() {
    let pat = test_pattern();
    let steps_str = steps_txt::export(&pat);
    let pat_from_steps = steps_txt::import(&steps_str).unwrap();
    let toml_str = toml_fmt::export(&pat_from_steps).unwrap();
    let pat_from_toml = toml_fmt::import(&toml_str).unwrap();
    assert_patterns_equal(&pat, &pat_from_toml);
}

// ===========================================================================
// Format detection
// ===========================================================================

#[test]
fn detect_format_from_extension() {
    assert_eq!(formats::detect_format("foo.syx"), Some(Format::Syx));
    assert_eq!(formats::detect_format("foo.toml"), Some(Format::Toml));
    assert_eq!(
        formats::detect_format("foo.steps.txt"),
        Some(Format::StepsTxt)
    );
    assert_eq!(formats::detect_format("foo.json"), Some(Format::Json));
    assert_eq!(formats::detect_format("foo.txt"), None);
    assert_eq!(formats::detect_format("foo.xyz"), None);
}

#[test]
fn detect_steps_txt_before_txt() {
    assert_eq!(
        formats::detect_format("G1-P1A.steps.txt"),
        Some(Format::StepsTxt)
    );
}

#[test]
fn format_address_formatting() {
    assert_eq!(formats::format_address(0, 0, 0), "G1-P1A");
    assert_eq!(formats::format_address(0, 3, 0), "G1-P4A");
    assert_eq!(formats::format_address(3, 7, 1), "G4-P8B");
}

#[test]
fn format_extension_mapping() {
    assert_eq!(Format::Syx.extension(), "syx");
    assert_eq!(Format::Toml.extension(), "toml");
    assert_eq!(Format::StepsTxt.extension(), "steps.txt");
    assert_eq!(Format::Json.extension(), "json");
}

// ===========================================================================
// Helper
// ===========================================================================

fn assert_patterns_equal(a: &Pattern, b: &Pattern) {
    assert_eq!(a.active_steps, b.active_steps, "active_steps mismatch");
    assert_eq!(a.triplet, b.triplet, "triplet mismatch");
    for i in 0..16 {
        assert_eq!(a.step[i].note, b.step[i].note, "step {} note", i);
        assert_eq!(
            a.step[i].transpose, b.step[i].transpose,
            "step {} transpose",
            i
        );
        assert_eq!(a.step[i].accent, b.step[i].accent, "step {} accent", i);
        assert_eq!(a.step[i].slide, b.step[i].slide, "step {} slide", i);
        assert_eq!(a.step[i].time, b.step[i].time, "step {} time", i);
    }
}

// ===========================================================================
// Format version validation
// ===========================================================================

#[test]
fn toml_rejects_wrong_version() {
    let pat = Pattern::default();
    let mut toml_str = toml_fmt::export(&pat).unwrap();
    toml_str = toml_str.replace("format_version = 1", "format_version = 99");
    let err = toml_fmt::import(&toml_str);
    assert!(err.is_err());
    let msg = format!("{}", err.unwrap_err());
    assert!(msg.contains("unsupported format_version"), "got: {}", msg);
}

#[test]
fn json_rejects_wrong_version() {
    let pat = Pattern::default();
    let mut json_str = json::export(&pat).unwrap();
    json_str = json_str.replace("\"format_version\": 1", "\"format_version\": 99");
    let err = json::import(&json_str);
    assert!(err.is_err());
    let msg = format!("{}", err.unwrap_err());
    assert!(msg.contains("unsupported format_version"), "got: {}", msg);
}

#[test]
fn steps_rejects_wrong_format_header() {
    let pat = Pattern::default();
    let mut steps_str = steps_txt::export(&pat);
    steps_str = steps_str.replace("format=td3-stepdsl-v1", "format=td3-stepdsl-v99");
    let err = steps_txt::import(&steps_str);
    assert!(err.is_err());
    let msg = format!("{}", err.unwrap_err());
    assert!(msg.contains("unknown format"), "got: {}", msg);
}

// ===========================================================================
// Unknown fields rejected (deny_unknown_fields)
// ===========================================================================

#[test]
fn toml_rejects_unknown_field() {
    let toml_str = r#"
format = "td3-control"
format_version = 1
device = "TD-3"
active_steps = 1
triplet_time = false
bogus_field = true

[[steps]]
index = 1
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 2
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 3
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 4
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 5
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 6
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 7
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 8
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 9
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 10
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 11
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 12
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 13
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 14
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 15
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"

[[steps]]
index = 16
note = "C"
transpose = "NORMAL"
accent = false
slide = false
time = "NORMAL"
"#;
    let err = toml_fmt::import(toml_str);
    assert!(err.is_err(), "should reject unknown field 'bogus_field'");
}

#[test]
fn json_rejects_unknown_step_field() {
    let pat = Pattern::default();
    let mut json_str = json::export(&pat).unwrap();
    // Inject an unknown field into the first step
    json_str = json_str.replacen(
        "\"time\": \"NORMAL\"",
        "\"time\": \"NORMAL\",\n      \"velocity\": 100",
        1,
    );
    let err = json::import(&json_str);
    assert!(
        err.is_err(),
        "should reject unknown field 'velocity' in step"
    );
}

// ===========================================================================
// Cross-format round-trips through SysEx
// ===========================================================================

#[test]
fn toml_to_sysex_to_steps_roundtrip() {
    let pat = sysex_safe_pattern();
    let toml_str = toml_fmt::export(&pat).unwrap();
    let pat_from_toml = toml_fmt::import(&toml_str).unwrap();
    let sysex = pattern_to_sysex(&pat_from_toml, 0, 0, 0).unwrap();
    let pat_from_sysex = sysex_to_pattern(&sysex).unwrap();
    let steps_str = steps_txt::export(&pat_from_sysex);
    let pat_from_steps = steps_txt::import(&steps_str).unwrap();
    assert_patterns_equal(&pat, &pat_from_steps);
}

#[test]
fn steps_to_json_cross_conversion() {
    let pat = test_pattern();
    let steps_str = steps_txt::export(&pat);
    let pat_from_steps = steps_txt::import(&steps_str).unwrap();
    let json_str = json::export(&pat_from_steps).unwrap();
    let pat_from_json = json::import(&json_str).unwrap();
    assert_patterns_equal(&pat, &pat_from_json);
}
