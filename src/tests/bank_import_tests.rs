// Integration tests for `bank::import::import_bank` using mock device + prompt.
//
// The orchestrator's safety promise is "NO PATTERN MUST BE LOST". These tests
// exercise the core invariants:
//
//   - a backup zip always lands on disk before ANY device write
//   - user "N" (or EOF) aborts without writing
//   - `--partial` only narrows the write set; the backup still covers all 64
//   - silent source patterns are filtered by default, unless `--include-silent`
//   - a download error aborts before any backup or write occurs
//   - an upload error mid-loop surfaces the error but the backup is intact

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::bank::address::{parse_partial, BankAddress};
use crate::bank::import::{import_bank, BankDevice, ImportOptions, UserPrompt};
use crate::error::Td3Error;
use crate::formats::sqs::{self, serialize_bank, Bank, BankRecord, RECORD_COUNT};

// ---------------------------------------------------------------------------
// Scratch-dir helpers (shared with bank_cli_tests conventions)
// ---------------------------------------------------------------------------

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

fn rmrf(path: &std::path::Path) {
    let _ = fs::remove_dir_all(path);
}

// ---------------------------------------------------------------------------
// Mock device + prompt
// ---------------------------------------------------------------------------

/// In-memory device. `state` maps a file-order slot index (0..64) to its
/// current 112-byte payload. `download_fail_at` / `upload_fail_at` let a test
/// simulate a transport error at a specific call count.
struct MockDevice {
    state: HashMap<(u8, u8), Vec<u8>>,
    download_calls: Vec<(u8, u8)>,
    upload_calls: Vec<(u8, u8, Vec<u8>)>,
    download_fail_at: Option<usize>,
    upload_fail_at: Option<usize>,
}

impl MockDevice {
    fn new(bank: &Bank) -> Self {
        let mut state = HashMap::new();
        for rec in bank.records.iter() {
            state.insert((rec.group, rec.slot_addr), rec.payload.clone());
        }
        Self {
            state,
            download_calls: Vec::new(),
            upload_calls: Vec::new(),
            download_fail_at: None,
            upload_fail_at: None,
        }
    }

    fn with_download_failure(mut self, at_call_index: usize) -> Self {
        self.download_fail_at = Some(at_call_index);
        self
    }

    fn with_upload_failure(mut self, at_call_index: usize) -> Self {
        self.upload_fail_at = Some(at_call_index);
        self
    }
}

impl BankDevice for MockDevice {
    fn download(&mut self, group: u8, slot_addr: u8) -> Result<Vec<u8>, Td3Error> {
        let call_idx = self.download_calls.len();
        self.download_calls.push((group, slot_addr));
        if Some(call_idx) == self.download_fail_at {
            return Err(Td3Error::Timeout {
                operation: "mock download".to_string(),
            });
        }
        self.state.get(&(group, slot_addr)).cloned().ok_or_else(|| {
            Td3Error::FormatError(format!(
                "no mock state for G{}P{}",
                group + 1,
                slot_addr + 1
            ))
        })
    }

    fn upload(&mut self, group: u8, slot_addr: u8, payload: &[u8]) -> Result<(), Td3Error> {
        let call_idx = self.upload_calls.len();
        self.upload_calls.push((group, slot_addr, payload.to_vec()));
        if Some(call_idx) == self.upload_fail_at {
            return Err(Td3Error::Timeout {
                operation: "mock upload".to_string(),
            });
        }
        self.state.insert((group, slot_addr), payload.to_vec());
        Ok(())
    }
}

/// Scripted prompt - returns pre-baked yes/no answers in order. Fails the
/// test if called more times than answers provided.
struct ScriptedPrompt {
    answers: Vec<bool>,
    idx: usize,
}

impl ScriptedPrompt {
    fn yes() -> Self {
        Self {
            answers: vec![true],
            idx: 0,
        }
    }
    fn no() -> Self {
        Self {
            answers: vec![false],
            idx: 0,
        }
    }
}

