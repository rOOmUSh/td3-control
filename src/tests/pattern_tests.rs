#![allow(clippy::field_reassign_with_default)]

use crate::error::Td3Error;
use crate::formats::steps_txt;
use crate::pattern::*;
use crate::step;

use super::fixtures;

// ---------------------------------------------------------------------------
// Steps DSL import - positive
// ---------------------------------------------------------------------------

#[test]
fn string_to_pattern_simple_parses() {
    let text = include_str!("../../tests/fixtures/simple_pattern.steps.txt");
    let result = steps_txt::import(text);
    assert!(
        result.is_ok(),
        "Failed to parse simple_pattern.steps.txt: {:?}",
        result.err()
    );
    let pat = result.unwrap();
    assert_eq!(pat.active_steps, 16);
    assert!(!pat.triplet);
    for i in 0..16 {
        assert_eq!(pat.step[i].note, 0, "step {} note", i);
        assert_eq!(
            pat.step[i].transpose,
            step::Transpose::Normal,
            "step {} transpose",
            i
        );
        assert_eq!(pat.step[i].accent, step::Accent::Off, "step {} accent", i);
        assert_eq!(pat.step[i].slide, step::Slide::Off, "step {} slide", i);
        assert_eq!(pat.step[i].time, step::Time::Normal, "step {} time", i);
    }
}

#[test]
fn string_to_pattern_readme_example_parses() {
    let text = include_str!("../../tests/fixtures/readme_example.steps.txt");
    let result = steps_txt::import(text);
    assert!(
        result.is_ok(),
        "Failed to parse readme_example.steps.txt: {:?}",
        result.err()
    );
    let pat = result.unwrap();
    assert_eq!(pat.active_steps, 16);
    assert!(!pat.triplet);
    assert_eq!(pat.step[0].note, 3); // D#
    assert_eq!(pat.step[1].accent, step::Accent::On);
    assert_eq!(pat.step[4].slide, step::Slide::On);
    assert_eq!(pat.step[1].time, step::Time::Tie);
    assert_eq!(pat.step[13].time, step::Time::Rest);
}

#[test]
fn string_to_pattern_all_features_parses() {
    let text = include_str!("../../tests/fixtures/all_features.steps.txt");
    let result = steps_txt::import(text);
    assert!(
        result.is_ok(),
        "Failed to parse all_features.steps.txt: {:?}",
        result.err()
    );
    let pat = result.unwrap();
    assert_eq!(pat.active_steps, 16);
    assert!(pat.triplet);
    assert_eq!(pat.step[0].note, 0);
    assert_eq!(pat.step[0].transpose, step::Transpose::Down);
    assert_eq!(pat.step[0].accent, step::Accent::On);
    assert_eq!(pat.step[12].note, 12); // C^
}

