use super::schema_migrations::apply_schema_migrations;
use super::*;

mod connection;
mod query;
mod read;
mod schema;
mod write;

#[allow(unused_imports)]
pub(super) use connection::*;
#[allow(unused_imports)]
pub(super) use query::*;
#[allow(unused_imports)]
pub(super) use read::*;
#[allow(unused_imports)]
pub(super) use schema::*;
#[allow(unused_imports)]
pub(super) use write::*;

fn opt_text(value: &Option<String>) -> Value {
    match value {
        Some(s) => Value::Text(s.clone()),
        None => Value::Null,
    }
}

fn source_kind_text(kind: super::super::model::SourceKind) -> String {
    use super::super::model::SourceKind;
    match kind {
        SourceKind::File => "file",
        SourceKind::SnapshotSlot => "snapshotslot",
        SourceKind::Generated => "generated",
        SourceKind::Curated => "curated",
    }
    .to_string()
}

fn duplicate_status_text(status: super::super::model::DuplicateStatus) -> String {
    use super::super::model::DuplicateStatus;
    match status {
        DuplicateStatus::Unique => "unique",
        DuplicateStatus::ExactDuplicate => "exactduplicate",
        DuplicateStatus::NearDuplicate => "nearduplicate",
        DuplicateStatus::Unknown => "unknown",
    }
    .to_string()
}

fn analysis_status_text(status: super::super::model::AnalysisStatus) -> String {
    use super::super::model::AnalysisStatus;
    match status {
        AnalysisStatus::Unknown => "unknown",
        AnalysisStatus::Pending => "pending",
        AnalysisStatus::Ready => "ready",
        AnalysisStatus::NeedsReview => "needsreview",
        AnalysisStatus::Failed => "failed",
    }
    .to_string()
}

/// Load an existing catalog from SQLite. If the database is missing or empty,
/// return a fresh `LibraryData`. When the target database is empty and a
/// sibling legacy JSON catalog exists, import it automatically.
pub fn load(path: &Path) -> Result<LibraryData, Td3Error> {
    let path = crate::path_safety::require_safe_user_path(path)?;
    ensure_parent_dir(&path)?;
    let legacy_path = if !path.exists() {
        legacy_json_path(&path).filter(|p| p.exists())
    } else {
        None
    };

    let conn = open_connection(&path)?;
    init_schema(&conn)?;
    apply_schema_migrations(&conn)?;

    if db_is_empty(&conn)? {
        if let Some(legacy) = legacy_path {
            let data = load_legacy_json(&legacy)?;
            save_data(&conn, &data)?;
            return Ok(data);
        }
        return Ok(LibraryData::default());
    }

    load_data(&conn)
}

/// Persist the full catalog to SQLite inside one transaction.
pub fn save(path: &Path, data: &LibraryData) -> Result<(), Td3Error> {
    ensure_parent_dir(path)?;
    let conn = open_connection(path)?;
    init_schema(&conn)?;
    apply_schema_migrations(&conn)?;
    save_data(&conn, data)
}