impl UserPrompt for ScriptedPrompt {
    fn confirm(&mut self, _prompt_text: &str) -> Result<bool, Td3Error> {
        let ans = self
            .answers
            .get(self.idx)
            .copied()
            .unwrap_or_else(|| panic!("ScriptedPrompt ran out of answers"));
        self.idx += 1;
        Ok(ans)
    }
}

// ---------------------------------------------------------------------------
// Bank fixtures
// ---------------------------------------------------------------------------

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
}

fn load_golden_bank(name: &str) -> Bank {
    let bytes = fs::read(golden_path(name)).unwrap();
    sqs::parse_bank(&bytes).unwrap()
}

/// A real silent payload lifted from the factory-empty bank - passes
/// `sysex_to_pattern` validation AND returns true from `is_silent`.
fn silent_payload() -> Vec<u8> {
    let bank = load_golden_bank("20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs");
    bank.records[0].payload.clone()
}

/// A real non-silent payload lifted from the golden bank.
fn non_silent_payload() -> Vec<u8> {
    let bank = load_golden_bank("ALL TD-3 PATTERNS.sqs");
    bank.records[8].payload.clone()
}

/// Differentiate `p` by overwriting the marker bytes. Marker is an origin tag
/// (see `project_td3_marker_byte_semantics`); changing it makes diff() report
/// the slot as differing without breaking `sysex_to_pattern` validation.
fn with_marker(mut p: Vec<u8>, marker: [u8; 2]) -> Vec<u8> {
    p[0] = marker[0];
    p[1] = marker[1];
    p
}

/// Build a bank where every slot has `payload`.
fn uniform_bank(payload: Vec<u8>) -> Bank {
    let records: Vec<BankRecord> = (0..RECORD_COUNT)
        .map(|idx| BankRecord {
            group: (idx / 16) as u8,
            slot_addr: (idx % 16) as u8,
            payload: payload.clone(),
        })
        .collect();
    let records_arr: [BankRecord; RECORD_COUNT] = records.try_into().unwrap();
    Bank {
        product_bytes: sqs::PRODUCT_UTF16BE.to_vec(),
        version_bytes: sqs::VERSION_UTF16BE.to_vec(),
        records: records_arr,
    }
}