#[test]
fn string_to_pattern_minimal_steps_parses() {
    let text = include_str!("../../tests/fixtures/minimal_steps.steps.txt");
    let result = steps_txt::import(text);
    assert!(
        result.is_ok(),
        "Failed to parse minimal_steps.steps.txt: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().active_steps, 1);
}

#[test]
fn string_to_pattern_triplet_on_parses() {
    let text = include_str!("../../tests/fixtures/triplet_on.steps.txt");
    let result = steps_txt::import(text);
    assert!(
        result.is_ok(),
        "Failed to parse triplet_on.steps.txt: {:?}",
        result.err()
    );
    let pat = result.unwrap();
    assert_eq!(pat.active_steps, 8);
    assert!(pat.triplet);
}

#[test]
fn string_to_pattern_rejects_invalid_triplet_value() {
    let mut text = String::from("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=maybe\n\n");
    for i in 1..=16 {
        text.push_str(&format!("{:02}  C:---:N\n", i));
    }
    let err = steps_txt::import(&text).unwrap_err().to_string();
    assert!(err.contains("invalid triplet_time"));
}

// ---------------------------------------------------------------------------
// Steps DSL import - negative
// ---------------------------------------------------------------------------

#[test]
fn string_to_pattern_rejects_empty_input() {
    assert!(steps_txt::import("").is_err());
}

#[test]
fn string_to_pattern_rejects_wrong_header() {
    assert!(steps_txt::import("format=wrong-format\nactive_steps=16\ntriplet_time=off\n").is_err());
}

#[test]
fn string_to_pattern_rejects_missing_rows() {
    let text = "format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n\
        01  C:---:N\n02  C:---:N\n";
    assert!(steps_txt::import(text).is_err());
}

#[test]
fn string_to_pattern_rejects_wrong_column_count() {
    // TAS field with wrong length
    let mut text = String::from("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n");
    text.push_str("01  C:--:N\n"); // TAS only 2 chars instead of 3
    for i in 2..=16 {
        text.push_str(&format!("{:02}  C:---:N\n", i));
    }
    assert!(steps_txt::import(&text).is_err());
}

#[test]
fn string_to_pattern_rejects_active_steps_zero() {
    let mut text = String::from("format=td3-stepdsl-v1\nactive_steps=0\ntriplet_time=off\n\n");
    for i in 1..=16 {
        text.push_str(&format!("{:02}  C:---:N\n", i));
    }
    assert!(steps_txt::import(&text).is_err());
}

#[test]
fn string_to_pattern_rejects_active_steps_17() {
    let mut text = String::from("format=td3-stepdsl-v1\nactive_steps=17\ntriplet_time=off\n\n");
    for i in 1..=16 {
        text.push_str(&format!("{:02}  C:---:N\n", i));
    }
    assert!(steps_txt::import(&text).is_err());
}

// ---------------------------------------------------------------------------
// SysEx decode - positive
// ---------------------------------------------------------------------------

#[test]
fn sysex_to_pattern_simple_decodes() {
    let msg = fixtures::simple_sysex();
    let pat = sysex_to_pattern(&msg).expect("decode failed");
    assert_eq!(pat.active_steps, 16);
    assert!(!pat.triplet);
    for i in 0..16 {
        assert_eq!(pat.step[i].note, 0, "step {} note should be C (0)", i);
        assert_eq!(
            pat.step[i].transpose,
            step::Transpose::Normal,
            "step {} transpose",
            i
        );
        assert_eq!(pat.step[i].accent, step::Accent::Off, "step {} accent", i);
        assert_eq!(pat.step[i].slide, step::Slide::Off, "step {} slide", i);
        assert_eq!(pat.step[i].time, step::Time::Normal, "step {} time", i);
    }
}

// ---------------------------------------------------------------------------
// SysEx decode - negative
// ---------------------------------------------------------------------------

#[test]
fn sysex_to_pattern_rejects_empty_payload() {
    assert!(sysex_to_pattern(&[]).is_err());
}

#[test]
fn sysex_to_pattern_rejects_truncated_payload() {
    assert!(sysex_to_pattern(&[0x78; 50]).is_err());
}

#[test]
fn sysex_to_pattern_rejects_one_extra_payload_byte() {
    let mut msg = fixtures::simple_sysex();
    msg.push(0x00);
    let err = sysex_to_pattern(&msg).unwrap_err();
    match err {
        Td3Error::InvalidPayloadLength { expected, actual } => {
            assert_eq!(expected, 115);
            assert_eq!(actual, 116);
        }
        other => panic!("expected InvalidPayloadLength, got: {}", other),
    }
}

#[test]
fn sysex_to_pattern_rejects_wrong_message_id() {
    let mut msg = fixtures::simple_sysex();
    msg[0] = 0x99;
    assert!(sysex_to_pattern(&msg).is_err());
}

#[test]
fn sysex_to_pattern_rejects_active_steps_zero() {
    let mut msg = fixtures::simple_sysex();
    msg[0x67] = 0x00;
    msg[0x68] = 0x00;
    assert!(sysex_to_pattern(&msg).is_err());
}

#[test]
fn sysex_to_pattern_rejects_active_steps_over_16() {
    let mut msg = fixtures::simple_sysex();
    msg[0x67] = 0x01;
    msg[0x68] = 0x01;
    assert!(sysex_to_pattern(&msg).is_err());
}

#[test]
fn sysex_to_pattern_rejects_invalid_active_step_nibble() {
    let mut msg = fixtures::simple_sysex();
    msg[0x67] = 0x00;
    msg[0x68] = 0x10;
    match sysex_to_pattern(&msg).unwrap_err() {
        Td3Error::InvalidNibble { field, value } => {
            assert_eq!(field, "active steps low");
            assert_eq!(value, 0x10);
        }
        other => panic!("expected InvalidNibble, got: {}", other),
    }
}

#[test]
fn sysex_to_pattern_rejects_invalid_triplet_flag() {
    let mut msg = fixtures::simple_sysex();
    msg[0x66] = 0x02;
    match sysex_to_pattern(&msg).unwrap_err() {
        Td3Error::InvalidFlag { field, value } => {
            assert_eq!(field, "triplet");
            assert_eq!(value, 0x02);
        }
        other => panic!("expected InvalidFlag, got: {}", other),
    }
}

#[test]
fn sysex_to_pattern_rejects_invalid_pitch_nibble() {
    let mut msg = fixtures::simple_sysex();
    msg[0x05] = 0x10;
    match sysex_to_pattern(&msg).unwrap_err() {
        Td3Error::InvalidNibble { field, value } => {
            assert_eq!(field, "pitch high");
            assert_eq!(value, 0x10);
        }
        other => panic!("expected InvalidNibble, got: {}", other),
    }
}

#[test]
fn sysex_to_pattern_rejects_invalid_mask_nibble() {
    let mut msg = fixtures::simple_sysex();
    msg[0x6B] = 0x10;
    match sysex_to_pattern(&msg).unwrap_err() {
        Td3Error::InvalidNibble { field, value } => {
            assert_eq!(field, "tie mask");
            assert_eq!(value, 0x10);
        }
        other => panic!("expected InvalidNibble, got: {}", other),
    }
}

#[test]
fn sysex_to_pattern_rejects_invalid_accent_flag() {
    let mut msg = fixtures::simple_sysex();
    msg[0x26] = 0x02;
    match sysex_to_pattern(&msg).unwrap_err() {
        Td3Error::InvalidFlag { field, value } => {
            assert_eq!(field, "accent");
            assert_eq!(value, 0x02);
        }
        other => panic!("expected InvalidFlag, got: {}", other),
    }
}

// ---------------------------------------------------------------------------
// SysEx encode
// ---------------------------------------------------------------------------

#[test]
fn pattern_to_sysex_output_length() {
    let sysex = pattern_to_sysex(&Pattern::default(), 0, 0, 0).unwrap();
    assert_eq!(sysex.len(), 115);
}

#[test]
fn pattern_to_sysex_header_bytes() {
    let sysex = pattern_to_sysex(&Pattern::default(), 2, 3, 1).unwrap();
    assert_eq!(sysex[0], 0x78);
    assert_eq!(sysex[1], 2);
    assert_eq!(sysex[2], 3 + (1 << 3));
}

// ---------------------------------------------------------------------------
// SysEx round-trip
// ---------------------------------------------------------------------------

#[test]
fn sysex_roundtrip_default_pattern() {
    let original = Pattern::default();
    let sysex = pattern_to_sysex(&original, 0, 0, 0).unwrap();
    let decoded = sysex_to_pattern(&sysex).expect("round-trip decode failed");
    assert_eq!(decoded.active_steps, original.active_steps);
    assert_eq!(decoded.triplet, original.triplet);
    for i in 0..16 {
        assert_eq!(
            decoded.step[i].note, original.step[i].note,
            "step {} note",
            i
        );
        assert_eq!(
            decoded.step[i].transpose, original.step[i].transpose,
            "step {} transpose",
            i
        );
        assert_eq!(
            decoded.step[i].accent, original.step[i].accent,
            "step {} accent",
            i
        );
        assert_eq!(
            decoded.step[i].slide, original.step[i].slide,
            "step {} slide",
            i
        );
        assert_eq!(
            decoded.step[i].time, original.step[i].time,
            "step {} time",
            i
        );
    }
}

// ---------------------------------------------------------------------------
// Steps DSL round-trip
// ---------------------------------------------------------------------------

#[test]
fn text_roundtrip_simple_pattern() {
    let text = include_str!("../../tests/fixtures/simple_pattern.steps.txt");
    let pat = steps_txt::import(text).unwrap();
    let output = steps_txt::export(&pat);
    let reparsed = steps_txt::import(&output).expect("text round-trip re-parse failed");
    assert_eq!(reparsed.active_steps, pat.active_steps);
    for i in 0..16 {
        assert_eq!(reparsed.step[i].note, pat.step[i].note, "step {} note", i);
        assert_eq!(
            reparsed.step[i].transpose, pat.step[i].transpose,
            "step {} transpose",
            i
        );
        assert_eq!(
            reparsed.step[i].accent, pat.step[i].accent,
            "step {} accent",
            i
        );
        assert_eq!(
            reparsed.step[i].slide, pat.step[i].slide,
            "step {} slide",
            i
        );
        assert_eq!(reparsed.step[i].time, pat.step[i].time, "step {} time", i);
    }
}

#[test]
fn text_roundtrip_all_features() {
    let text = include_str!("../../tests/fixtures/all_features.steps.txt");
    let pat = steps_txt::import(text).unwrap();
    let output = steps_txt::export(&pat);
    let reparsed = steps_txt::import(&output).expect("text round-trip failed for all_features");
    assert_eq!(reparsed.active_steps, pat.active_steps);
    assert_eq!(reparsed.triplet, pat.triplet);
    for i in 0..16 {
        assert_eq!(reparsed.step[i].note, pat.step[i].note, "step {} note", i);
        assert_eq!(
            reparsed.step[i].transpose, pat.step[i].transpose,
            "step {} transpose",
            i
        );
    }
}

#[test]
fn text_roundtrip_readme_example() {
    let text = include_str!("../../tests/fixtures/readme_example.steps.txt");
    let pat = steps_txt::import(text).unwrap();
    let output = steps_txt::export(&pat);
    let reparsed = steps_txt::import(&output).expect("text round-trip failed for readme_example");
    assert_eq!(reparsed.active_steps, pat.active_steps);
    for i in 0..16 {
        assert_eq!(reparsed.step[i].note, pat.step[i].note, "step {} note", i);
        assert_eq!(reparsed.step[i].time, pat.step[i].time, "step {} time", i);
    }
}

// ---------------------------------------------------------------------------
// Full round-trip: steps -> Pattern -> sysex -> Pattern -> steps
// ---------------------------------------------------------------------------

#[test]
fn full_roundtrip_simple() {
    let text = include_str!("../../tests/fixtures/simple_pattern.steps.txt");
    let pat1 = steps_txt::import(text).unwrap();
    let sysex = pattern_to_sysex(&pat1, 0, 0, 0).unwrap();
    let pat2 = sysex_to_pattern(&sysex).expect("sysex decode failed");
    let text2 = steps_txt::export(&pat2);
    let pat3 = steps_txt::import(&text2).expect("re-parse after sysex round-trip failed");
    assert_eq!(pat3.active_steps, pat1.active_steps);
    for i in 0..16 {
        assert_eq!(pat3.step[i].note, pat1.step[i].note, "step {} note", i);
        assert_eq!(
            pat3.step[i].transpose, pat1.step[i].transpose,
            "step {} transpose",
            i
        );
    }
}

// ---------------------------------------------------------------------------
// Pattern::new and validate
// ---------------------------------------------------------------------------

#[test]
fn pattern_new_valid() {
    let steps: [step::Step; 16] = Default::default();
    let p = Pattern::new(false, 16, steps);
    assert!(p.is_ok());
}

#[test]
fn pattern_new_rejects_active_steps_zero() {
    let steps: [step::Step; 16] = Default::default();
    let p = Pattern::new(false, 0, steps);
    assert!(p.is_err());
}

#[test]
fn pattern_new_rejects_active_steps_over_16() {
    let steps: [step::Step; 16] = Default::default();
    let p = Pattern::new(false, 17, steps);
    assert!(p.is_err());
}

#[test]
fn pattern_new_rejects_invalid_note() {
    let mut steps: [step::Step; 16] = Default::default();
    steps[5].note = 13; // max valid is 12 (C^)
    let p = Pattern::new(false, 16, steps);
    assert!(p.is_err());
    let err = format!("{}", p.unwrap_err());
    assert!(
        err.contains("step 6"),
        "error should reference 1-indexed step 6, got: {}",
        err
    );
}

#[test]
fn pattern_validate_accepts_note_12() {
    let mut steps: [step::Step; 16] = Default::default();
    steps[0].note = 12; // C^ - upper C, valid
    let p = Pattern::new(false, 16, steps);
    assert!(p.is_ok());
}

#[test]
fn pattern_to_sysex_rejects_invalid_pattern() {
    let mut p = Pattern::default();
    p.active_steps = 0; // invalid
    let result = pattern_to_sysex(&p, 0, 0, 0);
    assert!(result.is_err());
}

#[test]
fn pattern_to_sysex_rejects_invalid_destination_address() {
    let p = Pattern::default();
    for (group, slot, side) in [(4, 0, 0), (0, 8, 0), (0, 0, 2)] {
        match pattern_to_sysex(&p, group, slot, side).unwrap_err() {
            Td3Error::InvalidPatternAddress {
                patgroup,
                slot: got_slot,
                side: got_side,
            } => {
                assert_eq!(patgroup, group);
                assert_eq!(got_slot, slot);
                assert_eq!(got_side, side);
            }
            other => panic!("expected InvalidPatternAddress, got: {}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// TieRest policy: all 4 Time states are valid
// ---------------------------------------------------------------------------

#[test]
fn all_time_states_round_trip_through_sysex() {
    let mut steps: [step::Step; 16] = Default::default();
    steps[0].time = step::Time::Normal;
    steps[1].time = step::Time::Tie;
    steps[2].time = step::Time::Rest;
    steps[3].time = step::Time::TieRest;

    let pattern = Pattern::new(false, 16, steps).unwrap();
    let sysex = pattern_to_sysex(&pattern, 0, 0, 0).unwrap();
    let decoded = sysex_to_pattern(&sysex).unwrap();

    assert_eq!(decoded.step[0].time, step::Time::Normal);
    assert_eq!(decoded.step[0].slide, step::Slide::Off);
    assert_eq!(decoded.step[1].time, step::Time::Tie);
    assert_eq!(decoded.step[2].time, step::Time::Rest);
    assert_eq!(decoded.step[3].time, step::Time::TieRest);
}

#[test]
fn tierest_accepted_by_pattern_new() {
    let mut steps: [step::Step; 16] = Default::default();
    steps[0].time = step::Time::TieRest;
    let p = Pattern::new(false, 16, steps);
    assert!(p.is_ok(), "TieRest is a valid hardware state");
}
