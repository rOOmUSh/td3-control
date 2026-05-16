//! Unit tests for `bank::inventory::scan_backup_dir`.
//!
//! Covers:
//! - Filename parsing for both PreImport and PreUi variants.
//! - PID-suffix form.
//! - Rejection of unrelated files + malformed filenames.
//! - Ordering is deterministic (lexicographic by timestamp).
//! - Size reflects on-disk bytes.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bank::backup::BackupKind;
use crate::bank::inventory::scan_backup_dir;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!(
        "td3-inv-test-{}-{}-{}-{}",
        tag,
        pid,
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[test]
fn scan_returns_error_when_dir_missing() {
    let dir = std::env::temp_dir().join("td3-inv-does-not-exist-xxxxx");
    let _ = fs::remove_dir_all(&dir);
    let err = scan_backup_dir(&dir).unwrap_err();
    assert!(err.to_string().contains("does not exist"));
}

#[test]
fn scan_recognises_preimport_and_preui_names() {
    let dir = unique_dir("both-kinds");
    // Two well-formed backup filenames, different kinds.
    fs::write(
        dir.join("bank_preimport_backup_1970-01-01_11-11-11-abcdef0123456789.zip"),
        b"pk zipped",
    )
    .unwrap();
    fs::write(
        dir.join("bank_ui_backup_1970-01-01_11-12-22-fedcba9876543210.zip"),
        b"pk zipped x",
    )
    .unwrap();

    let entries = scan_backup_dir(&dir).unwrap();
    assert_eq!(entries.len(), 2);

    // Sort order is by timestamp lexicographic, so the preimport one (11-11-11)
    // comes first.
    assert_eq!(entries[0].kind, BackupKind::PreImport);
    assert_eq!(entries[0].timestamp, "1970-01-01_11-11-11");
    assert_eq!(entries[0].short_hash, "abcdef0123456789");
    assert_eq!(
        entries[0].filename,
        "bank_preimport_backup_1970-01-01_11-11-11-abcdef0123456789.zip"
    );
    assert!(entries[0].size_bytes > 0);

    assert_eq!(entries[1].kind, BackupKind::PreUi);
    assert_eq!(entries[1].timestamp, "1970-01-01_11-12-22");
    assert_eq!(entries[1].short_hash, "fedcba9876543210");
    assert_eq!(
        entries[1].filename,
        "bank_ui_backup_1970-01-01_11-12-22-fedcba9876543210.zip"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_parses_pid_suffixed_names() {
    let dir = unique_dir("pid-suffix");
    // Same-second collision -> the writer appends `-<pid>` before the hash.
    fs::write(
        dir.join("bank_preimport_backup_1970-01-01_11-11-11-99999-abcdef0123456789.zip"),
        b"pk",
    )
    .unwrap();

    let entries = scan_backup_dir(&dir).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, BackupKind::PreImport);
    // timestamp field carries the PID-suffixed stem verbatim.
    assert_eq!(entries[0].timestamp, "1970-01-01_11-11-11-99999");
    assert_eq!(entries[0].short_hash, "abcdef0123456789");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_skips_unrelated_and_malformed_files() {
    let dir = unique_dir("skip-garbage");
    fs::write(dir.join("README.md"), b"# notes").unwrap();
    fs::write(dir.join("random.zip"), b"pk").unwrap();
    fs::write(dir.join("bank_preimport_backup_.zip"), b"pk").unwrap(); // empty timestamp
    fs::write(
        dir.join("bank_preimport_backup_1970-01-01_11-11-11-NOTHEX.zip"),
        b"pk",
    )
    .unwrap(); // hash not hex
    fs::write(
        dir.join("bank_preimport_backup_1970-01-01_11-11-11-deadbeef.zip"),
        b"pk",
    )
    .unwrap(); // valid

    let entries = scan_backup_dir(&dir).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].short_hash, "deadbeef");

    let _ = fs::remove_dir_all(&dir);
}