/// Write a bank to a scratch `.sqs` file and return the path.
fn write_bank_to(path: &std::path::Path, bank: &Bank) {
    fs::write(path, serialize_bank(bank).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// Happy path: full bank upload
// ---------------------------------------------------------------------------

#[test]
fn import_full_bank_writes_every_differing_slot_and_creates_backup() {
    let scratch = scratch_dir("import_full_happy");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Source: all non-silent, marker byte 0xAA.
    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);

    // Device: all non-silent, marker byte 0xBB (differs byte-for-byte).
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt).expect("happy path must succeed");

    // Backup must be on disk with the content-addressed filename suffix.
    assert!(report.backup.path.is_file(), "backup zip missing");
    let fname = report
        .backup
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(fname.starts_with("bank_preimport_backup_"));
    assert!(fname.ends_with(".zip"));
    // 64-hex SHA-256 recorded in report.
    assert_eq!(report.backup.sha256_hex.len(), 64);

    // All 64 device reads were performed for the backup.
    assert_eq!(device.download_calls.len(), 64);

    // All 64 slots were written to the device (source differs from device on every slot).
    assert_eq!(device.upload_calls.len(), 64);
    assert_eq!(report.writes_completed, 64);

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// User says "N" (or EOF) → abort before any write
// ---------------------------------------------------------------------------

#[test]
fn import_aborts_on_user_no_and_leaves_device_untouched() {
    let scratch = scratch_dir("import_user_no");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::no();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let err = import_bank(&opts, &mut device, &mut prompt)
        .expect_err("user 'N' must abort with BankImportAborted");
    assert!(matches!(err, Td3Error::BankImportAborted), "got: {:?}", err);

    // Invariant: NOT A SINGLE upload happened.
    assert!(device.upload_calls.is_empty(), "abort must skip all writes");

    // Invariant: backup is already on disk - recoverable even though the user bailed.
    let zips: Vec<_> = fs::read_dir(&backup_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".zip"))
        .collect();
    assert_eq!(zips.len(), 1, "exactly one backup zip should remain");

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// --partial filter narrows writes but backup still covers all 64
// ---------------------------------------------------------------------------

#[test]
fn import_partial_only_writes_targets_but_backs_up_full_bank() {
    let scratch = scratch_dir("import_partial");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::yes();

    let targets: Vec<BankAddress> = parse_partial("1-1A,2-3B,4-8A").unwrap();

    let opts = ImportOptions {
        source: src_path,
        partial: Some(targets.clone()),
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt).expect("partial must succeed");

    // Backup always covers full bank: 64 downloads.
    assert_eq!(device.download_calls.len(), 64);

    // Writes: exactly the 3 target addresses, in that order.
    assert_eq!(device.upload_calls.len(), 3);
    assert_eq!(report.writes_completed, 3);
    let write_addrs: Vec<(u8, u8)> = device
        .upload_calls
        .iter()
        .map(|(g, s, _)| (*g, *s))
        .collect();
    let expected: Vec<(u8, u8)> = targets.iter().map(|a| (a.group, a.slot_addr)).collect();
    assert_eq!(write_addrs, expected);

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Silent source patterns are skipped by default
// ---------------------------------------------------------------------------

#[test]
fn import_skips_silent_source_patterns_by_default() {
    let scratch = scratch_dir("import_silent_skip");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Build a bank where half the slots are silent and half are not.
    let loud = with_marker(non_silent_payload(), [0xAA, 0xAA]);
    let quiet = silent_payload();
    let records: Vec<BankRecord> = (0..RECORD_COUNT)
        .map(|idx| BankRecord {
            group: (idx / 16) as u8,
            slot_addr: (idx % 16) as u8,
            payload: if idx % 2 == 0 {
                loud.clone()
            } else {
                quiet.clone()
            },
        })
        .collect();
    let records_arr: [BankRecord; RECORD_COUNT] = records.try_into().unwrap();
    let source = Bank {
        product_bytes: sqs::PRODUCT_UTF16BE.to_vec(),
        version_bytes: sqs::VERSION_UTF16BE.to_vec(),
        records: records_arr,
    };
    write_bank_to(&src_path, &source);

    // Device: every slot is different (non-silent with a different marker).
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt).expect("silent-skip must succeed");

    // Only the 32 non-silent slots should have been written.
    assert_eq!(device.upload_calls.len(), 32);
    assert_eq!(report.writes_completed, 32);

    rmrf(&scratch);
}

#[test]
fn include_silent_flag_force_writes_silent_slots() {
    let scratch = scratch_dir("import_include_silent");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Source: all slots silent.
    let source = uniform_bank(silent_payload());
    write_bank_to(&src_path, &source);
    // Device: all slots non-silent (differ byte-for-byte from source).
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: true,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt)
        .expect("include_silent must allow silent writes");

    assert_eq!(device.upload_calls.len(), 64);
    assert_eq!(report.writes_completed, 64);

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// No-op: source byte-identical to device → prompt NOT shown, nothing written
// ---------------------------------------------------------------------------

#[test]
fn import_with_identical_source_and_device_writes_nothing() {
    let scratch = scratch_dir("import_noop");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let bank = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &bank);
    let mut device = MockDevice::new(&bank);
    // Prompt with zero answers - the orchestrator must NOT call confirm() on a no-op.
    let mut prompt = ScriptedPrompt {
        answers: vec![],
        idx: 0,
    };

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt).expect("noop must succeed");
    assert_eq!(report.writes_completed, 0);
    assert!(device.upload_calls.is_empty());

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Device read failure aborts BEFORE any backup is written
// ---------------------------------------------------------------------------

