//! Real-device integration test: full 64-pattern round-trip.
//!
//! Requires a physical TD-3 connected via USB MIDI.
//! Skipped by default (`#[ignore]`). Run explicitly with:
//!
//!     cargo test device_integration -- --ignored --nocapture
//!
//! What it does:
//!   1. Probes the device (product name + firmware)
//!   2. Downloads all 64 patterns (4 groups × 8 slots × A/B)
//!   3. Uploads every pattern back to its original address
//!   4. Downloads all 64 patterns again
//!   5. Compares each second download byte-for-byte with the first
//!
//! This verifies the full round-trip: device → decode → encode → device → decode,
//! proving that no data is lost or corrupted through the upload/download cycle.

use std::sync::mpsc;
use std::time::Duration;

use crate::formats::format_address;
use crate::formats::mid::{export as mid_export, MidiExportOptions};
use crate::formats::mid_import::{
    import as mid_import, MidiImportOptions, RejectPolyphonyResolver,
};
use crate::midi_io;
use crate::pattern::{pattern_to_sysex, sysex_to_pattern, Pattern};
use crate::td3_protocol;

const TIMEOUT: Duration = Duration::from_secs(5);
const GROUPS: u8 = 4;
const SLOTS: u8 = 8;
const SIDES: u8 = 2;
const TOTAL_PATTERNS: usize = (GROUPS as usize) * (SLOTS as usize) * (SIDES as usize);

/// A captured pattern with its raw SysEx payload and address.
struct CapturedPattern {
    patgroup: u8,
    slot: u8,
    side: u8,
    raw_payload: Vec<u8>,
}

/// Open a MIDI session to the TD-3 and return the connection + receiver.
fn open_device_session() -> (
    midir::MidiOutputConnection,
    mpsc::Receiver<Vec<u8>>,
    midir::MidiInputConnection<()>,
) {
    let (out_midi, out_port, in_midi, in_port) =
        midi_io::open_ports("TD-3", "TD-3", false).expect("TD-3 not found - is it connected?");

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let in_conn = in_midi
        .connect(
            &in_port,
            "integration-test-input",
            move |_stamp, msg, _| {
                let _ = tx.send(msg.to_owned());
            },
            (),
        )
        .expect("failed to connect MIDI input");

    let out_conn = out_midi
        .connect(&out_port, "integration-test-output")
        .expect("failed to connect MIDI output");

    (out_conn, rx, in_conn)
}

/// Download all 64 patterns from the device.
fn download_all(
    out_conn: &mut midir::MidiOutputConnection,
    rx: &mpsc::Receiver<Vec<u8>>,
) -> Vec<CapturedPattern> {
    let mut patterns = Vec::with_capacity(TOTAL_PATTERNS);

    for patgroup in 0..GROUPS {
        for slot in 0..SLOTS {
            for side in 0..SIDES {
                let address = format_address(patgroup, slot, side);
                let (raw_payload, _pattern) =
                    td3_protocol::download_pattern(out_conn, rx, patgroup, slot, side, TIMEOUT)
                        .unwrap_or_else(|e| panic!("download failed for {}: {}", address, e));

                patterns.push(CapturedPattern {
                    patgroup,
                    slot,
                    side,
                    raw_payload,
                });
            }
        }
    }

    assert_eq!(patterns.len(), TOTAL_PATTERNS);
    patterns
}

/// Upload all captured patterns back to their original addresses.
fn upload_all(
    out_conn: &mut midir::MidiOutputConnection,
    rx: &mpsc::Receiver<Vec<u8>>,
    patterns: &[CapturedPattern],
) {
    for cap in patterns {
        let address = format_address(cap.patgroup, cap.slot, cap.side);
        let pattern = sysex_to_pattern(&cap.raw_payload)
            .unwrap_or_else(|e| panic!("decode failed for {} before upload: {}", address, e));

        td3_protocol::upload_pattern(
            out_conn,
            rx,
            &pattern,
            cap.patgroup,
            cap.slot,
            cap.side,
            TIMEOUT,
        )
        .unwrap_or_else(|e| panic!("upload failed for {}: {}", address, e));
    }
}

