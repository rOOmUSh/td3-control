// Tests for the `.sqs` full-bank file format.

use crate::formats::sqs;

fn golden_path(name: &str) -> String {
    format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn read_golden(name: &str) -> Vec<u8> {
    let path = golden_path(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {} missing: {}", path, e))
}

// ---------------------------------------------------------------------------
// Header parse / round-trip
// ---------------------------------------------------------------------------

#[test]
fn golden_bank_round_trip_byte_exact() {
    let original = read_golden("ALL TD-3 PATTERNS.sqs");
    assert_eq!(original.len(), sqs::FILE_LEN);

    let bank = sqs::parse_bank(&original).expect("golden .sqs must parse");
    let re_encoded = sqs::serialize_bank(&bank).expect("re-encode must succeed");

    assert_eq!(
        original, re_encoded,
        ".sqs round-trip must be byte-for-byte identical"
    );
}

#[test]
fn empty_bank_round_trip_byte_exact() {
    let original = read_golden("20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs");
    let bank = sqs::parse_bank(&original).unwrap();
    let re_encoded = sqs::serialize_bank(&bank).unwrap();
    assert_eq!(original, re_encoded);
}

#[test]
fn bank_has_64_records_in_file_order() {
    let data = read_golden("ALL TD-3 PATTERNS.sqs");
    let bank = sqs::parse_bank(&data).unwrap();

    assert_eq!(bank.records.len(), 64);
    for (idx, rec) in bank.records.iter().enumerate() {
        assert_eq!(rec.group as usize, idx / 16, "record {} group", idx);
        assert_eq!(rec.slot_addr as usize, idx % 16, "record {} slot_addr", idx);
        assert_eq!(rec.payload.len(), 112);
    }
}

#[test]
fn header_fields_constants() {
    let data = read_golden("ALL TD-3 PATTERNS.sqs");
    let bank = sqs::parse_bank(&data).unwrap();
    assert_eq!(bank.product_bytes, sqs::PRODUCT_UTF16BE);
    assert_eq!(bank.version_bytes, sqs::VERSION_UTF16BE);
}

// ---------------------------------------------------------------------------
// Negative parse cases
// ---------------------------------------------------------------------------

#[test]
fn parse_rejects_wrong_file_size() {
    let short = vec![0u8; sqs::FILE_LEN - 1];
    let err = sqs::parse_bank(&short).unwrap_err();
    assert!(err.to_string().contains("size mismatch"), "got: {}", err);

    let long = vec![0u8; sqs::FILE_LEN + 1];
    let err = sqs::parse_bank(&long).unwrap_err();
    assert!(err.to_string().contains("size mismatch"), "got: {}", err);
}

#[test]
fn parse_rejects_wrong_magic() {
    let mut data = read_golden("ALL TD-3 PATTERNS.sqs");
    data[0] = 0x00;
    let err = sqs::parse_bank(&data).unwrap_err();
    assert!(err.to_string().contains("magic"), "got: {}", err);
}

#[test]
fn parse_rejects_bad_record_payload_len() {
    let mut data = read_golden("ALL TD-3 PATTERNS.sqs");
    // First record's payload_len is at header_end (30) + 8 = 38. Corrupt it.
    let plen_off = sqs::HEADER_LEN + 8;
    data[plen_off..plen_off + 4].copy_from_slice(&111u32.to_be_bytes());
    let err = sqs::parse_bank(&data).unwrap_err();
    assert!(err.to_string().contains("payload_len"), "got: {}", err);
}

#[test]
fn parse_rejects_out_of_order_record() {
    // Corrupt record 0's group field so it no longer matches position.
    let mut data = read_golden("ALL TD-3 PATTERNS.sqs");
    let group_off = sqs::HEADER_LEN;
    data[group_off..group_off + 4].copy_from_slice(&1u32.to_be_bytes()); // should be 0
    let err = sqs::parse_bank(&data).unwrap_err();
    assert!(err.to_string().contains("out of order"), "got: {}", err);
}

#[test]
fn parse_rejects_group_out_of_range() {
    let mut data = read_golden("ALL TD-3 PATTERNS.sqs");
    // Walk to the last record (idx 63) and set its group to 99.
    let rec_off = sqs::HEADER_LEN + 63 * sqs::RECORD_LEN;
    data[rec_off..rec_off + 4].copy_from_slice(&99u32.to_be_bytes());
    let err = sqs::parse_bank(&data).unwrap_err();
    assert!(
        err.to_string().contains("out of range") || err.to_string().contains("out of order"),
        "got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// Address mapping & folder naming
// ---------------------------------------------------------------------------

#[test]
fn address_mapping_corners() {
    assert_eq!(sqs::folder_name(0, 0), "G1P1A");
    assert_eq!(sqs::folder_name(0, 7), "G1P8A");
    assert_eq!(sqs::folder_name(0, 8), "G1P1B");
    assert_eq!(sqs::folder_name(0, 15), "G1P8B");
    assert_eq!(sqs::folder_name(3, 0), "G4P1A");
    assert_eq!(sqs::folder_name(3, 15), "G4P8B");
    // Per-record address accessors on the golden bank:
    let data = read_golden("ALL TD-3 PATTERNS.sqs");
    let bank = sqs::parse_bank(&data).unwrap();
    assert_eq!(bank.records[0].slot_num(), 0);
    assert_eq!(bank.records[0].side(), 0);
    assert_eq!(bank.records[8].slot_num(), 0);
    assert_eq!(bank.records[8].side(), 1);
    assert_eq!(bank.records[63].slot_num(), 7);
    assert_eq!(bank.records[63].side(), 1);
}

// ---------------------------------------------------------------------------
// Silent detection - semantic REST mask
// ---------------------------------------------------------------------------

#[test]
fn silent_when_rest_mask_is_all_ones() {
    let mut payload = vec![0u8; 112];
    // REST mask at offset 0x6C..0x6F.
    payload[0x6C] = 0x0F;
    payload[0x6D] = 0x0F;
    payload[0x6E] = 0x0F;
    payload[0x6F] = 0x0F;
    assert!(sqs::is_silent(&payload));
}

#[test]
fn not_silent_when_any_step_not_rest() {
    let mut payload = vec![0u8; 112];
    payload[0x6C] = 0x0F;
    payload[0x6D] = 0x0F;
    payload[0x6E] = 0x0F;
    payload[0x6F] = 0x0E; // one step not REST
    assert!(!sqs::is_silent(&payload));
}

#[test]
fn factory_clean_record_is_silent() {
    let data = read_golden("20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs");
    let bank = sqs::parse_bank(&data).unwrap();
    // All 64 records should be silent (user reset entire bank).
    for (idx, rec) in bank.records.iter().enumerate() {
        assert!(
            sqs::is_silent(&rec.payload),
            "record {} ({}) should be silent",
            idx,
            sqs::folder_name(rec.group, rec.slot_addr)
        );
    }
}

#[test]
fn on_device_cleared_record_is_silent_despite_pitch_junk() {
    // Record 6 is G1P7A, CLEAR'd on device. Pitch/accent/slide contain
    // residual bytes but REST mask is 0F 0F 0F 0F - device plays silent.
    let data = read_golden("FULL_BANK_SQS_WITH_A_CLEARED_ON_DEVICE_SILENT_G1P7A.sqs");
    let bank = sqs::parse_bank(&data).unwrap();
    let g1p7a = &bank.records[6];
    assert_eq!(sqs::folder_name(g1p7a.group, g1p7a.slot_addr), "G1P7A");
    assert!(
        sqs::is_silent(&g1p7a.payload),
        "G1P7A after on-device CLEAR must be silent"
    );
    // Confirm pitch table is NOT factory-clean (residual junk from prior content).
    // Factory-clean pitch pattern is `01 08` repeating; CLEAR'd slot differs.
    let pitch_first_16 = &g1p7a.payload[0x02..0x12];
    let factory_pitch = [0x01u8, 0x08]
        .iter()
        .copied()
        .cycle()
        .take(16)
        .collect::<Vec<u8>>();
    assert_ne!(
        pitch_first_16,
        factory_pitch.as_slice(),
        "CLEAR'd slot pitch table should retain residual bytes, not factory template"
    );
}

// ---------------------------------------------------------------------------
// Marker byte preservation
// ---------------------------------------------------------------------------

#[test]
fn marker_bytes_preserved_through_round_trip() {
    // Golden bank has mixed markers - verify each is preserved verbatim.
    let data = read_golden("ALL TD-3 PATTERNS.sqs");
    let bank = sqs::parse_bank(&data).unwrap();
    let re_encoded = sqs::serialize_bank(&bank).unwrap();

    for (idx, rec) in bank.records.iter().enumerate() {
        let rec_off = sqs::HEADER_LEN + idx * sqs::RECORD_LEN + 12;
        let marker_in_file = [data[rec_off], data[rec_off + 1]];
        let marker_in_reencode = [re_encoded[rec_off], re_encoded[rec_off + 1]];
        assert_eq!(
            rec.marker(),
            marker_in_file,
            "record {} marker mismatch with source file",
            idx
        );
        assert_eq!(
            marker_in_reencode, marker_in_file,
            "record {} marker not preserved in re-encode",
            idx
        );
    }
}
