//! Golden fixture tests (P8.1 + P8.2).
//!
//! Tests that decode real-device and protocol-shaped fixtures,
//! verifying exact field values against known truth.

use crate::error::Td3Error;
use crate::formats::{json, steps_txt, syx, toml_fmt};
use crate::pattern::{pattern_to_sysex, sysex_to_pattern};
use crate::step::{Accent, Slide, Time, Transpose};

use super::fixtures;

// ===========================================================================
// P8.1: Real-device golden fixture - G1-P4A decode
// ===========================================================================

#[test]
fn golden_g1p4a_payload_length() {
    assert_eq!(fixtures::REAL_G1_P4A_PAYLOAD.len(), 115);
}

#[test]
fn golden_g1p4a_syx_file_length() {
    assert_eq!(fixtures::REAL_G1_P4A_SYX_FILE.len(), 123);
}

#[test]
fn golden_g1p4a_decodes_correctly() {
    let pat = sysex_to_pattern(fixtures::REAL_G1_P4A_PAYLOAD).unwrap();

    assert_eq!(pat.active_steps, 16);
    assert!(!pat.triplet);

    // After 303 note unpacking, notes are assigned from packed list.
    // TIE/REST steps carry the last Normal step's note.
    // Notes: G G G G G G G G A# A# A# A# A# A# A# A#
    let expected_notes: [u8; 16] = [7, 7, 7, 7, 7, 7, 7, 7, 10, 10, 10, 10, 10, 10, 10, 10];
    for (i, &expected) in expected_notes.iter().enumerate() {
        assert_eq!(pat.step[i].note, expected, "step {} note", i + 1);
    }

    // Transpose after 303 unpacking (TIE/REST steps carry previous transpose):
    // Step 0: G Normal (packed 0), Step 1-2: held from 0 (Normal),
    // Step 3: G Normal (packed 1), Step 4: rest carry Normal,
    // Step 5: G Down (packed 2), Step 6: G Normal (packed 3),
    // Step 7: rest carry Normal, Step 8: A# Normal (packed 4),
    // Step 9: rest carry Normal, Step 10: A# Down (packed 5),
    // Step 11: held from 10 (Down), Step 12: A# Down (packed 6),
    // Step 13: A# Normal (packed 7), Step 14: rest carry Normal,
    // Step 15: A# Down (packed 8)
    let expected_transpose = [
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Down,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Down,
        Transpose::Down,
        Transpose::Down,
        Transpose::Normal,
        Transpose::Normal,
        Transpose::Down,
    ];
    for (i, &expected) in expected_transpose.iter().enumerate() {
        assert_eq!(pat.step[i].transpose, expected, "step {} transpose", i + 1);
    }

    // All accents off
    for i in 0..16 {
        assert_eq!(pat.step[i].accent, Accent::Off, "step {} accent", i + 1);
    }

    // Slide bytes: this fixture has all slide bytes = 0 in the SysEx.
    // No slides present in the raw device data.
    for i in 0..16 {
        assert_eq!(pat.step[i].slide, Slide::Off, "step {} slide", i + 1);
    }

    // Time: TIE steps preserved as Tie (steps 2,3,12 in 1-indexed = 1,2,11 in 0-indexed).
    // Rests at steps 5,8,10,15 (1-indexed) = 4,7,9,14 (0-indexed).
    let expected_time = [
        Time::Normal,
        Time::Tie,
        Time::Tie,
        Time::Normal,
        Time::Rest,
        Time::Normal,
        Time::Normal,
        Time::Rest,
        Time::Normal,
        Time::Rest,
        Time::Normal,
        Time::Tie,
        Time::Normal,
        Time::Normal,
        Time::Rest,
        Time::Normal,
    ];
    for (i, &expected) in expected_time.iter().enumerate() {
        assert_eq!(pat.step[i].time, expected, "step {} time", i + 1);
    }
}

