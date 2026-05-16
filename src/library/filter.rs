//! Filter parameters for `LibraryStore::list_items`.
//!
//! `ItemFilter` is also used as an Axum `Query<_>` extractor on the HTTP
//! side; `serde(default)` on every field makes every query parameter
//! optional. Filtering itself is applied at the SQL layer.

use serde::Deserialize;

use super::model::SourceKind;

/// All filter axes surfaced by the catalog view. Fields are optional so a
/// callers can combine any subset; bool fields default to `false`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ItemFilter {
    pub search: Option<String>,
    pub format: Option<String>,
    pub source_kind: Option<SourceKind>,
    pub favorite: Option<bool>,
    pub archived: Option<bool>,
    pub duplicate_only: bool,
    pub related_only: bool,
    pub failed_imports_only: bool,
    pub snapshot_id: Option<String>,
    pub slot_key: Option<String>,
    pub scale: Option<String>,
    pub root: Option<String>,
    pub tag: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub needs_review: bool,
}

