// Tests for the SynthTribe .seq single-pattern file format.

use crate::formats::{self, seq};
use crate::pattern::sysex_to_pattern;
use crate::step;

use super::fixtures::{simple_sysex, REAL_G1_P4A_PAYLOAD};

#[test]
fn detect_format_recognises_seq_extension() {
    assert_eq!(
        formats::detect_format("pattern.seq"),
        Some(formats::Format::Seq)
    );
    assert_eq!(
        formats::detect_format("PATTERN.SEQ"),
        Some(formats::Format::Seq)
    );
}

#[test]
fn roundtrip_simple_pattern() {
    let original = sysex_to_pattern(&simple_sysex()).unwrap();
    let encoded = seq::export(&original).unwrap();
    assert_eq!(encoded.len(), 146, ".seq file must be 146 bytes");
    let decoded = seq::import(&encoded).unwrap();

    assert_eq!(decoded.active_steps, original.active_steps);
    assert_eq!(decoded.triplet, original.triplet);
    for i in 0..16 {
        assert_eq!(decoded.step[i], original.step[i], "step {} mismatch", i);
    }
}

#[test]
fn roundtrip_real_g1_p4a_payload() {
    let original = sysex_to_pattern(REAL_G1_P4A_PAYLOAD).unwrap();
    let encoded = seq::export(&original).unwrap();
    let decoded = seq::import(&encoded).unwrap();

    assert_eq!(decoded.active_steps, original.active_steps);
    assert_eq!(decoded.triplet, original.triplet);
    for i in 0..16 {
        assert_eq!(decoded.step[i], original.step[i], "step {} mismatch", i);
    }
}

#[test]
fn export_has_magic_and_payload_size() {
    let pattern = sysex_to_pattern(&simple_sysex()).unwrap();
    let bytes = seq::export(&pattern).unwrap();

    // Magic
    assert_eq!(&bytes[0..4], &[0x23, 0x98, 0x54, 0x76]);
    // Payload size at offset 30 = 0x00000070 = 112
    assert_eq!(&bytes[30..34], &[0x00, 0x00, 0x00, 0x70]);
    // Payload begins at 34, ends at 146
    assert_eq!(bytes.len(), 146);
}

#[test]
fn import_rejects_wrong_magic() {
    let mut bad = vec![0u8; 146];
    bad[0] = 0x00; // not 0x23
    let err = seq::import(&bad).unwrap_err();
    assert!(
        err.to_string().contains("magic"),
        "expected magic error, got: {}",
        err
    );
}

#[test]
fn import_rejects_truncated_file() {
    let short = vec![0x23, 0x98, 0x54, 0x76];
    let err = seq::import(&short).unwrap_err();
    assert!(
        err.to_string().contains("truncated") || err.to_string().contains("short"),
        "expected truncation error, got: {}",
        err
    );
}

#[test]
fn import_rejects_product_length_without_version_field() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x23, 0x98, 0x54, 0x76]);
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.extend_from_slice(&[0x00, 0x54, 0x00, 0x44, 0x00, 0x2D, 0x00, 0x33]);
    let err = seq::import(&bytes).unwrap_err();
    assert!(
        err.to_string().contains("version length"),
        "expected version length error, got: {}",
        err
    );
}

#[test]
fn import_rejects_wrong_payload_size() {
    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend_from_slice(&[0x23, 0x98, 0x54, 0x76]);
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.extend_from_slice(&[0x00, 0x54, 0x00, 0x44, 0x00, 0x2D, 0x00, 0x33]);
    bytes.extend_from_slice(&10u32.to_be_bytes());
    bytes.extend_from_slice(&[0x00, 0x31, 0x00, 0x2E, 0x00, 0x33, 0x00, 0x2E, 0x00, 0x37]);
    bytes.extend_from_slice(&50u32.to_be_bytes()); // wrong size
    bytes.resize(bytes.len() + 50, 0);
    let err = seq::import(&bytes).unwrap_err();
    assert!(
        err.to_string().contains("payload length"),
        "expected payload length error, got: {}",
        err
    );
}

#[test]
fn golden_jam_pattern_decode() {
    // Real reference file captured from SynthTribe (firmware 1.3.7).
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/JAM PATTERN.seq"
    );
    let bytes = std::fs::read(path).expect("tests/fixtures/JAM PATTERN.seq must exist");
    assert_eq!(bytes.len(), 146);

    let pattern = seq::import(&bytes).unwrap();
    assert_eq!(pattern.active_steps, 16);
    assert!(!pattern.triplet);

    // Gate states derived from tie_word=0xFFFB, rest_word=0x0180.
    // Step 3 (index 2) is Tie; steps 8 and 9 (indices 7 and 8) are Rest.
    assert_eq!(pattern.step[0].time, step::Time::Normal);
    assert_eq!(pattern.step[1].time, step::Time::Normal);
    assert_eq!(pattern.step[2].time, step::Time::Tie);
    assert_eq!(pattern.step[3].time, step::Time::Normal);
    assert_eq!(pattern.step[7].time, step::Time::Rest);
    assert_eq!(pattern.step[8].time, step::Time::Rest);
    assert_eq!(pattern.step[12].time, step::Time::Normal);
    assert_eq!(pattern.step[15].time, step::Time::Normal);

    // First decoded Normal step: D# Down.
    assert_eq!(pattern.step[0].note, 3); // D#
    assert_eq!(pattern.step[0].transpose, step::Transpose::Down);

    // Step 5 (index 4) should be C^ Up with accent AND slide.
    assert_eq!(pattern.step[4].note, 12); // C^
    assert_eq!(pattern.step[4].transpose, step::Transpose::Up);
    assert_eq!(pattern.step[4].accent, step::Accent::On);
    assert_eq!(pattern.step[4].slide, step::Slide::On);
}

#[test]
fn golden_jam_pattern_reencodes_identically_after_marker_normalisation() {
    // The SynthTribe reference file stores marker bytes as `00 00`, matching
    // our encoder. Re-encoding the decoded pattern should yield the exact
    // same byte sequence as the original file.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/JAM PATTERN.seq"
    );
    let original = std::fs::read(path).expect("tests/fixtures/JAM PATTERN.seq must exist");
    let pattern = seq::import(&original).unwrap();
    let re_encoded = seq::export(&pattern).unwrap();
    assert_eq!(
        original, re_encoded,
        ".seq re-encode must match byte-for-byte"
    );
}
