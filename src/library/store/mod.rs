//! SQLite-backed catalog store for the Bank Management library.
//!
//! SQLite is the primary source of truth at `<path>`. The in-memory copy is
//! retained as a mutation staging mirror for write-heavy paths that have not
//! yet been fully rewritten to pure SQL transactions.
//!
//! Read paths should prefer direct persistence queries instead of traversing
//! the mirror, so the live database remains authoritative.
//!
//! This file is kept deliberately to a small set of CRUD methods - filter
//! logic lives in `filter.rs`, persistence in `persistence.rs`.
//!
//! Several methods are not called by every build target (e.g. `upsert_item`,
//! `upsert_tag`, `upsert_snapshot_slot`) but remain intentional public API
//! for ingest, analyzer, and test code, so we silence `dead_code` here
//! rather than delete them.
#![allow(dead_code)]

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::bank::BackupInventoryEntry;
use crate::error::Td3Error;

use super::filter::ItemFilter;
use super::ids;
use super::model::{
    AnalysisStatus, DuplicateStatus, FileIndexEntry, ImportBatch, LibraryItem, PatternAnalysis,
    PatternRelation, Snapshot, SnapshotOrigin, SnapshotSlot, SourceKind, Tag, TagKind,
};
use super::persistence;

mod archive;
mod favorites;
mod ingest_facade;
mod items;
mod mirror;
mod sidecars;
mod snapshots;
mod tags; // LEGACY DEFAULTS - TEST-ONLY.
          //
          // Production startup resolves the catalog path and the sidecar directory
          // from `TD3_CONFIG.env` via `load_or_create_with_sidecar(env.library_database_path,
          // env.pattern_sidecar_dir)` (see `src/web/mod.rs::start_server`). The
          // constants below remain only so unit tests under `src/tests/library_*`
          // can spin up a store without an `AppEnv` in scope. They are
          // `cfg(test)`-gated so no production codepath can reach them.
#[cfg(test)]
pub const DEFAULT_PATH: &str = "ui/config/bank-library.sqlite3";

#[cfg(test)]
pub const PATTERN_SIDECAR_DIRNAME: &str = "bank-library-patterns";

/// In-memory representation of the full catalog. `format_version` lets us
/// evolve the SQLite payload shape later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryData {
    #[serde(default = "LibraryData::default_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub items: Vec<LibraryItem>,
    #[serde(default)]
    pub snapshots: Vec<Snapshot>,
    #[serde(default)]
    pub snapshot_slots: Vec<SnapshotSlot>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    /// `(item_id, tag_id)` membership edges.
    #[serde(default)]
    pub item_tags: Vec<(String, String)>,
    #[serde(default)]
    pub file_index: Vec<FileIndexEntry>,
    #[serde(default)]
    pub pattern_analysis: Vec<PatternAnalysis>,
    #[serde(default)]
    pub pattern_relations: Vec<PatternRelation>,
    #[serde(default)]
    pub import_batches: Vec<ImportBatch>,
}

impl LibraryData {
    pub const CURRENT_FORMAT_VERSION: u32 = 2;

    fn default_format_version() -> u32 {
        Self::CURRENT_FORMAT_VERSION
    }
}

impl Default for LibraryData {
    fn default() -> Self {
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            items: Vec::new(),
            snapshots: Vec::new(),
            snapshot_slots: Vec::new(),
            tags: Vec::new(),
            item_tags: Vec::new(),
            file_index: Vec::new(),
            pattern_analysis: Vec::new(),
            pattern_relations: Vec::new(),
            import_batches: Vec::new(),
        }
    }
}

/// Shared, thread-safe catalog store backing the Bank Management UI.
pub struct LibraryStore {
    path: PathBuf,
    /// Relative or absolute path of the per-item sidecar directory. When
    /// relative, it's resolved against the catalog file's parent directory
    /// (so the default `bank-library-patterns` ends up as a sibling of the
    /// SQLite db, preserving historical behavior).
    sidecar_dir: PathBuf,
    data: RwLock<LibraryMirrorData>,
}

#[derive(Debug, Clone, Default)]
struct LibraryMirrorData {
    items: Vec<LibraryItem>,
    snapshots: Vec<Snapshot>,
    snapshot_slots: Vec<SnapshotSlot>,
    tags: Vec<Tag>,
    item_tags: Vec<(String, String)>,
}