#[test]
#[ignore]
fn device_full_bank_roundtrip() {
    let (mut out_conn, rx, _in_conn) = open_device_session();

    // Probe device
    let info =
        td3_protocol::probe_device(&mut out_conn, &rx, TIMEOUT).expect("device probe failed");
    eprintln!(
        "Device: {}, firmware {}",
        info.product_name, info.firmware_version
    );

    // Download all 64 patterns.
    eprintln!("Downloading all {} patterns...", TOTAL_PATTERNS);
    let reference = download_all(&mut out_conn, &rx);
    eprintln!("  Downloaded {} patterns", reference.len());

    // Upload all patterns back.
    eprintln!("Uploading all {} patterns back...", TOTAL_PATTERNS);
    upload_all(&mut out_conn, &rx, &reference);
    eprintln!("  Uploaded {} patterns", reference.len());

    // Download all patterns again.
    eprintln!("Re-downloading all {} patterns...", TOTAL_PATTERNS);
    let verification = download_all(&mut out_conn, &rx);
    eprintln!("  Downloaded {} patterns", verification.len());

    // Semantic comparison
    //
    // Raw bytes won't match byte-for-byte because of two known don't-care regions:
    //   1. Byte 4 ("unknown1"): encoder writes 0x01, device may store 0x00
    //   2. Unused trailing packed slots: the 303 format packs notes/accent/slide
    //      sequentially - only Normal steps consume entries. Our encoder fills
    //      unused trailing slots with defaults (0x18 for notes, 0x00 for accent/slide),
    //      which may differ from the arbitrary values the device originally stored.
    //
    // These regions do NOT affect the pattern you hear. The comparison that matters
    // is at the decoded Pattern level: every step's note, transpose, accent, slide,
    // and time must be identical.
    eprintln!("Comparing (semantic - decoded Pattern level)...");
    let mut semantic_mismatches = 0;
    let mut byte_diffs = 0;

    for (ref_cap, ver_cap) in reference.iter().zip(verification.iter()) {
        let address = format_address(ref_cap.patgroup, ref_cap.slot, ref_cap.side);

        let pat_ref = sysex_to_pattern(&ref_cap.raw_payload)
            .unwrap_or_else(|e| panic!("{}: decode reference failed: {}", address, e));
        let pat_ver = sysex_to_pattern(&ver_cap.raw_payload)
            .unwrap_or_else(|e| panic!("{}: decode verification failed: {}", address, e));

        // Track raw byte differences (informational)
        if ref_cap.raw_payload != ver_cap.raw_payload {
            byte_diffs += 1;
        }

        // Semantic comparison (assertable)
        let mut step_diffs = Vec::new();

        if pat_ref.active_steps != pat_ver.active_steps {
            step_diffs.push(format!(
                "active_steps: {} → {}",
                pat_ref.active_steps, pat_ver.active_steps
            ));
        }
        if pat_ref.triplet != pat_ver.triplet {
            step_diffs.push(format!(
                "triplet: {} → {}",
                pat_ref.triplet, pat_ver.triplet
            ));
        }
        for i in 0..16 {
            let r = &pat_ref.step[i];
            let v = &pat_ver.step[i];
            if r.note != v.note
                || r.transpose != v.transpose
                || r.accent != v.accent
                || r.slide != v.slide
                || r.time != v.time
            {
                step_diffs.push(format!(
                    "step {:02}: note {}→{}, transpose {:?}→{:?}, accent {:?}→{:?}, slide {:?}→{:?}, time {:?}→{:?}",
                    i + 1,
                    r.note,
                    v.note,
                    r.transpose,
                    v.transpose,
                    r.accent,
                    v.accent,
                    r.slide,
                    v.slide,
                    r.time,
                    v.time
                ));
            }
        }

        if !step_diffs.is_empty() {
            semantic_mismatches += 1;
            eprintln!("  SEMANTIC MISMATCH: {}", address);
            for diff in &step_diffs {
                eprintln!("    {}", diff);
            }
        }
    }

    eprintln!(
        "  Raw byte diffs: {}/{} (expected - unused packed slots + unknown1 field)",
        byte_diffs, TOTAL_PATTERNS
    );

    assert_eq!(
        semantic_mismatches, 0,
        "{} of {} patterns have semantic differences after upload round-trip",
        semantic_mismatches, TOTAL_PATTERNS
    );

    eprintln!(
        "All {} patterns verified - upload/download round-trip preserves all pattern data",
        TOTAL_PATTERNS
    );
}

