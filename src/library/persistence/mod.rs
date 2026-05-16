//! SQLite-backed load/save helpers for the Bank Management catalog.
//!
//! The store still owns an in-memory `LibraryData`, but persistence now lands
//! in a SQLite database instead of a single JSON document. This is the first
//! migration step:
//! - keep the existing `LibraryStore` API stable;
//! - store each entity collection in its own table;
//! - rewrite the full catalog inside one SQL transaction on `save()`;
//! - import a sibling legacy `bank-library.json` on first open when the DB is
//!   still empty.
//!
//! The helper surface now supports both:
//! - full-catalog saves, used by the complex low-frequency paths; and
//! - targeted row-level writes for the hot mutation paths.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, params_from_iter, types::Value, Connection};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::Td3Error;

use super::filter::ItemFilter;
use super::model::{
    FileIndexEntry, ImportBatch, LibraryItem, PatternAnalysis, PatternRelation, Snapshot,
    SnapshotSlot, Tag,
};
use super::store::LibraryData;

const TABLE_METADATA: &str = "metadata";
const TABLE_ITEMS: &str = "items";
const TABLE_SNAPSHOTS: &str = "snapshots";
const TABLE_SNAPSHOT_SLOTS: &str = "snapshot_slots";
const TABLE_TAGS: &str = "tags";
const TABLE_ITEM_TAGS: &str = "item_tags";
const TABLE_FILE_INDEX: &str = "file_index";
const TABLE_PATTERN_ANALYSIS: &str = "pattern_analysis";
const TABLE_PATTERN_RELATIONS: &str = "pattern_relations";
const TABLE_IMPORT_BATCHES: &str = "import_batches";

#[derive(Debug, Default)]
pub struct DeleteImportBatchPlan {
    pub batch_existed: bool,
    pub batch_paths: Vec<String>,
    pub items_to_delete: Vec<String>,
    pub snapshots_to_delete: Vec<String>,
    pub orphan_snapshot_ids: Vec<String>,
}

#[derive(Debug, Default)]
pub struct DeleteImportBatchApplyReport {
    pub removed_entries: u32,
}

mod duplicate_status;
mod item_queries;
mod item_writes;
mod migrations;
mod schema_migrations;
mod snapshot_queries;
mod snapshot_writes;
mod transactions;

pub use duplicate_status::*;
pub use item_queries::*;
pub use item_writes::*;
pub use migrations::*;
pub use snapshot_queries::*;
pub use snapshot_writes::*;
pub use transactions::{load, save};
