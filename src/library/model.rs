//! Domain types for the Bank Management library.
//!
//! These types are the stable on-disk + over-the-wire shape of the library.
//! They are intentionally flat and `Serialize`/`Deserialize` so the store
//! can round-trip them through a single JSON document and the web handlers
//! can return them directly without a separate DTO layer.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

/// A single catalog entry in the library - one pattern-sized unit of content,
/// regardless of whether it originated as a file on disk, a slot in a bank
/// snapshot, a generated pattern, or a curated preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    pub item_id: String,
    pub display_name: String,
    pub source_kind: SourceKind,
    pub source_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_note: Option<String>,
    #[serde(default = "default_duplicate_status")]
    pub duplicate_status: DuplicateStatus,
    #[serde(default)]
    pub related_group_count: u32,
    #[serde(default = "default_analysis_status")]
    pub analysis_status: AnalysisStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// SHA-256 hex of the canonical pattern bytes, when the item is backed by
    /// a real pattern. Used for duplicate detection across files. `None` for
    /// catalog entries without pattern payloads (e.g. legacy records imported
    /// before hashing was available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

fn default_duplicate_status() -> DuplicateStatus {
    DuplicateStatus::Unknown
}

fn default_analysis_status() -> AnalysisStatus {
    AnalysisStatus::Unknown
}

/// Where the item originated. Serialized as lowercase for a stable wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    File,
    SnapshotSlot,
    Generated,
    Curated,
}

/// Duplicate-detection status surfaced to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateStatus {
    Unique,
    ExactDuplicate,
    NearDuplicate,
    Unknown,
}

/// Pipeline state for per-item analysis (scale, rhythm, relations).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisStatus {
    Unknown,
    Pending,
    Ready,
    NeedsReview,
    Failed,
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

/// A bank-level snapshot of 64 slots (4 groups × 8 patterns × 2 sides).
/// Snapshots are how imports, backups, and manual saves are catalogued.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub snapshot_id: String,
    pub name: String,
    pub created_at: String,
    pub origin: SnapshotOrigin,
    pub slot_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
}

/// Why the snapshot was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotOrigin {
    Backup,
    Imported,
    Manual,
    Merge,
}

/// A single slot within a snapshot. When `item_id` is `None`, `empty` must be
/// `true`; the store returns 64 entries per snapshot either way so the UI can
/// render a stable 8×8 grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSlot {
    pub snapshot_id: String,
    pub slot_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub empty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

/// A tag. User-created, auto-derived, or system-reserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub tag_id: String,
    pub label: String,
    pub kind: TagKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagKind {
    User,
    Auto,
    System,
}

// ---------------------------------------------------------------------------
// File index
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndexEntry {
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_sha256: Option<String>,
    pub discovered_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    pub status: FileIngestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// ID of the `ImportBatch` that owns this entry. Optional for backward
    /// compatibility with older catalogs written before batch linkage was
    /// enforced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    /// When `status = DuplicateSkipped`, this holds the `item_id` of the
    /// already-cataloged LibraryItem whose hash matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
    /// When `status = Imported` and the parse produced a new LibraryItem, this
    /// points at that item so the UI can link directly from an entry row to
    /// the detail drawer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileIngestStatus {
    Discovered,
    Parsed,
    Imported,
    DuplicateSkipped,
    Unsupported,
    Failed,
}

// ---------------------------------------------------------------------------
// Analysis + relations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternAnalysis {
    pub item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rhythm_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_rhythm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRelation {
    pub from_item_id: String,
    pub to_item_id: String,
    pub kind: RelationKind,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    SameScale,
    SameRoot,
    SameRhythm,
    NearDuplicate,
    AnalyzerRelated,
    ProgressionFamily,
}

// ---------------------------------------------------------------------------
// Import batches
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBatch {
    pub batch_id: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_root: Option<String>,
    pub files_found: u32,
    pub files_imported: u32,
    pub duplicates_skipped: u32,
    pub unsupported: u32,
    pub failed: u32,
}