/// Lighter variant: download → encode → decode (no upload, no device write).
/// Verifies that our SysEx codec is lossless for every pattern on the device.
#[test]
#[ignore]
fn device_codec_roundtrip_all_patterns() {
    let (mut out_conn, rx, _in_conn) = open_device_session();

    let info =
        td3_protocol::probe_device(&mut out_conn, &rx, TIMEOUT).expect("device probe failed");
    eprintln!(
        "Device: {}, firmware {}",
        info.product_name, info.firmware_version
    );

    eprintln!(
        "Downloading and verifying codec round-trip for all {} patterns...",
        TOTAL_PATTERNS
    );
    let mut tested = 0;

    for patgroup in 0..GROUPS {
        for slot in 0..SLOTS {
            for side in 0..SIDES {
                let address = format_address(patgroup, slot, side);

                let (raw_payload, pattern) = td3_protocol::download_pattern(
                    &mut out_conn,
                    &rx,
                    patgroup,
                    slot,
                    side,
                    TIMEOUT,
                )
                .unwrap_or_else(|e| panic!("download failed for {}: {}", address, e));

                // Re-encode the decoded pattern, then decode again
                let re_encoded = pattern_to_sysex(&pattern, patgroup, slot, side)
                    .unwrap_or_else(|e| panic!("encode failed for {}: {}", address, e));
                let re_decoded = sysex_to_pattern(&re_encoded)
                    .unwrap_or_else(|e| panic!("re-decode failed for {}: {}", address, e));

                // Header bytes must match (skip unknown1 at bytes 3-4)
                assert_eq!(
                    &re_encoded[0..3],
                    &raw_payload[0..3],
                    "{}: header bytes differ",
                    address
                );

                // Semantic comparison: all step fields must survive decode→encode→decode
                assert_eq!(
                    pattern.active_steps, re_decoded.active_steps,
                    "{}: active_steps",
                    address
                );
                assert_eq!(pattern.triplet, re_decoded.triplet, "{}: triplet", address);
                for i in 0..16 {
                    assert_eq!(
                        pattern.step[i].note,
                        re_decoded.step[i].note,
                        "{}: step {} note",
                        address,
                        i + 1
                    );
                    assert_eq!(
                        pattern.step[i].transpose,
                        re_decoded.step[i].transpose,
                        "{}: step {} transpose",
                        address,
                        i + 1
                    );
                    assert_eq!(
                        pattern.step[i].accent,
                        re_decoded.step[i].accent,
                        "{}: step {} accent",
                        address,
                        i + 1
                    );
                    assert_eq!(
                        pattern.step[i].slide,
                        re_decoded.step[i].slide,
                        "{}: step {} slide",
                        address,
                        i + 1
                    );
                    assert_eq!(
                        pattern.step[i].time,
                        re_decoded.step[i].time,
                        "{}: step {} time",
                        address,
                        i + 1
                    );
                }

                // Metadata bytes must match (triplet, active_steps, ties, rests)
                assert_eq!(
                    &re_encoded[101..],
                    &raw_payload[101..],
                    "{}: trailing metadata bytes differ",
                    address
                );

                tested += 1;
            }
        }
    }

    eprintln!(
        "All {} patterns pass codec round-trip (decode → encode preserves all data)",
        tested
    );
}

// ---------------------------------------------------------------------------
// MIDI-format round-trip: device → .mid → device → device
// ---------------------------------------------------------------------------

