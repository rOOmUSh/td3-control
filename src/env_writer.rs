//! Write updated values back to `TD3_CONFIG.env` without destroying the
//! file's comments or section structure.
//!
//! The algorithm is intentionally line-based: each line in the existing
//! file is either preserved verbatim or has its value replaced. Blank
//! lines and comments (`#...`) are untouched.
//!
//! Atomicity: new content is first written to `TD3_CONFIG.env.tmp`. Only
//! after that write succeeds do we rename the live file to
//! `TD3_CONFIG.env.bak` and rename the tmp file into place. If the final
//! rename fails, we best-effort restore the original from `.bak`.
//!
//! This module is deliberately separate from `app_env.rs`. Parsing
//! (load path) and rewriting (save path) have different invariants and
//! should evolve independently.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Td3Error;

/// Rewrite `path` with the provided updates applied.
///
/// - `updates` is a partial map of `{ KEY => raw_string_value }`. Values
///   are written as given; caller is expected to have already validated
///   them (typically via `env_metadata::validate_value`).
/// - On success, the original file is preserved as `{path}.bak` (single
///   generation; any previous `.bak` is overwritten).
/// - If no keys are provided, the file is left untouched.
pub fn apply_updates(path: &Path, updates: &HashMap<String, String>) -> Result<(), Td3Error> {
    if updates.is_empty() {
        return Ok(());
    }

    let path = crate::path_safety::require_safe_user_path(path)?;
    let original = fs::read_to_string(&path)?;
    let new_content = rewrite_content(&original, updates);

    let bak = backup_path(&path);
    let tmp = tmp_path(&path);

    // Stage the new content first. If this fails (disk full, permissions),
    // the original file is still intact.
    fs::write(&tmp, &new_content).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Td3Error::Io(e)
    })?;

    // Windows rename refuses to overwrite an existing destination - clear
    // any prior backup before re-using the name.
    if bak.exists() {
        if let Err(e) = fs::remove_file(&bak) {
            let _ = fs::remove_file(&tmp);
            return Err(Td3Error::Io(e));
        }
    }

    // Move the current file aside.
    if let Err(e) = fs::rename(&path, &bak) {
        let _ = fs::remove_file(&tmp);
        return Err(Td3Error::Io(e));
    }

    // Move the staged tmp into place. On failure, try to restore the backup
    // so the caller isn't left with a missing env file.
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::rename(&bak, &path);
        return Err(Td3Error::Io(e));
    }

    Ok(())
}

/// Read raw `KEY=VALUE` pairs from `content`, preserving the *file's*
/// view of each value (after surrounding double-quotes are stripped).
///
/// Unknown keys are returned as well - callers filter via the
/// `env_metadata::FIELDS` table. Blank lines and `#` comments are
/// skipped. A line with no `=` is skipped (not an error) so the Settings
/// full-state endpoint stays robust against stray lines that the loader
/// would reject; this function exists only to recover raw values, not
/// to validate the file.
pub fn read_raw_pairs(content: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(eq_pos) = trimmed.find('=') else {
            continue;
        };
        let key = trimmed[..eq_pos].trim();
        if key.is_empty() {
            continue;
        }
        let raw = trimmed[eq_pos + 1..].trim();
        let unquoted = strip_surrounding_quotes(raw);
        out.insert(key.to_owned(), unquoted.to_owned());
    }
    out
}

fn strip_surrounding_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Canonical backup path: `{path}.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".bak");
    PathBuf::from(os)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

/// Rewrite `content`, replacing `KEY=VALUE` lines for keys in `updates`.
///
/// Preserves blank lines and `#` comment lines byte-for-byte. Any keys
/// in `updates` that don't appear in the original file are appended at
/// the bottom under a `# --- runtime additions ---` footer.
pub(crate) fn rewrite_content(content: &str, updates: &HashMap<String, String>) -> String {
    let eol = detect_eol(content);
    let mut out = String::with_capacity(content.len() + 256);
    let mut matched = HashSet::<String>::new();

    for line in content.lines() {
        match try_rewrite_line(line, updates) {
            Some((rewritten, matched_key)) => {
                matched.insert(matched_key);
                out.push_str(&rewritten);
            }
            None => {
                out.push_str(line);
            }
        }
        out.push_str(eol);
    }

    // Unmatched keys get appended - kept sorted so repeated writes produce
    // deterministic output.
    let mut unmatched: Vec<(&String, &String)> = updates
        .iter()
        .filter(|(k, _)| !matched.contains(*k))
        .collect();
    unmatched.sort_by_key(|(k, _)| k.as_str());

    if !unmatched.is_empty() {
        out.push_str(eol);
        out.push_str("# --- runtime additions ---");
        out.push_str(eol);
        for (k, v) in unmatched {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push_str(eol);
        }
    }

    out
}

fn detect_eol(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn try_rewrite_line(line: &str, updates: &HashMap<String, String>) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let eq_pos = trimmed.find('=')?;
    let key = trimmed[..eq_pos].trim();
    let new_raw = updates.get(key)?;

    let leading_ws = &line[..line.len() - trimmed.len()];
    let original_value = trimmed[eq_pos + 1..].trim_start();
    let was_quoted = original_value.starts_with('"')
        && original_value.trim_end().ends_with('"')
        && original_value.trim_end().len() >= 2;

    let new_value = if was_quoted {
        format!("\"{}\"", new_raw)
    } else {
        new_raw.clone()
    };

    Some((
        format!("{}{}={}", leading_ws, key, new_value),
        key.to_owned(),
    ))
}
