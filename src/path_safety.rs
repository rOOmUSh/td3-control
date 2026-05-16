use std::path::{Component, Path, PathBuf};

use crate::error::Td3Error;

/// Reject any path that contains a parent-directory component (`..`). Symlink
/// escape via a literal `..` segment is the only attack we filter out at this
/// layer; callers that want stronger guarantees (e.g. confining writes to a
/// specific directory) should additionally pass the result through
/// `require_within_base`.
///
/// Returns the path unchanged when safe so callers can chain this in front of
/// existing `fs::*` calls without restructuring control flow.
pub fn require_safe_user_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, Td3Error> {
    let p = path.as_ref();
    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            return Err(Td3Error::CliError(format!(
                "path traversal not allowed: {}",
                p.display()
            )));
        }
    }
    Ok(p.to_path_buf())
}

/// Resolve `candidate` against `base`, returning an absolute path that is
/// guaranteed to live inside `base` (no `..` escapes, no absolute overrides).
///
/// `candidate` must be relative. Use this for paths that the caller intends
/// to read or write under a controlled directory (library root, backup dir).
pub fn require_within_base<B, P>(base: B, candidate: P) -> Result<PathBuf, Td3Error>
where
    B: AsRef<Path>,
    P: AsRef<Path>,
{
    let candidate = candidate.as_ref();
    if candidate.is_absolute() {
        return Err(Td3Error::CliError(format!(
            "absolute path not permitted under base: {}",
            candidate.display()
        )));
    }
    let safe = require_safe_user_path(candidate)?;
    Ok(base.as_ref().join(safe))
}