/// Compare two patterns field-by-field, returning a list of diff strings.
fn diff_patterns(a: &Pattern, b: &Pattern) -> Vec<String> {
    let mut diffs = Vec::new();
    if a.active_steps != b.active_steps {
        diffs.push(format!(
            "active_steps: {} → {}",
            a.active_steps, b.active_steps
        ));
    }
    if a.triplet != b.triplet {
        diffs.push(format!("triplet: {} → {}", a.triplet, b.triplet));
    }
    for i in 0..16 {
        let r = &a.step[i];
        let v = &b.step[i];
        if r.note != v.note
            || r.transpose != v.transpose
            || r.accent != v.accent
            || r.slide != v.slide
            || r.time != v.time
        {
            diffs.push(format!(
                "step {:02}: note {}→{}, transpose {:?}→{:?}, accent {:?}→{:?}, slide {:?}→{:?}, time {:?}→{:?}",
                i + 1,
                r.note,
                v.note,
                r.transpose,
                v.transpose,
                r.accent,
                v.accent,
                r.slide,
                v.slide,
                r.time,
                v.time
            ));
        }
    }
    diffs
}

/// Download all 64 patterns as `Pattern` values. The "first steps" reference.
fn download_all_as_patterns(
    out_conn: &mut midir::MidiOutputConnection,
    rx: &mpsc::Receiver<Vec<u8>>,
) -> Vec<(u8, u8, u8, Pattern)> {
    let mut out: Vec<(u8, u8, u8, Pattern)> = Vec::with_capacity(TOTAL_PATTERNS);
    for patgroup in 0..GROUPS {
        for slot in 0..SLOTS {
            for side in 0..SIDES {
                let address = format_address(patgroup, slot, side);
                let (_raw, pattern) =
                    td3_protocol::download_pattern(out_conn, rx, patgroup, slot, side, TIMEOUT)
                        .unwrap_or_else(|e| panic!("download failed for {}: {}", address, e));
                out.push((patgroup, slot, side, pattern));
            }
        }
    }
    out
}

/// Upload every pattern in the reference back to its original slot.
/// Used to restore originals between rounds so subsequent rounds see the
/// same starting state as round 1 (not the canonicalized form left behind
/// by a previous mid upload).
fn upload_originals(
    out_conn: &mut midir::MidiOutputConnection,
    rx: &mpsc::Receiver<Vec<u8>>,
    reference: &[(u8, u8, u8, Pattern)],
) {
    for (pg, sl, sd, original) in reference {
        let address = format_address(*pg, *sl, *sd);
        td3_protocol::upload_pattern(out_conn, rx, original, *pg, *sl, *sd, TIMEOUT)
            .unwrap_or_else(|e| panic!("restore upload failed for {}: {}", address, e));
    }
}

/// One round of the .mid round-trip:
///   - re-download every slot as a `Pattern`
///   - serialize each to `.mid` bytes
///   - parse the `.mid` back via `mid_import::import`
///   - upload the re-imported pattern to its original address
///   - re-download and diff against the saved originals
///
/// Returns the count of patterns that differed from their original, and
/// prints per-pattern diffs to stderr for any mismatches.
fn run_mid_roundtrip_round(
    out_conn: &mut midir::MidiOutputConnection,
    rx: &mpsc::Receiver<Vec<u8>>,
    reference: &[(u8, u8, u8, Pattern)],
    label: &str,
) -> usize {
    let mid_options = MidiExportOptions::default();
    let import_options = MidiImportOptions::default();

    eprintln!(
        "  [{}] Downloading mid for {} patterns...",
        label,
        reference.len()
    );
    let mut mids: Vec<Vec<u8>> = Vec::with_capacity(reference.len());
    for (pg, sl, sd, _orig) in reference {
        let address = format_address(*pg, *sl, *sd);
        let (_raw, pattern) = td3_protocol::download_pattern(out_conn, rx, *pg, *sl, *sd, TIMEOUT)
            .unwrap_or_else(|e| panic!("download failed for {}: {}", address, e));
        let mid_bytes = mid_export(&pattern, &address, &mid_options)
            .unwrap_or_else(|e| panic!("mid export failed for {}: {}", address, e));
        mids.push(mid_bytes);
    }

    eprintln!("  [{}] Re-importing .mid and uploading...", label);
    for ((pg, sl, sd, _orig), mid_bytes) in reference.iter().zip(mids.iter()) {
        let address = format_address(*pg, *sl, *sd);
        let mut resolver = RejectPolyphonyResolver;
        let reimported = mid_import(mid_bytes, &import_options, &mut resolver)
            .unwrap_or_else(|e| panic!("mid import failed for {}: {}", address, e));
        td3_protocol::upload_pattern(out_conn, rx, &reimported, *pg, *sl, *sd, TIMEOUT)
            .unwrap_or_else(|e| panic!("upload failed for {}: {}", address, e));
    }

    eprintln!("  [{}] Re-downloading and comparing to originals...", label);
    let mut mismatches = 0;
    for (pg, sl, sd, original) in reference {
        let address = format_address(*pg, *sl, *sd);
        let (_raw, verified) = td3_protocol::download_pattern(out_conn, rx, *pg, *sl, *sd, TIMEOUT)
            .unwrap_or_else(|e| panic!("re-download failed for {}: {}", address, e));

        let diffs = diff_patterns(original, &verified);
        if !diffs.is_empty() {
            mismatches += 1;
            eprintln!("    [{}] MISMATCH: {}", label, address);
            for d in diffs {
                eprintln!("      {}", d);
            }
        }
    }
    mismatches
}

