//! Bank Management library module.
//!
//! This module owns:
//! - the long-lived catalog of pattern items, snapshots, tags, and relations
//!   (Persistence layer of the Bank Management UI);
//! - analysis surfacing helpers for duplicate/related detection;
//! - pure comparison and merge-plan functions consumed by `/api/bank/*`
//!   handlers.
//!
//! The submodules are deliberately small so each concern is isolated:
//!
//! - `model`        - domain types (items, snapshots, tags, relations).
//! - `ids`          - dependency-free time-sortable ID generator.
//! - `store`        - SQLite-backed catalog store.
//! - `filter`       - `ItemFilter` + `apply_filter` predicate logic.
//! - `persistence`  - SQLite load/save + legacy JSON import helpers.
//! - `compare`      - pure diff functions (item-to-item, snapshot-to-snapshot).
//! - `merge_plan`   - pure merge-plan builder for snapshot merges.
//! - `duplicates`   - exact + near-duplicate clustering over pattern data.
//! - `scanner`      - filesystem extension classifier (no parsing yet).

pub mod compare;
pub mod duplicates;
pub mod filter;
pub mod ids;
pub mod ingest;
pub mod merge_plan;
pub mod model;
pub mod persistence;
pub mod related;
pub mod scanner;
pub mod store;

// The items below are the stable public surface of the library module -
// handlers, tests, and adjacent analysis code import them. Some re-exports
// are only used by selected targets, so silence the dead-code warnings
// instead of deleting names the UI and tests rely on.
#[allow(unused_imports)]
pub use compare::{
    compare_items, compare_snapshots, ItemCompareReport, SlotCompareOutcome, SlotCompareState,
    SnapshotCompareReport,
};
#[allow(unused_imports)]
pub use duplicates::{
    compute_clusters, statuses_from_clusters, DuplicateCluster, DuplicateClusterKind,
};
pub use filter::ItemFilter;
#[allow(unused_imports)]
pub use merge_plan::{
    build_merge_plan, MergeAction, MergeOperation, MergeOperationAction, MergePlan, MergePlanStep,
};
#[allow(unused_imports)]
pub use model::{
    AnalysisStatus, DuplicateStatus, FileIndexEntry, FileIngestStatus, ImportBatch, LibraryItem,
    PatternAnalysis, PatternRelation, RelationKind, Snapshot, SnapshotOrigin, SnapshotSlot,
    SourceKind, Tag, TagKind,
};
#[allow(unused_imports)]
pub use related::{compute_related_groups, GroupKind, RelatedGroup};
#[allow(unused_imports)]
pub use store::{DeleteImportBatchReport, LibraryData, LibraryStore};
