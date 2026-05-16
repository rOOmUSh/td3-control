use super::*;

impl LibraryStore {
    // ------------------------------------------------------------------
    // Per-item pattern sidecar files
    // ------------------------------------------------------------------
    //
    // Store the 112-byte TD-3 pattern payload that backs each LibraryItem
    // in a sidecar file at
    // `<catalog_dir>/bank-library-patterns/<item_id>.syx`. The sidecar is
    // written during ingest (both File-source and SnapshotSlot-source). The
    // compare + duplicates endpoints read these back on demand - no cache
    // layer sits between the filesystem and those derived views.

    /// Directory where per-item pattern sidecars live.
    ///
    /// If `sidecar_dir` is absolute, it's used verbatim. If it's relative
    /// (the common case - `bank-library-patterns` by default), it resolves
    /// as a sibling of the catalog db, preserving historical layout.
    pub fn pattern_sidecar_dir(&self) -> PathBuf {
        if self.sidecar_dir.is_absolute() {
            self.sidecar_dir.clone()
        } else {
            let parent = self
                .path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            parent.join(&self.sidecar_dir)
        }
    }

    /// Resolve the on-disk sidecar path for `item_id`. The path is derived
    /// deterministically; this method does NOT check that the file exists.
    pub fn pattern_sidecar_path(&self, item_id: &str) -> PathBuf {
        // Guard against traversal: only allow filename-safe characters in
        // `item_id`. Our generated IDs are of the form `item-<timestamp>-<n>`
        // so this is defensive - malformed IDs produce a path that simply
        // doesn't exist, which `pattern_bytes_for` reports as `None`.
        let safe = item_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>();
        self.pattern_sidecar_dir().join(format!("{}.syx", safe))
    }

    /// Write a 112-byte pattern payload sidecar for `item_id`.
    ///
    /// Atomic on a per-file basis: bytes are streamed into a temp file in the
    /// same directory, flushed, then renamed over the final path. If anything
    /// fails before the rename, the temp file is removed and an existing
    /// sidecar is left intact - callers can rely on "either the new payload
    /// is fully on disk, or the old one is unchanged".
    ///
    /// Payload length is validated up front so we never cache garbage.
    pub fn write_pattern_bytes(&self, item_id: &str, payload: &[u8]) -> Result<(), Td3Error> {
        use std::io::Write;
        if payload.len() != 112 {
            return Err(Td3Error::Other(format!(
                "write_pattern_bytes: expected 112 bytes, got {}",
                payload.len()
            )));
        }
        let dir = crate::path_safety::require_safe_user_path(self.pattern_sidecar_dir())?;
        std::fs::create_dir_all(&dir).map_err(|e| {
            Td3Error::Other(format!(
                "write_pattern_bytes: mkdir {}: {}",
                dir.display(),
                e
            ))
        })?;
        let path = crate::path_safety::require_safe_user_path(self.pattern_sidecar_path(item_id))?;
        let safe = item_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_path = crate::path_safety::require_safe_user_path(
            dir.join(format!("{}.{}.{}.tmp", safe, pid, nanos)),
        )?;

        let write_result = (|| -> Result<(), Td3Error> {
            let mut f = std::fs::File::create(&tmp_path).map_err(|e| {
                Td3Error::Other(format!(
                    "write_pattern_bytes: create tmp {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
            f.write_all(payload).map_err(|e| {
                Td3Error::Other(format!(
                    "write_pattern_bytes: write tmp {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
            f.sync_all().map_err(|e| {
                Td3Error::Other(format!(
                    "write_pattern_bytes: sync tmp {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
            Ok(())
        })();

        if let Err(e) = write_result {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }

        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(Td3Error::Other(format!(
                "write_pattern_bytes: rename {} -> {}: {}",
                tmp_path.display(),
                path.display(),
                e
            )));
        }
        Ok(())
    }

    /// Read the 112-byte pattern payload sidecar for `item_id`. Returns
    /// `None` on any error or if the file is missing - callers should treat
    /// a missing sidecar as "pattern unavailable" rather than an error.
    pub fn pattern_bytes_for(&self, item_id: &str) -> Option<Vec<u8>> {
        let path =
            crate::path_safety::require_safe_user_path(self.pattern_sidecar_path(item_id)).ok()?;
        match std::fs::read(&path) {
            Ok(bytes) if bytes.len() == 112 => Some(bytes),
            _ => None,
        }
    }

    /// Remove the sidecar file for `item_id`, if any. Called from
    /// `delete_item`. Errors are swallowed: the catalog is the source of
    /// truth, an orphan sidecar is only a tiny disk leak.
    pub(super) fn remove_pattern_sidecar(&self, item_id: &str) {
        if let Ok(path) =
            crate::path_safety::require_safe_user_path(self.pattern_sidecar_path(item_id))
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}
