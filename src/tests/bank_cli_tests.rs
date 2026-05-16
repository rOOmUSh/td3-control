// Tests for `bank::extract_bank` / `bank::pack_bank` - folder ↔ `.sqs`.
//
// Principal contract: extract → pack round-trips byte-for-byte against the
// original `.sqs`, because the manifest carries marker bytes and the per-slot
// `.syx` files carry the raw 112-byte payloads (including pitch/accent/slide
// residue that a decode → encode path would strip).

use std::fs;
use std::path::{Path, PathBuf};

use crate::bank::{extract_bank, pack_bank};
use crate::formats::mid::MidiExportOptions;
use crate::formats::mid_import::MidiImportOptions;
use crate::formats::sqs;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
}

/// Produce a unique scratch directory under the target dir. Avoids the repo
/// tree and doesn't require the `tempfile` crate.
fn scratch_dir(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-scratch")
        .join(format!("{}_{}", label, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn rmrf(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

// ---------------------------------------------------------------------------
// Extract shape
// ---------------------------------------------------------------------------

#[test]
fn extract_creates_64_subfolders_and_manifest() {
    let scratch = scratch_dir("extract_shape");
    let out = scratch.join("bank");

    extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &out,
        false,
        &MidiExportOptions::default(),
    )
    .expect("extract must succeed");

    assert!(out.join("bank_manifest.json").is_file(), "manifest missing");

    for g in 0..4 {
        for s in 0..16 {
            let addr = sqs::folder_name(g, s);
            let folder = out.join(&addr);
            assert!(folder.is_dir(), "subfolder {} missing", addr);
            for ext in ["syx", "toml", "steps.txt", "json", "mid", "seq"] {
                let f = folder.join(format!("{}.{}", addr, ext));
                assert!(f.is_file(), "{} missing in {}", ext, addr);
            }
        }
    }

    rmrf(&scratch);
}

#[test]
fn extract_refuses_existing_dir_without_force() {
    let scratch = scratch_dir("extract_refuses");
    let out = scratch.join("bank");
    fs::create_dir_all(&out).unwrap();

    let err = extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &out,
        false,
        &MidiExportOptions::default(),
    )
    .expect_err("must refuse existing dir without --force");
    let msg = format!("{}", err);
    assert!(
        msg.contains("already exists"),
        "error should mention existence: {}",
        msg
    );

    rmrf(&scratch);
}

#[test]
fn extract_force_overwrites_existing_dir() {
    let scratch = scratch_dir("extract_force");
    let out = scratch.join("bank");
    fs::create_dir_all(&out).unwrap();
    // Write a stray file; extract with --force should still lay the tree on top.
    fs::write(out.join("stale.txt"), b"stale").unwrap();

    extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &out,
        true,
        &MidiExportOptions::default(),
    )
    .expect("extract --force must succeed");

    assert!(out.join("G1P1A").is_dir());
    assert!(out.join("bank_manifest.json").is_file());

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Round-trip: extract → pack byte-exact
// ---------------------------------------------------------------------------

#[test]
fn extract_then_pack_round_trips_byte_exact_golden() {
    let scratch = scratch_dir("rt_golden");
    let out_dir = scratch.join("bank");
    let out_file = scratch.join("repacked.sqs");

    let original = fs::read(golden_path("ALL TD-3 PATTERNS.sqs")).unwrap();

    extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &out_dir,
        false,
        &MidiExportOptions::default(),
    )
    .unwrap();
    pack_bank(&out_dir, &out_file, false, &MidiImportOptions::default()).unwrap();

    let repacked = fs::read(&out_file).unwrap();
    assert_eq!(
        original, repacked,
        "extract → pack must be byte-exact with manifest + .syx present"
    );

    rmrf(&scratch);
}

#[test]
fn extract_then_pack_round_trips_byte_exact_empty_bank() {
    let scratch = scratch_dir("rt_empty");
    let out_dir = scratch.join("bank");
    let out_file = scratch.join("repacked.sqs");

    let original = fs::read(golden_path(
        "20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs",
    ))
    .unwrap();

    extract_bank(
        &golden_path("20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs"),
        &out_dir,
        false,
        &MidiExportOptions::default(),
    )
    .unwrap();
    pack_bank(&out_dir, &out_file, false, &MidiImportOptions::default()).unwrap();

    let repacked = fs::read(&out_file).unwrap();
    assert_eq!(
        original, repacked,
        "empty-bank round-trip must also be byte-exact"
    );

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Pack error paths
// ---------------------------------------------------------------------------

#[test]
fn pack_fails_if_subfolder_missing() {
    let scratch = scratch_dir("pack_missing");
    let dir = scratch.join("bank");

    extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &dir,
        false,
        &MidiExportOptions::default(),
    )
    .unwrap();
    // Remove one of the 64 expected subfolders.
    fs::remove_dir_all(dir.join("G2P5B")).unwrap();

    let err = pack_bank(
        &dir,
        &scratch.join("out.sqs"),
        false,
        &MidiImportOptions::default(),
    )
    .expect_err("pack must refuse a tree with a missing slot");
    let msg = format!("{}", err);
    assert!(
        msg.contains("G2P5B"),
        "error should name missing slot: {}",
        msg
    );

    rmrf(&scratch);
}

#[test]
fn pack_refuses_existing_output_without_force() {
    let scratch = scratch_dir("pack_refuses");
    let dir = scratch.join("bank");
    let out = scratch.join("out.sqs");

    extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &dir,
        false,
        &MidiExportOptions::default(),
    )
    .unwrap();
    fs::write(&out, b"prior content").unwrap();

    let err = pack_bank(&dir, &out, false, &MidiImportOptions::default())
        .expect_err("pack must refuse an existing file without --force");
    let msg = format!("{}", err);
    assert!(
        msg.contains("already exists"),
        "error should mention existence: {}",
        msg
    );

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Manifest fallback: pack without a manifest still produces a valid file,
// but marker bytes are forced to the CLI default 00 01 for all records.
// ---------------------------------------------------------------------------

#[test]
fn pack_without_manifest_warns_and_uses_default_marker() {
    let scratch = scratch_dir("pack_no_manifest");
    let dir = scratch.join("bank");

    extract_bank(
        &golden_path("ALL TD-3 PATTERNS.sqs"),
        &dir,
        false,
        &MidiExportOptions::default(),
    )
    .unwrap();
    // Remove the manifest - pack should still succeed but with default marker.
    fs::remove_file(dir.join("bank_manifest.json")).unwrap();
    // Also remove every .syx so the raw-byte shortcut doesn't mask the default.
    for g in 0..4 {
        for s in 0..16 {
            let addr = sqs::folder_name(g, s);
            let _ = fs::remove_file(dir.join(&addr).join(format!("{}.syx", addr)));
        }
    }

    let out_file = scratch.join("repacked.sqs");
    pack_bank(&dir, &out_file, false, &MidiImportOptions::default())
        .expect("pack must succeed without manifest");

    let bytes = fs::read(&out_file).unwrap();
    let bank = sqs::parse_bank(&bytes).unwrap();
    // Every record's marker should now be 00 01 (the CLI default).
    for rec in bank.records.iter() {
        assert_eq!(
            rec.marker(),
            [0x00, 0x01],
            "record {} marker must be 00 01 when no manifest + no .syx",
            sqs::folder_name(rec.group, rec.slot_addr)
        );
    }

    rmrf(&scratch);
}
