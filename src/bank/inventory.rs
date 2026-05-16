//! Backup-zip inventory scanner.
//!
//! The bank-import and UI workflows drop `.zip` backups into a shared
//! directory (see `backup.rs`). This module walks that directory and parses
//! the filename convention into structured metadata so the Bank Management
//! UI can present existing backups as "Snapshot" entries without cracking
//! the zip open.
//!
//! Filename convention (written by `write_backup_zip`):
//!
//! - `bank_preimport_backup_<YYYY-MM-DD_HH-MM-SS>-<short-hash>.zip`
//! - `bank_ui_backup_<YYYY-MM-DD_HH-MM-SS>-<short-hash>.zip`
//!
//! Optional PID suffix when a same-second collision forces it:
//!
//! - `bank_preimport_backup_<TS>-<PID>-<short-hash>.zip`
//!
//! This module is intentionally forgiving on the timestamp field - it only
//! checks that the `bank_<kind>_backup_` prefix and the `-<hash>.zip` tail
//! are present, and returns the remainder as an opaque `timestamp` string.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Td3Error;

use super::backup::BackupKind;

/// One scanned backup `.zip` file, with filename fields parsed out.
///
/// Several fields (`kind`, `short_hash`, `size_bytes`) are consumed only by
/// the HTTP layer and JS UI, so the release-build dead-code lint
/// understandably flags them - we silence it here rather than sprinkle
/// `#[allow]` annotations at every field.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BackupInventoryEntry {
    pub path: PathBuf,
    pub filename: String,
    pub kind: BackupKind,
    /// Opaque timestamp stem, e.g. `1970-01-01_11-11-11`. May include a
    /// trailing `-<PID>` suffix when the original backup run collided on the
    /// second. Older compact backup names are still accepted by the parser.
    pub timestamp: String,
    pub short_hash: String,
    pub size_bytes: u64,
}

/// Scan `dir` for backup `.zip` files and return one entry per recognised
/// filename. Unrecognised files are silently skipped - the directory often
/// contains unrelated user archives alongside our backups.
///
/// Errors only on directory access failures; malformed filenames are not an
/// error.
pub fn scan_backup_dir(dir: &Path) -> Result<Vec<BackupInventoryEntry>, Td3Error> {
    let dir = crate::path_safety::require_safe_user_path(dir)?;
    if !dir.exists() {
        return Err(Td3Error::Other(format!(
            "backup inventory: path does not exist: {}",
            dir.display()
        )));
    }
    if !dir.is_dir() {
        return Err(Td3Error::Other(format!(
            "backup inventory: not a directory: {}",
            dir.display()
        )));
    }

    let reader = fs::read_dir(&dir).map_err(|e| {
        Td3Error::Other(format!(
            "backup inventory: read_dir {}: {}",
            dir.display(),
            e
        ))
    })?;

    let mut out: Vec<BackupInventoryEntry> = Vec::new();
    for entry in reader.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let parsed = match parse_filename(&filename) {
            Some(p) => p,
            None => continue,
        };
        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);

        out.push(BackupInventoryEntry {
            path: path.clone(),
            filename,
            kind: parsed.kind,
            timestamp: parsed.timestamp,
            short_hash: parsed.short_hash,
            size_bytes: size,
        });
    }

    // Stable ordering by timestamp (lexicographic works because the
    // timestamp format is zero-padded fixed-width). Fall back to filename
    // for the tie-breaker so tests are deterministic.
    out.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(out)
}

struct Parsed {
    kind: BackupKind,
    timestamp: String,
    short_hash: String,
}

/// Strict filename parse. Returns `None` when `name` doesn't match either
/// the `preimport` or `ui` prefix.
fn parse_filename(name: &str) -> Option<Parsed> {
    let stem = name.strip_suffix(".zip")?;

    // Detect kind by prefix.
    let (kind, rest) = if let Some(r) = stem.strip_prefix("bank_preimport_backup_") {
        (BackupKind::PreImport, r)
    } else if let Some(r) = stem.strip_prefix("bank_ui_backup_") {
        (BackupKind::PreUi, r)
    } else {
        return None;
    };

    // `rest` is `<timestamp>[-<pid>]-<short-hash>`.
    // Split on the last `-` to isolate `<short-hash>`.
    let dash_idx = rest.rfind('-')?;
    let (timestamp, hash_with_dash) = rest.split_at(dash_idx);
    let short_hash = &hash_with_dash[1..]; // skip the dash

    if timestamp.is_empty() || short_hash.is_empty() {
        return None;
    }
    if !short_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(Parsed {
        kind,
        timestamp: timestamp.to_string(),
        short_hash: short_hash.to_string(),
    })
}

// Tests for this module live in `src/tests/bank_inventory_tests.rs`
