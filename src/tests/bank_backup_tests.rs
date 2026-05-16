// Tests for `bank::backup::write_backup_zip` - the kind-parameterized filename
// contract.
//
// The two workflow kinds (`PreImport` and `PreUi`) must produce distinct
// on-disk filenames so a user browsing their backup directory weeks later
// can tell at a glance whether a zip came from an `import-bank` auto-dump
// or from a `control` UI-session auto-dump.

use std::fs;
use std::path::PathBuf;

use crate::bank::backup::{write_backup_zip, BackupKind};
use crate::formats::mid::MidiExportOptions;
use crate::formats::sqs::{self, Bank, BankRecord, RECORD_COUNT};

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

fn load_golden_bank(name: &str) -> Bank {
    let bytes = fs::read(format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
    .unwrap();
    sqs::parse_bank(&bytes).unwrap()
}

fn sample_bank() -> Bank {
    // Borrow the factory-empty bank. It's guaranteed to parse and serialize
    // cleanly through the zip pipeline (covers all 6 per-format renders).
    let bank = load_golden_bank("20260414_111111_EMPTY_BANK_A-B_SIDES_CLEAR.sqs");
    let records: Vec<BankRecord> = bank.records.to_vec();
    let records_arr: [BankRecord; RECORD_COUNT] = records.try_into().unwrap();
    Bank {
        product_bytes: sqs::PRODUCT_UTF16BE.to_vec(),
        version_bytes: sqs::VERSION_UTF16BE.to_vec(),
        records: records_arr,
    }
}

#[test]
fn pre_import_backup_filename_uses_preimport_stem() {
    let dir = scratch_dir("backup_kind_preimport");
    let bank = sample_bank();

    let result = write_backup_zip(
        &dir,
        &bank,
        BackupKind::PreImport,
        &MidiExportOptions::default(),
    )
    .unwrap();
    let fname = result
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    assert!(
        fname.starts_with("bank_preimport_backup_"),
        "expected bank_preimport_backup_… prefix, got {}",
        fname
    );
    assert!(fname.ends_with(".zip"));
    // Must NOT be mistaken for a UI-session backup.
    assert!(
        !fname.contains("preui"),
        "PreImport backup must not contain 'preui' marker: {}",
        fname
    );
    rmrf(&dir);
}

#[test]
fn pre_ui_backup_filename_carries_ui_marker() {
    // User requirement: "additional marker in the zip file name '_ui', that
    // will differentiate between a backup of full bank upload and a backup
    // of before working with ui."
    let dir = scratch_dir("backup_kind_preui");
    let bank = sample_bank();

    let result = write_backup_zip(
        &dir,
        &bank,
        BackupKind::PreUi,
        &MidiExportOptions::default(),
    )
    .unwrap();
    let fname = result
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    assert!(
        fname.starts_with("bank_ui_backup_"),
        "expected bank_ui_backup_… prefix, got {}",
        fname
    );
    assert!(
        fname.len() >= "bank_ui_backup_1970-01-01_10-49-58-0000000000000000.zip".len(),
        "expected readable timestamp shape in {}",
        fname
    );
    let ts = &fname["bank_ui_backup_".len().."bank_ui_backup_".len() + 19];
    assert_eq!(ts.chars().nth(4), Some('-'));
    assert_eq!(ts.chars().nth(7), Some('-'));
    assert_eq!(ts.chars().nth(10), Some('_'));
    assert_eq!(ts.chars().nth(13), Some('-'));
    assert_eq!(ts.chars().nth(16), Some('-'));
    // The literal '_ui' marker must appear in the filename so a user can
    // identify it at a glance.
    assert!(
        fname.contains("_ui"),
        "filename must contain '_ui' marker: {}",
        fname
    );
    // Must NOT be mistaken for an import-bank backup.
    assert!(
        !fname.contains("preimport"),
        "PreUi backup must not contain 'preimport' stem: {}",
        fname
    );
    assert!(fname.ends_with(".zip"));
    rmrf(&dir);
}

#[test]
fn pre_import_and_pre_ui_backups_coexist_in_same_dir() {
    // A user running `import-bank` and then `control` in the same working
    // directory must NOT have one backup overwrite the other. The distinct
    // stems guarantee it.
    let dir = scratch_dir("backup_kind_coexist");
    let bank = sample_bank();

    let import_zip = write_backup_zip(
        &dir,
        &bank,
        BackupKind::PreImport,
        &MidiExportOptions::default(),
    )
    .unwrap();
    let ui_zip = write_backup_zip(
        &dir,
        &bank,
        BackupKind::PreUi,
        &MidiExportOptions::default(),
    )
    .unwrap();

    assert!(import_zip.path.is_file());
    assert!(ui_zip.path.is_file());
    assert_ne!(import_zip.path, ui_zip.path);
    rmrf(&dir);
}

#[test]
fn missing_backup_dir_is_created_automatically() {
    let bank = sample_bank();
    let parent = scratch_dir("backup_auto_create_parent");
    let target = parent.join("nested").join("backups");
    assert!(
        !target.exists(),
        "precondition: target dir must not exist yet"
    );

    let result = write_backup_zip(
        &target,
        &bank,
        BackupKind::PreUi,
        &MidiExportOptions::default(),
    )
    .unwrap();

    assert!(target.is_dir(), "backup dir should have been created");
    assert!(
        result.path.starts_with(&target),
        "backup file should land in the auto-created dir"
    );
    assert!(result.path.is_file(), "backup file should exist on disk");

    rmrf(&parent);
}

#[test]
fn backup_dir_path_is_a_file_returns_bank_backup_failed() {
    use crate::error::Td3Error;
    let bank = sample_bank();
    let dir = scratch_dir("backup_dir_is_file");
    let file_masquerading_as_dir = dir.join("not_a_dir");
    fs::write(&file_masquerading_as_dir, b"oops").unwrap();

    let err = write_backup_zip(
        &file_masquerading_as_dir,
        &bank,
        BackupKind::PreUi,
        &MidiExportOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Td3Error::BankBackupFailed(_)),
        "got: {:?}",
        err
    );

    rmrf(&dir);
}
