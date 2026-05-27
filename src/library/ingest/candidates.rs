use std::path::Path;

use crate::error::Td3Error;

use super::import_order::sort_import_paths;

const MAX_SCAN_JSON_BYTES: u64 = 2550;
const MAX_SCAN_TOML_BYTES: u64 = 1900;

pub fn list_candidate_files(
    root: &Path,
    recursive: bool,
) -> Result<Vec<std::path::PathBuf>, Td3Error> {
    if !root.exists() {
        return Err(Td3Error::Other(format!(
            "ingest: path does not exist: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(Td3Error::Other(format!(
            "ingest: not a directory: {}",
            root.display()
        )));
    }
    let mut out = Vec::new();
    let mut stats = WalkStats::default();
    walk(root, recursive, &mut out, &mut stats)?;
    eprintln!(
        "[scan] done: matched {} / scanned {} (across {} dir(s))",
        stats.matched, stats.scanned, stats.dirs
    );
    sort_import_paths(&mut out);
    Ok(out)
}

#[derive(Default)]
struct WalkStats {
    matched: usize,
    scanned: usize,
    dirs: usize,
}

fn walk(
    dir: &Path,
    recursive: bool,
    out: &mut Vec<std::path::PathBuf>,
    stats: &mut WalkStats,
) -> Result<(), Td3Error> {
    stats.dirs += 1;
    eprintln!("[scan] folder: {}", dir.display());
    let reader = std::fs::read_dir(dir)
        .map_err(|e| Td3Error::Other(format!("ingest: read_dir {}: {}", dir.display(), e)))?;

    let mut local_matched = 0usize;
    let mut local_scanned = 0usize;

    for entry in reader.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            if recursive {
                walk(&path, recursive, out, stats)?;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        local_scanned += 1;
        stats.scanned += 1;

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_candidate_filename(name) && is_within_scan_size_limit(&path, name) {
            local_matched += 1;
            stats.matched += 1;
            out.push(path);
        }
    }

    eprintln!(
        "[scan]   {}: matched {} / scanned {}  (total {}/{})",
        dir.display(),
        local_matched,
        local_scanned,
        stats.matched,
        stats.scanned,
    );
    Ok(())
}

/// Return true iff `name` looks like a file our ingest pipeline is willing
/// to open. See `list_candidate_files` for the accepted-shape list.
pub fn is_candidate_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();

    if lower.ends_with(".seq")
        || lower.ends_with(".sqs")
        || lower.ends_with(".syx")
        || lower.ends_with(".mid")
        || lower.ends_with(".pat")
        || lower.ends_with(".rbs")
    {
        return true;
    }

    // Only the specific `.steps.txt` suffix; never a plain `.txt`.
    if lower.ends_with(".steps.txt") {
        return true;
    }

    // JSON/TOML only when the filename embeds a TD-3 slot address.
    if lower.ends_with(".json") || lower.ends_with(".toml") {
        return has_slot_marker(&lower);
    }

    false
}

fn is_within_scan_size_limit(path: &Path, name: &str) -> bool {
    let Some(limit) = scan_size_limit(name) else {
        return true;
    };
    match std::fs::metadata(path) {
        Ok(meta) => meta.len() <= limit,
        Err(_) => false,
    }
}

fn scan_size_limit(name: &str) -> Option<u64> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".json") {
        Some(MAX_SCAN_JSON_BYTES)
    } else if lower.ends_with(".toml") {
        Some(MAX_SCAN_TOML_BYTES)
    } else {
        None
    }
}

/// Look for `G<digit>P<digit>[ab]` - with an optional single `-` between
/// the group and pattern halves - anywhere inside `lower` (already
/// lowercased). Mirrors the regexes `G\dP\d[AB]` and `G\d-P\d[AB]`, which
/// are the two filename shapes this project's own exporters produce.
fn has_slot_marker(lower: &str) -> bool {
    let b = lower.as_bytes();
    let n = b.len();
    if n < 5 {
        return false;
    }
    let mut i = 0;
    while i + 5 <= n {
        if b[i] != b'g' || !b[i + 1].is_ascii_digit() {
            i += 1;
            continue;
        }
        if b[i + 2] == b'p' && b[i + 3].is_ascii_digit() && (b[i + 4] == b'a' || b[i + 4] == b'b') {
            return true;
        }
        if i + 6 <= n
            && b[i + 2] == b'-'
            && b[i + 3] == b'p'
            && b[i + 4].is_ascii_digit()
            && (b[i + 5] == b'a' || b[i + 5] == b'b')
        {
            return true;
        }
        i += 1;
    }
    false
}