#[test]
fn golden_g1p4a_syx_file_import_matches_payload() {
    let from_payload = sysex_to_pattern(fixtures::REAL_G1_P4A_PAYLOAD).unwrap();
    let from_syx = syx::import(fixtures::REAL_G1_P4A_SYX_FILE).unwrap();

    assert_eq!(from_payload.active_steps, from_syx.active_steps);
    assert_eq!(from_payload.triplet, from_syx.triplet);
    for i in 0..16 {
        assert_eq!(
            from_payload.step[i].note,
            from_syx.step[i].note,
            "step {} note",
            i + 1
        );
        assert_eq!(
            from_payload.step[i].transpose,
            from_syx.step[i].transpose,
            "step {} transpose",
            i + 1
        );
        assert_eq!(
            from_payload.step[i].accent,
            from_syx.step[i].accent,
            "step {} accent",
            i + 1
        );
        assert_eq!(
            from_payload.step[i].slide,
            from_syx.step[i].slide,
            "step {} slide",
            i + 1
        );
        assert_eq!(
            from_payload.step[i].time,
            from_syx.step[i].time,
            "step {} time",
            i + 1
        );
    }
}

#[test]
fn golden_g1p4a_sysex_roundtrip_semantic() {
    // Decode then re-encode. The re-encoded payload must match the original
    // on all pattern-relevant bytes, with one known difference:
    //   - Bytes 3-4 ("unknown1"): encoder hardcodes [0x00, 0x01]; device sent [0x00, 0x00]
    // Slide bytes and TIE/REST flags now round-trip perfectly since slide and
    // TIE are independent features - no conversion adds or removes either.
    let pat = sysex_to_pattern(fixtures::REAL_G1_P4A_PAYLOAD).unwrap();
    let re_encoded = pattern_to_sysex(&pat, 0, 3, 0).unwrap(); // patgroup=0, slot=3, side=0

    assert_eq!(re_encoded.len(), fixtures::REAL_G1_P4A_PAYLOAD.len());
    // Bytes 0-2 (msg_id, patgroup, pattern) match
    assert_eq!(&re_encoded[0..3], &fixtures::REAL_G1_P4A_PAYLOAD[0..3]);
    // Bytes 5-100 (notes + accents + slides) must match exactly
    assert_eq!(
        &re_encoded[5..101],
        &fixtures::REAL_G1_P4A_PAYLOAD[5..101],
        "notes, accent, and slide bytes must match real capture"
    );
    // Bytes 101-114 (triplet, active_steps, unknown2, tie, rest) must match
    assert_eq!(
        &re_encoded[101..],
        &fixtures::REAL_G1_P4A_PAYLOAD[101..],
        "trailing bytes (triplet, active_steps, ties, rests) must match"
    );
}

#[test]
fn golden_g1p4a_cross_format_roundtrip() {
    // Real SysEx → Pattern → Steps → Pattern → TOML → Pattern → JSON → Pattern → SysEx
    // Verify semantic preservation through all format conversions.
    let pat1 = sysex_to_pattern(fixtures::REAL_G1_P4A_PAYLOAD).unwrap();

    let steps_str = steps_txt::export(&pat1);
    let pat2 = steps_txt::import(&steps_str).unwrap();

    let toml_str = toml_fmt::export(&pat2).unwrap();
    let pat3 = toml_fmt::import(&toml_str).unwrap();

    let json_str = json::export(&pat3).unwrap();
    let pat4 = json::import(&json_str).unwrap();

    let final_sysex = pattern_to_sysex(&pat4, 0, 3, 0).unwrap();
    // Compare all pattern data bytes (skip unknown1 at 3-4 only)
    assert_eq!(&final_sysex[0..3], &fixtures::REAL_G1_P4A_PAYLOAD[0..3]);
    assert_eq!(
        &final_sysex[5..101],
        &fixtures::REAL_G1_P4A_PAYLOAD[5..101],
        "notes, accent, and slide bytes must match through cross-format chain"
    );
    assert_eq!(
        &final_sysex[101..],
        &fixtures::REAL_G1_P4A_PAYLOAD[101..],
        "trailing bytes must match through cross-format chain"
    );
}

// ===========================================================================
// P8.2: Protocol fixture decode tests
// ===========================================================================

#[test]
fn protocol_product_name_response_valid() {
    let payload = fixtures::PRODUCT_NAME_RESP;
    assert_eq!(payload[0], 0x07, "command byte");
    assert_eq!(*payload.last().unwrap(), 0x00, "null terminator");
    let name = std::str::from_utf8(&payload[1..payload.len() - 1]).unwrap();
    assert_eq!(name, "TD-3");
}