#[test]
fn download_failure_aborts_before_backup_or_write() {
    let scratch = scratch_dir("import_dl_fail");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    // Fail the 20th download to simulate a MIDI timeout mid-read.
    let mut device = MockDevice::new(&device_bank).with_download_failure(20);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let err =
        import_bank(&opts, &mut device, &mut prompt).expect_err("download failure must abort");
    assert!(matches!(err, Td3Error::Timeout { .. }), "got: {:?}", err);

    // Invariant: no uploads happened.
    assert!(device.upload_calls.is_empty());

    // Invariant: no .zip exists (only possibly a .tmp that errored out
    // earlier would exist - but even that is avoided because backup
    // writing only runs after the device read completes).
    let zips: Vec<_> = fs::read_dir(&backup_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".zip"))
        .collect();
    assert!(
        zips.is_empty(),
        "no backup should exist on download failure"
    );

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Upload failure mid-loop: backup survives, writes_completed reflects partial progress
// ---------------------------------------------------------------------------

#[test]
fn upload_failure_midway_preserves_backup_and_surfaces_error() {
    let scratch = scratch_dir("import_ul_fail");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    // Fail the 5th upload.
    let mut device = MockDevice::new(&device_bank).with_upload_failure(5);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let err =
        import_bank(&opts, &mut device, &mut prompt).expect_err("upload failure must surface");
    assert!(matches!(err, Td3Error::Timeout { .. }), "got: {:?}", err);

    // Invariant: backup zip is on disk before uploads begin.
    let zips: Vec<_> = fs::read_dir(&backup_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".zip"))
        .collect();
    assert_eq!(zips.len(), 1, "backup zip must survive an upload failure");

    // Five uploads were attempted (0..4 succeeded, 5 failed).
    assert_eq!(device.upload_calls.len(), 6);

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Backup-dir handling: auto-create when missing; fail fast when the path
// points to a regular file (still before any device read).
// ---------------------------------------------------------------------------

#[test]
fn missing_backup_dir_is_created_then_import_proceeds() {
    let scratch = scratch_dir("import_auto_create_backup_dir");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("nested").join("backups");
    assert!(
        !backup_dir.exists(),
        "precondition: backup dir must not exist"
    );

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt)
        .expect("import must succeed after auto-create");
    assert!(backup_dir.is_dir(), "backup dir should have been created");
    assert!(report.backup.path.starts_with(&backup_dir));

    rmrf(&scratch);
}