impl From<LibraryData> for LibraryMirrorData {
    fn from(value: LibraryData) -> Self {
        Self {
            items: value.items,
            snapshots: value.snapshots,
            snapshot_slots: value.snapshot_slots,
            tags: value.tags,
            item_tags: value.item_tags,
        }
    }
}
impl LibraryStore {
    /// Open the catalog at `path`, creating the SQLite database if it's missing.
    ///
    /// Test-only convenience wrapper. Production must call
    /// `load_or_create_with_sidecar` with paths sourced from
    /// `AppEnv.library_database_path` and `AppEnv.pattern_sidecar_dir`,
    /// so there is exactly one runtime source of truth for both.
    #[cfg(test)]
    pub fn load_or_create(path: impl Into<PathBuf>) -> Result<Self, Td3Error> {
        Self::load_or_create_with_sidecar(path, PATTERN_SIDECAR_DIRNAME)
    }

    /// Open the catalog at `path`. `sidecar_dir` is the per-item sidecar
    /// directory - either a bare name (resolved as a sibling of the catalog
    /// file) or an absolute path.
    pub fn load_or_create_with_sidecar(
        path: impl Into<PathBuf>,
        sidecar_dir: impl Into<PathBuf>,
    ) -> Result<Self, Td3Error> {
        let path = crate::path_safety::require_safe_user_path(path.into())?;
        let sidecar_dir = sidecar_dir.into();
        let data = persistence::load(&path)?;
        let store = LibraryStore {
            path,
            sidecar_dir,
            data: RwLock::new(data.into()),
        };
        // Materialize the database file if it didn't exist, so the UI sees a
        // valid empty catalog rather than a missing-path error.
        let exists = crate::path_safety::require_safe_user_path(&store.path)?.exists();
        if !exists {
            store.save()?;
        }
        // Seed system-reserved tags exactly once (idempotent). "safe-live"
        // is declared for live-performance
        // guardrails.
        store.seed_system_tags()?;
        Ok(store)
    }

    /// Ensure reserved system tags exist. Idempotent: tags are keyed by
    /// label and only inserted when missing.
    fn seed_system_tags(&self) -> Result<(), Td3Error> {
        const SYSTEM_TAGS: &[&str] = &["safe-live"];
        for label in SYSTEM_TAGS {
            let _ = self.ensure_tag_with_kind(label, TagKind::System)?;
        }
        Ok(())
    }

    /// Persist the current in-memory catalog to SQLite inside one transaction.
    pub fn save(&self) -> Result<(), Td3Error> {
        let mirror = self
            .data
            .read()
            .map_err(|_| Td3Error::Other("library: read lock poisoned".into()))?;
        let mut data = persistence::load(&self.path)?;
        data.items = mirror.items.clone();
        data.snapshots = mirror.snapshots.clone();
        data.snapshot_slots = mirror.snapshot_slots.clone();
        data.tags = mirror.tags.clone();
        data.item_tags = mirror.item_tags.clone();
        persistence::save(&self.path, &data)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Snapshot of mirror state used by tests to assert the in-memory mirror
/// didn't drift away from durable state after a failed persistence call.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct MirrorSnapshot {
    pub item_ids: Vec<String>,
    pub item_favorites: Vec<(String, bool)>,
    pub item_archived: Vec<(String, bool)>,
    pub item_tags_per_item: Vec<(String, Vec<String>)>,
    pub snapshot_ids: Vec<String>,
    pub snapshot_names: Vec<(String, String)>,
    pub snapshot_pinned: Vec<(String, bool)>,
    pub tag_labels: Vec<String>,
    pub item_tag_edges: Vec<(String, String)>,
}

/// Summary returned by `LibraryStore::delete_import_batch`. Surfaced to the
/// UI so the user sees exactly how much catalog state a batch deletion
/// cleared. Counts never include surviving rows that merely had a dangling
/// pointer scrubbed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteImportBatchReport {
    pub batch_id: String,
    pub removed_entries: u32,
    pub removed_items: u32,
    pub removed_snapshots: u32,
}

/// Summary returned by `LibraryStore::delete_snapshot`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteSnapshotReport {
    pub snapshot_id: String,
    pub removed_slots: u32,
    pub removed_items: u32,
}
/// Current time as `YYYYMMDDTHHMMSSZ`. Uses `civil_from_days` so we don't
/// take on a `chrono` dependency.
pub fn now_iso() -> String {
    timestamp_utc(std::time::SystemTime::now())
}

fn timestamp_utc(now: std::time::SystemTime) -> String {
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = (secs / 86400) as i64;

    let z = days + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36525 - doe / 146096) / 365;
    let y_base = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y_base + 1 } else { y_base };

    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}Z",
        year, month, d, h, m, s
    )
}
