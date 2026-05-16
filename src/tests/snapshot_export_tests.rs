//! Tests for `web::snapshot_export` - the pure helper that writes per-slot
//! pattern files into a user-chosen target directory.
//!
//! These deliberately avoid axum / LibraryStore plumbing: the `run()`
//! function takes the 112-byte payload directly, so we only need a
//! temp directory + a known-good payload from the fixtures module.

use std::fs;
use std::path::PathBuf;

use crate::formats::mid::MidiExportOptions;
use crate::tests::fixtures::simple_sysex;
use crate::web::snapshot_export::{
    run, sanitize_component, slot_key_to_filename, validate_formats, ExportRequest, ExportSlot,
    ALLOWED_FORMATS,
};

/// Test scaffolding only: tests don't have an `AppEnv` in scope, so they use
/// `MidiExportOptions::default()`. The runtime `AppState::midi_export_options`
/// is built via `from_env(&env)` and never reaches this default at runtime.
fn test_midi_opts() -> MidiExportOptions {
    MidiExportOptions::default()
}

/// `simple_sysex()` returns a 115-byte device message with the 3-byte header
/// `[0x78, patgroup, slot_addr]`. The 112-byte body after the header is what
/// the library sidecar stores and what the export helper expects.
fn simple_payload() -> Vec<u8> {
    simple_sysex()[3..].to_vec()
}

/// Create a unique per-test temp dir under the system tempdir, so parallel
/// `cargo test` runs don't collide.
fn temp_dir_for(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("td3_snapshot_export_{}_{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// sanitize_component / slot_key_to_filename
// ---------------------------------------------------------------------------

#[test]
fn sanitize_strips_bad_chars() {
    assert_eq!(sanitize_component("idea.rbs"), "idea.rbs");
    assert_eq!(sanitize_component("idea"), "idea");
    assert_eq!(sanitize_component("a/b\\c:d"), "a_b_c_d");
    assert_eq!(sanitize_component("  weird  name  "), "weird_name");
}

#[test]
fn sanitize_empty_fallback() {
    assert_eq!(sanitize_component(""), "snapshot");
    assert_eq!(sanitize_component("///"), "snapshot");
}

#[test]
fn slot_key_filename_strips_dash() {
    assert_eq!(slot_key_to_filename("G1-P1A"), "G1P1A");
    assert_eq!(slot_key_to_filename("G4-P8B"), "G4P8B");
}

// ---------------------------------------------------------------------------
// validate_formats
// ---------------------------------------------------------------------------

#[test]
fn validate_accepts_allowed_formats() {
    for f in ALLOWED_FORMATS {
        validate_formats(&[f.to_string()])
            .unwrap_or_else(|e| panic!("should accept '{}': {}", f, e));
    }
}

#[test]
fn validate_rejects_syx_explicitly() {
    let err = validate_formats(&["syx".to_string()]).unwrap_err();
    assert!(
        err.to_string().contains("syx"),
        "error mentions syx: {}",
        err
    );
}

#[test]
fn validate_rejects_sqs_explicitly() {
    let err = validate_formats(&["sqs".to_string()]).unwrap_err();
    assert!(
        err.to_string().contains("sqs"),
        "error mentions sqs: {}",
        err
    );
}

#[test]
fn validate_rejects_empty_format_list() {
    let err = validate_formats(&[]).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("no formats"));
}

#[test]
fn validate_rejects_unknown_format() {
    let err = validate_formats(&["gltf".to_string()]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("gltf"), "names offender: {}", msg);
    assert!(msg.contains("allowed"), "mentions allowed set: {}", msg);
}

// ---------------------------------------------------------------------------
// run(): happy paths
// ---------------------------------------------------------------------------

#[test]
fn run_writes_files_with_slot_named_prefixes() {
    let target = temp_dir_for("named_prefixes");
    let slots = vec![
        ExportSlot {
            slot_key: "G1-P1A".to_string(),
            payload: Some(simple_payload()),
        },
        ExportSlot {
            slot_key: "G2-P3B".to_string(),
            payload: Some(simple_payload()),
        },
    ];
    let formats = ["toml".to_string(), "steps_txt".to_string()];
    let midi_opts = test_midi_opts();
    let result = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "idea.rbs",
        slots: &slots,
        formats: &formats,
        midi_opts: &midi_opts,
    })
    .unwrap();

    assert_eq!(result.file_count, 4, "2 slots × 2 formats = 4 files");
    assert!(result.skipped.is_empty());

    let expected_dir = target.join("idea.rbs_export");
    assert!(expected_dir.is_dir(), "export folder created");

    for name in &[
        "G1P1A.toml",
        "G1P1A.steps.txt",
        "G2P3B.toml",
        "G2P3B.steps.txt",
    ] {
        let p = expected_dir.join(name);
        let meta = fs::metadata(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
        assert!(meta.len() > 0, "{} is non-empty", p.display());
    }

    fs::remove_dir_all(&target).ok();
}

#[test]
fn run_skips_empty_slots_without_erroring() {
    let target = temp_dir_for("skip_empty");
    let slots = vec![
        ExportSlot {
            slot_key: "G1-P1A".to_string(),
            payload: Some(simple_payload()),
        },
        ExportSlot {
            slot_key: "G1-P2A".to_string(),
            payload: None,
        },
        ExportSlot {
            slot_key: "G1-P3A".to_string(),
            payload: None,
        },
    ];
    let formats = ["pat".to_string()];
    let midi_opts = test_midi_opts();
    let result = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "mixed",
        slots: &slots,
        formats: &formats,
        midi_opts: &midi_opts,
    })
    .unwrap();

    assert_eq!(result.file_count, 1);
    assert_eq!(
        result.skipped,
        vec!["G1-P2A".to_string(), "G1-P3A".to_string()]
    );

    let dir = target.join("mixed_export");
    assert!(dir.join("G1P1A.pat").is_file());
    assert!(!dir.join("G1P2A.pat").exists(), "empty slot is not written");

    fs::remove_dir_all(&target).ok();
}