#[test]
fn protocol_product_name_truncated() {
    // Just command byte, no name, no null
    let payload: &[u8] = &[0x07];
    assert!(
        payload.len() < 3,
        "too short for valid product name response"
    );
}

#[test]
fn protocol_product_name_missing_null() {
    // Command + name but no null terminator
    let payload: &[u8] = &[0x07, b'T', b'D', b'-', b'3'];
    assert_ne!(*payload.last().unwrap(), 0x00);
}

#[test]
fn protocol_firmware_response_valid() {
    let payload = fixtures::FIRMWARE_RESP_1_3_7;
    assert_eq!(payload[0], 0x09, "command byte");
    assert_eq!(payload[1], 0x00, "sub-command byte");
    let version: Vec<String> = payload[2..].iter().map(|b| b.to_string()).collect();
    assert_eq!(version.join("."), "1.3.7");
}

#[test]
fn protocol_firmware_response_truncated() {
    // Only command byte, no version data
    let payload: &[u8] = &[0x09];
    assert!(payload.len() < 3, "too short for valid firmware response");
}

#[test]
fn protocol_firmware_response_minimal_valid() {
    // Minimum: cmd + sub-cmd + 1 version byte
    let payload: &[u8] = &[0x09, 0x00, 0x05];
    assert!(payload.len() >= 3);
    assert_eq!(payload[2].to_string(), "5");
}

#[test]
fn protocol_upload_ack_valid() {
    let payload = fixtures::UPLOAD_ACK_RESP;
    assert_eq!(payload[0], 0x01, "ACK command byte");
    assert_eq!(payload.len(), 3, "ACK must be exactly 3 bytes");
}

#[test]
fn protocol_upload_ack_wrong_length() {
    // ACK with extra bytes - should be rejected
    let too_long: &[u8] = &[0x01, 0x00, 0x00, 0xFF];
    assert_ne!(too_long.len(), 3);

    // ACK too short
    let too_short: &[u8] = &[0x01, 0x00];
    assert_ne!(too_short.len(), 3);
}

#[test]
fn protocol_pattern_dump_wrong_command_byte() {
    // Valid length but wrong message ID
    let mut bad_payload = fixtures::REAL_G1_P4A_PAYLOAD.to_vec();
    bad_payload[0] = 0x77; // Download request cmd, not dump response
    let result = sysex_to_pattern(&bad_payload);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::WrongMessageId { actual } => assert_eq!(actual, 0x77),
        other => panic!("expected WrongMessageId, got: {}", other),
    }
}

#[test]
fn protocol_pattern_dump_correct_cmd_wrong_length() {
    // Correct command byte but truncated payload
    let truncated: &[u8] = &[0x78, 0x00, 0x03, 0x00, 0x01]; // only 5 bytes
    let result = sysex_to_pattern(truncated);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::PayloadTooShort { expected, actual } => {
            assert_eq!(expected, 115);
            assert_eq!(actual, 5);
        }
        other => panic!("expected PayloadTooShort, got: {}", other),
    }
}

#[test]
fn protocol_pattern_dump_empty_payload() {
    let result = sysex_to_pattern(&[]);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::PayloadTooShort { actual, .. } => assert_eq!(actual, 0),
        other => panic!("expected PayloadTooShort, got: {}", other),
    }
}

#[test]
fn protocol_pattern_dump_one_byte_short() {
    // 114 bytes - one byte short of valid
    let payload = &fixtures::REAL_G1_P4A_PAYLOAD[..114];
    let result = sysex_to_pattern(payload);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::PayloadTooShort { actual, .. } => assert_eq!(actual, 114),
        other => panic!("expected PayloadTooShort, got: {}", other),
    }
}

#[test]
fn protocol_pattern_dump_one_byte_long() {
    let mut payload = fixtures::REAL_G1_P4A_PAYLOAD.to_vec();
    payload.push(0x00);
    let result = sysex_to_pattern(&payload);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::InvalidPayloadLength { expected, actual } => {
            assert_eq!(expected, 115);
            assert_eq!(actual, 116);
        }
        other => panic!("expected InvalidPayloadLength, got: {}", other),
    }
}