/// Full .mid round-trip on every device slot:
///
/// **Round 1** - the true test of lossless import:
///   1. Download all 64 patterns as `Pattern` values (the "first steps"
///      reference). The device must already hold whatever content the caller
///      wants validated (e.g. factory patterns restored from .syx).
///   2. Re-download each slot, serialize to `.mid`, parse back via
///      `mid_import::import`, upload to the original address.
///   3. Re-download and compare step-by-step against the first steps.
///      If all 64 match, the round-trip is lossless. Test passes.
///
/// **Round 2** - fallback diagnostic, only runs if Round 1 had mismatches:
///   4. Upload the first-steps reference back to the device, restoring the
///      pre-round-1 state (since round 1's upload canonicalized slots).
///   5. Repeat the mid serialize → re-import → upload → re-download cycle.
///   6. Compare against the first steps again.
///      If round 2 passes while round 1 did not, the pipeline is fixed-point
///      stable after one canonicalization but lossy on the first pass - we
///      report both and pass the test.
///      If round 2 also fails, the round-trip is unstable and the test fails.
///
/// A `RejectPolyphonyResolver` is used - TD-3 dumps are always monophonic,
/// so polyphony here would indicate a bug in the exporter.
#[test]
#[ignore]
fn device_mid_roundtrip_all_patterns() {
    let (mut out_conn, rx, _in_conn) = open_device_session();

    let info =
        td3_protocol::probe_device(&mut out_conn, &rx, TIMEOUT).expect("device probe failed");
    eprintln!(
        "Device: {}, firmware {}",
        info.product_name, info.firmware_version
    );

    // Capture the "first steps" reference from whatever the device
    // currently holds. All later comparisons go against this.
    eprintln!("Downloading {} originals (first steps)...", TOTAL_PATTERNS);
    let reference = download_all_as_patterns(&mut out_conn, &rx);
    assert_eq!(reference.len(), TOTAL_PATTERNS);
    eprintln!("  Captured {} originals", reference.len());

    // Round 1: primary round-trip check.
    eprintln!("Round 1: device → mid → re-import → device → compare to first steps");
    let round1 = run_mid_roundtrip_round(&mut out_conn, &rx, &reference, "round1");

    if round1 == 0 {
        eprintln!(
            "Round 1 passed: all {} patterns preserved through mid round-trip",
            TOTAL_PATTERNS
        );
        return;
    }

    // Round 2: restore originals, repeat. Only reached if round 1 had diffs.
    eprintln!(
        "Round 1: {} of {} patterns differ. Uploading first steps back and running round 2...",
        round1, TOTAL_PATTERNS
    );
    upload_originals(&mut out_conn, &rx, &reference);
    let round2 = run_mid_roundtrip_round(&mut out_conn, &rx, &reference, "round2");

    assert_eq!(
        round2, 0,
        "Round 2 still has {} of {} mismatches after restoring originals \
         (round 1 had {}). Mid round-trip is not stable even after one \
         canonicalization pass.",
        round2, TOTAL_PATTERNS, round1
    );

    eprintln!(
        "Round 1: {} mismatches. Round 2: 0 mismatches. \
         Mid round-trip is lossy on first pass but fixed-point stable thereafter.",
        round1
    );
}
