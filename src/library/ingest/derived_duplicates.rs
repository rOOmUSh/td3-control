use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::Td3Error;
use crate::library::model::{FileIndexEntry, FileIngestStatus};
use crate::library::scanner;
use crate::library::store::{self, LibraryStore};

use super::helpers::{sha256_hex, truncate_err};
use super::import_order::{
    import_priority_for_filename, logical_pattern_name, lower_filename, parent_key,
};
use super::IngestOutcome;

const DERIVED_DUPLICATE_MTIME_WINDOW: Duration = Duration::from_secs(3);

pub fn native_truth_for_derived_path(path: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    let filename = lower_filename(path);
    if !is_derived_filename(&filename) {
        return None;
    }

    let directory = parent_key(path);
    let logical_name = logical_pattern_name(&filename);
    let derived_modified = modified_time(path)?;

    candidates
        .iter()
        .filter(|candidate| {
            let candidate_name = lower_filename(candidate);
            is_native_truth_filename(&candidate_name)
                && parent_key(candidate) == directory
                && logical_pattern_name(&candidate_name) == logical_name
                && modified_times_match(derived_modified, candidate)
        })
        .min_by_key(|candidate| import_priority_for_filename(&lower_filename(candidate)))
        .cloned()
}

pub fn record_derived_duplicate_path(
    store: &LibraryStore,
    path: &Path,
    batch_id: &str,
    duplicate_of: &str,
) -> Result<IngestOutcome, Td3Error> {
    let entry = build_derived_duplicate_entry(path, batch_id, duplicate_of);
    store.append_file_index_entry(entry.clone())?;
    Ok(IngestOutcome { entry })
}

fn is_derived_filename(lower_name: &str) -> bool {
    lower_name.ends_with(".pat") || lower_name.ends_with(".mid")
}

fn is_native_truth_filename(lower_name: &str) -> bool {
    lower_name.ends_with(".seq")
        || lower_name.ends_with(".syx")
        || lower_name.ends_with(".steps.txt")
}

fn modified_times_match(derived_modified: SystemTime, truth_path: &Path) -> bool {
    let Some(truth_modified) = modified_time(truth_path) else {
        return false;
    };
    system_time_delta(derived_modified, truth_modified) <= DERIVED_DUPLICATE_MTIME_WINDOW
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn system_time_delta(a: SystemTime, b: SystemTime) -> Duration {
    match a.duration_since(b) {
        Ok(delta) => delta,
        Err(err) => err.duration(),
    }
}

fn build_derived_duplicate_entry(
    path: &Path,
    batch_id: &str,
    duplicate_of: &str,
) -> FileIndexEntry {
    let path_str = path.to_string_lossy().to_string();
    let filename = lower_filename(path);
    let (format_name, status) = scanner::classify(&filename);
    let mut entry = FileIndexEntry {
        path: path_str,
        size: 0,
        hash_sha256: None,
        discovered_at: store::now_iso(),
        format: format_name,
        status,
        error: None,
        batch_id: Some(batch_id.to_string()),
        duplicate_of: Some(duplicate_of.to_string()),
        item_id: None,
    };

    if entry.status == FileIngestStatus::Unsupported {
        entry.duplicate_of = None;
        return entry;
    }

    let safe_path = match crate::path_safety::require_safe_user_path(path) {
        Ok(path) => path,
        Err(err) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&err.to_string()));
            entry.duplicate_of = None;
            return entry;
        }
    };

    match std::fs::metadata(&safe_path) {
        Ok(meta) if meta.is_file() => entry.size = meta.len(),
        Ok(_) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some("not a regular file".to_string());
            entry.duplicate_of = None;
            return entry;
        }
        Err(err) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("metadata: {}", err)));
            entry.duplicate_of = None;
            return entry;
        }
    }

    match std::fs::read(&safe_path) {
        Ok(bytes) => {
            entry.hash_sha256 = Some(sha256_hex(&bytes));
            entry.status = FileIngestStatus::DuplicateSkipped;
        }
        Err(err) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("read: {}", err)));
            entry.duplicate_of = None;
        }
    }

    entry
}
