use super::*;

pub(in crate::library::persistence) fn open_connection(
    path: &Path,
) -> Result<Connection, Td3Error> {
    let path = crate::path_safety::require_safe_user_path(path)?;
    let conn = Connection::open(&path)
        .map_err(|e| Td3Error::Other(format!("library: open sqlite {}: {}", path.display(), e)))?;
    conn.busy_timeout(Duration::from_millis(5_000))
        .map_err(|e| Td3Error::Other(format!("library: busy_timeout {}: {}", path.display(), e)))?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        ",
    )
    .map_err(|e| Td3Error::Other(format!("library: sqlite pragmas {}: {}", path.display(), e)))?;
    Ok(conn)
}

pub(in crate::library::persistence) fn open_partial_connection(
    path: &Path,
) -> Result<Connection, Td3Error> {
    ensure_parent_dir(path)?;
    let conn = open_connection(path)?;
    init_schema(&conn)?;
    apply_schema_migrations(&conn)?;
    Ok(conn)
}

pub(in crate::library::persistence) fn ensure_parent_dir(path: &Path) -> Result<(), Td3Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                Td3Error::Other(format!(
                    "library: create_dir_all {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }
    Ok(())
}

pub(in crate::library::persistence) fn legacy_json_path(path: &Path) -> Option<PathBuf> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext == "json" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    Some(
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{}.json", stem)),
    )
}

pub(in crate::library::persistence) fn load_legacy_json(
    path: &Path,
) -> Result<LibraryData, Td3Error> {
    let path = crate::path_safety::require_safe_user_path(path)?;
    let bytes = fs::read(&path)
        .map_err(|e| Td3Error::Other(format!("library: read legacy {}: {}", path.display(), e)))?;
    if bytes.is_empty() {
        return Ok(LibraryData::default());
    }
    let data: LibraryData = serde_json::from_slice(&bytes)
        .map_err(|e| Td3Error::Other(format!("library: parse legacy {}: {}", path.display(), e)))?;
    if data.format_version != LibraryData::CURRENT_FORMAT_VERSION {
        return Err(Td3Error::Other(format!(
            "library: unsupported legacy format_version {} in {} (expected {})",
            data.format_version,
            path.display(),
            LibraryData::CURRENT_FORMAT_VERSION
        )));
    }
    Ok(data)
}
