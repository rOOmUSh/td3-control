//! Filesystem scanner.
//!
//! Walks a folder, classifies files by extension, and hands back
//! `FileIndexEntry` rows with `status = Discovered`. This module does not
//! parse file contents; each entry can later be promoted to `Parsed`,
//! `Imported`, or `Failed` by the ingest pipeline.
//!
//! Supported extensions (matches `formats::detect_format` plus raw text):
//!   `.seq`, `.sqs`, `.syx`, `.mid`, `.toml`, `.json`, `.steps.txt`, `.pat`,
//!   `.rbs`. Everything else is returned with `status = Unsupported`.
//!
//! `scan_folder` is retained as a thin public surface for tests and tooling
//! that only want an extension-classified directory walk without parsing.
#![allow(dead_code)]

use std::fs;
use std::path::Path;

use crate::error::Td3Error;

use super::model::{FileIndexEntry, FileIngestStatus};
use super::store;

/// Walk `root` and return one `FileIndexEntry` per encountered file. When
/// `recursive` is false, only immediate children are scanned.
pub fn scan_folder(root: &Path, recursive: bool) -> Result<Vec<FileIndexEntry>, Td3Error> {
    let root = crate::path_safety::require_safe_user_path(root)?;
    if !root.exists() {
        return Err(Td3Error::Other(format!(
            "scanner: path does not exist: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(Td3Error::Other(format!(
            "scanner: not a directory: {}",
            root.display()
        )));
    }
    let mut out = Vec::new();
    walk(&root, recursive, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<FileIndexEntry>) -> Result<(), Td3Error> {
    let dir = crate::path_safety::require_safe_user_path(dir)?;
    let reader = fs::read_dir(&dir)
        .map_err(|e| Td3Error::Other(format!("scanner: read_dir {}: {}", dir.display(), e)))?;
    for entry in reader.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            if recursive {
                let sub = crate::path_safety::require_safe_user_path(&path)?;
                walk(&sub, recursive, out)?;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let (format, status) = classify(&name);

        out.push(FileIndexEntry {
            path: path.to_string_lossy().to_string(),
            size,
            hash_sha256: None,
            discovered_at: store::now_iso(),
            format,
            status,
            error: None,
            batch_id: None,
            duplicate_of: None,
            item_id: None,
        });
    }
    Ok(())
}

/// Return `(format_name, status)` for a lowercased filename. Unrecognized
/// extensions yield `(None, Unsupported)` - the UI can still show them in
/// the import batch for user visibility.
pub fn classify(lower_name: &str) -> (Option<String>, FileIngestStatus) {
    let fmt = if lower_name.ends_with(".steps.txt") {
        Some("steps")
    } else if lower_name.ends_with(".syx") {
        Some("syx")
    } else if lower_name.ends_with(".toml") {
        Some("toml")
    } else if lower_name.ends_with(".json") {
        Some("json")
    } else if lower_name.ends_with(".mid") {
        Some("mid")
    } else if lower_name.ends_with(".seq") {
        Some("seq")
    } else if lower_name.ends_with(".sqs") {
        Some("sqs")
    } else if lower_name.ends_with(".pat") {
        Some("pat")
    } else if lower_name.ends_with(".rbs") {
        Some("rbs")
    } else {
        None
    };
    match fmt {
        Some(f) => (Some(f.to_string()), FileIngestStatus::Discovered),
        None => (None, FileIngestStatus::Unsupported),
    }
}