#[test]
fn run_renders_all_allowed_formats_for_one_slot() {
    let target = temp_dir_for("all_formats");
    let slots = vec![ExportSlot {
        slot_key: "G1-P1A".to_string(),
        payload: Some(simple_payload()),
    }];
    let formats: Vec<String> = ALLOWED_FORMATS.iter().map(|s| s.to_string()).collect();
    let midi_opts = test_midi_opts();
    let result = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "all",
        slots: &slots,
        formats: &formats,
        midi_opts: &midi_opts,
    })
    .unwrap();

    assert_eq!(result.file_count as usize, ALLOWED_FORMATS.len());

    let dir = target.join("all_export");
    let expected: &[&str] = &[
        "G1P1A.toml",
        "G1P1A.json",
        "G1P1A.steps.txt",
        "G1P1A.pat",
        "G1P1A.seq",
        "G1P1A.mid",
        "G1P1A.rbs",
    ];
    for name in expected {
        let p = dir.join(name);
        assert!(p.is_file(), "missing {}", p.display());
        assert!(fs::metadata(&p).unwrap().len() > 0, "empty {}", p.display());
    }

    fs::remove_dir_all(&target).ok();
}

#[test]
fn run_creates_nested_target_if_missing_parent_exists() {
    // `create_dir_all` makes the `_export` sub-folder but the target dir
    // itself must exist. Verify that a pre-existing `_export` folder is
    // reused (idempotent) instead of erroring.
    let target = temp_dir_for("idempotent");
    let pre_existing = target.join("reuse_export");
    fs::create_dir_all(&pre_existing).unwrap();
    fs::write(pre_existing.join("old-file.txt"), b"leftover").unwrap();

    let slots = vec![ExportSlot {
        slot_key: "G1-P1A".to_string(),
        payload: Some(simple_payload()),
    }];
    let midi_opts = test_midi_opts();
    run(&ExportRequest {
        target_dir: &target,
        folder_stem: "reuse",
        slots: &slots,
        formats: &["json".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap();

    assert!(
        pre_existing.join("old-file.txt").is_file(),
        "pre-existing files preserved"
    );
    assert!(
        pre_existing.join("G1P1A.json").is_file(),
        "new file written alongside"
    );

    fs::remove_dir_all(&target).ok();
}

// ---------------------------------------------------------------------------
// run(): error paths
// ---------------------------------------------------------------------------

#[test]
fn run_rejects_missing_target_dir() {
    let target = std::env::temp_dir().join("td3_snapshot_export_does_not_exist_nonce_xyz");
    // Ensure it's really absent.
    let _ = fs::remove_dir_all(&target);

    let midi_opts = test_midi_opts();
    let err = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "x",
        slots: &[ExportSlot {
            slot_key: "G1-P1A".to_string(),
            payload: Some(simple_payload()),
        }],
        formats: &["toml".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("does not exist"),
        "reports missing dir: {}",
        msg
    );
}

#[test]
fn run_rejects_target_that_is_a_file() {
    let target_parent = temp_dir_for("file_target_parent");
    let file_path = target_parent.join("a-file.txt");
    fs::write(&file_path, b"hi").unwrap();

    let midi_opts = test_midi_opts();
    let err = run(&ExportRequest {
        target_dir: &file_path,
        folder_stem: "x",
        slots: &[ExportSlot {
            slot_key: "G1-P1A".to_string(),
            payload: Some(simple_payload()),
        }],
        formats: &["toml".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap_err();
    assert!(err.to_string().contains("not a directory"));

    fs::remove_dir_all(&target_parent).ok();
}

#[test]
fn run_rejects_empty_slot_list() {
    let target = temp_dir_for("no_slots");
    let midi_opts = test_midi_opts();
    let err = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "x",
        slots: &[],
        formats: &["toml".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("no slots"));
    fs::remove_dir_all(&target).ok();
}

#[test]
fn run_rejects_bad_payload_length() {
    let target = temp_dir_for("bad_payload");
    // 50-byte payload instead of 112.
    let bogus = vec![0u8; 50];
    let midi_opts = test_midi_opts();
    let err = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "x",
        slots: &[ExportSlot {
            slot_key: "G1-P1A".to_string(),
            payload: Some(bogus),
        }],
        formats: &["toml".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("G1-P1A"), "names offending slot: {}", msg);
    assert!(msg.contains("112"), "mentions expected length: {}", msg);
    fs::remove_dir_all(&target).ok();
}

#[test]
fn run_rejects_sqs_and_syx_even_if_list_also_has_valid() {
    let target = temp_dir_for("mixed_bad");
    let slots = vec![ExportSlot {
        slot_key: "G1-P1A".to_string(),
        payload: Some(simple_payload()),
    }];

    let midi_opts = test_midi_opts();
    let err_sqs = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "x",
        slots: &slots,
        formats: &["toml".to_string(), "sqs".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap_err();
    assert!(err_sqs.to_string().contains("sqs"));

    let err_syx = run(&ExportRequest {
        target_dir: &target,
        folder_stem: "x",
        slots: &slots,
        formats: &["syx".to_string(), "toml".to_string()],
        midi_opts: &midi_opts,
    })
    .unwrap_err();
    assert!(err_syx.to_string().contains("syx"));

    // Neither call should have created the export folder (validate_formats
    // runs before any filesystem work).
    assert!(!target.join("x_export").exists());

    fs::remove_dir_all(&target).ok();
}