#[test]
fn backup_dir_pointing_to_a_file_fails_before_device_read() {
    let scratch = scratch_dir("import_backup_dir_is_file");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("not_a_dir");
    std::fs::write(&backup_dir, b"oops").unwrap();

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    let mut device = MockDevice::new(&device_bank);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir,
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let err = import_bank(&opts, &mut device, &mut prompt).expect_err("file-as-dir must fail");
    assert!(
        matches!(err, Td3Error::BankBackupFailed(_)),
        "got: {:?}",
        err
    );
    assert!(
        device.download_calls.is_empty(),
        "device read must not run on bad backup-dir"
    );
    assert!(device.upload_calls.is_empty());

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// End-to-end interrupt recovery: the backup zip remains byte-exact and
// recoverable even if the upload loop is interrupted mid-run.
// ---------------------------------------------------------------------------

#[test]
fn mid_loop_interrupt_leaves_recoverable_backup_zip() {
    // Simulates a process death mid-upload (equivalent to Ctrl-C or OS kill
    // between writes). The safety promise - "NO PATTERN MUST BE LOST" - holds
    // iff the zip on disk after the crash can be parsed and contains the
    // pre-import device state byte-for-byte, so a subsequent `import-bank` of
    // that zip's `bank.sqs` restores the device exactly.
    let scratch = scratch_dir("import_interrupt_recover");
    let src_path = scratch.join("source.sqs");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let source = uniform_bank(with_marker(non_silent_payload(), [0xAA, 0xAA]));
    write_bank_to(&src_path, &source);
    let device_bank = uniform_bank(with_marker(non_silent_payload(), [0xBB, 0xBB]));
    // Simulate process interrupt after the 3rd upload attempt.
    let mut device = MockDevice::new(&device_bank).with_upload_failure(3);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: src_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let err = import_bank(&opts, &mut device, &mut prompt)
        .expect_err("simulated interrupt must surface an error");
    assert!(matches!(err, Td3Error::Timeout { .. }), "got: {:?}", err);

    // Locate the backup zip.
    let zips: Vec<PathBuf> = fs::read_dir(&backup_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "zip").unwrap_or(false))
        .collect();
    assert_eq!(
        zips.len(),
        1,
        "exactly one backup zip on disk after interrupt"
    );

    // Filename suffix must match the first 16 hex chars of the on-disk SHA-256.
    let fname = zips[0].file_name().unwrap().to_string_lossy().into_owned();
    let stem = fname.trim_end_matches(".zip");
    let hash16 = stem.rsplit('-').next().expect("hash suffix");
    let on_disk_bytes = fs::read(&zips[0]).unwrap();
    let on_disk_hex: String = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&on_disk_bytes);
        h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    };
    assert!(
        on_disk_hex.starts_with(hash16),
        "filename hash suffix {} must match on-disk SHA-256 {}",
        hash16,
        on_disk_hex
    );

    // Open the zip, pull bank.sqs out, parse it, and verify it is byte-for-byte
    // the PRE-import device state (the state a recovery import would restore).
    let zip_file = fs::File::open(&zips[0]).unwrap();
    let mut archive = zip::ZipArchive::new(zip_file).expect("zip must be a valid archive");
    let mut bank_bytes: Vec<u8> = Vec::new();
    {
        let mut bank_entry = archive
            .by_name("bank.sqs")
            .expect("bank.sqs missing from zip");
        std::io::copy(&mut bank_entry, &mut bank_bytes).unwrap();
    }
    let recovered = sqs::parse_bank(&bank_bytes).expect("bank.sqs inside zip must parse");
    for (i, rec) in recovered.records.iter().enumerate() {
        assert_eq!(
            rec.payload, device_bank.records[i].payload,
            "recovered slot {} must match pre-import device state",
            i
        );
    }

    // Three uploads were attempted (0..2 succeeded, 3 failed). Device state is
    // now a mix of source + original; the backup zip is the user's escape hatch.
    assert_eq!(device.upload_calls.len(), 4);

    rmrf(&scratch);
}

// ---------------------------------------------------------------------------
// Golden-file source (sanity: the full plumbing works on a real `.sqs` too)
// ---------------------------------------------------------------------------

#[test]
fn import_golden_source_against_empty_device_bank() {
    let scratch = scratch_dir("import_golden");
    let backup_dir = scratch.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let source_path = golden_path("ALL TD-3 PATTERNS.sqs");

    // Device is factory-empty.
    let empty_bank = load_golden_bank("20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs");
    let mut device = MockDevice::new(&empty_bank);
    let mut prompt = ScriptedPrompt::yes();

    let opts = ImportOptions {
        source: source_path,
        partial: None,
        include_silent: false,
        backup_dir: backup_dir.clone(),
        midi_opts: crate::formats::mid::MidiExportOptions::default(),
    };

    let report = import_bank(&opts, &mut device, &mut prompt).expect("golden import must succeed");

    // Full 64 slots downloaded.
    assert_eq!(device.download_calls.len(), 64);
    // At least one write happened (the golden bank has many non-silent patterns).
    assert!(report.writes_completed > 0);
    // Backup exists.
    assert!(report.backup.path.is_file());

    rmrf(&scratch);
}
