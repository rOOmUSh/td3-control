//! Bank Management API handlers (`/api/bank/*`).
//!
//! Kept in a separate module so `handlers.rs` stays focused on the existing
//! TD-3 device routes. All handlers here are thin - they delegate to
//! `crate::library` for storage + pure compare/merge logic.
//!
//! The handlers intentionally stay thin:
//! - storage and ingest behavior live in `crate::library`;
//! - item/snapshot compare logic stays pure in `crate::library::compare`;
//! - duplicate and related-group views are derived from catalog state at
//!   request time.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{rejection::JsonRejection, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::bank;
use crate::library::duplicates::pattern_hash;
use crate::library::{
    build_merge_plan, compare_items as lib_compare_items, compare_snapshots, compute_clusters,
    compute_related_groups, ids, ingest,
    model::{
        AnalysisStatus, DuplicateStatus, LibraryItem, SnapshotOrigin, SnapshotSlot, SourceKind,
    },
    statuses_from_clusters, store, DeleteImportBatchReport, FileIndexEntry, FileIngestStatus,
    GroupKind, ItemFilter, RelatedGroup,
};
use crate::pattern::{pattern_to_sysex, sysex_to_pattern, Pattern};
use crate::td3_protocol;
use crate::web::Td3Error;

use super::api_types::*;
use super::handlers::{spawn_clock_runner, stop_clock, web_to_pattern, AppError};
use super::state::{AppState, ClockState};

mod audition;
mod compare;
mod folders;
mod import;
mod items;
mod related;
mod scan;
mod shared;
mod snapshot_io;
mod snapshots;
mod tags;

#[cfg(test)]
pub(crate) use folders::browse_folder_with_picker;

use audition::*;
use compare::*;
use folders::*;
use import::*;
use items::*;
use related::*;
use scan::*;
use snapshot_io::*;
use snapshots::*;
use tags::*;

fn json_payload<T>(
    payload: Result<Json<T>, JsonRejection>,
    name: &'static str,
) -> Result<T, AppError> {
    payload
        .map(|Json(req)| req)
        .map_err(|err| AppError::BadRequest(format!("invalid {} JSON: {}", name, err)))
}

/// Assemble the `/api/bank/*` subrouter. Nested into the main `api` router
/// in `web::start_server`.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/bank/items", get(list_items))
        .route("/bank/items/{id}", get(get_item))
        .route("/bank/items/{id}/pattern", get(get_item_pattern))
        .route("/bank/items/{id}/delete", delete(delete_item))
        .route(
            "/bank/items/{id}/add-to-snapshot",
            post(add_item_to_snapshot),
        )
        .route("/bank/items/{id}/favorite", post(set_favorite))
        .route("/bank/items/{id}/archive", post(set_archived))
        .route("/bank/items/{id}/tags", post(add_tag))
        .route("/bank/items/{id}/tags/{tag}", delete(remove_tag))
        .route("/bank/items/bulk-tag", post(bulk_tag))
        .route("/bank/patterns/save", post(save_patterns_to_bank))
        .route("/bank/snapshots", get(list_snapshots).post(create_snapshot))
        .route(
            "/bank/snapshots/from-patterns",
            post(create_snapshot_from_patterns),
        )
        .route("/bank/snapshots/sync-backups", post(sync_backups))
        .route(
            "/bank/snapshots/{id}",
            get(get_snapshot)
                .patch(patch_snapshot)
                .delete(delete_snapshot),
        )
        .route("/bank/snapshots/{id}/slots", delete(delete_snapshot_slots))
        .route("/bank/snapshots/{id}/move-slot", post(move_snapshot_slot))
        .route(
            "/bank/snapshots/{id}/export-patterns",
            post(export_snapshot_patterns),
        )
        .route("/bank/tags", get(list_tags))
        .route("/bank/scan", post(scan))
        .route("/bank/scan/progress", get(scan_progress))
        .route("/bank/scan/{job_id}", get(scan_job_status))
        .route("/bank/browse-folder", get(browse_folder))
        .route("/bank/import", post(import))
        .route("/bank/import-batches", get(list_import_batches))
        .route("/bank/import-batches/{id}", get(get_import_batch))
        .route(
            "/bank/import-batches/{id}/retry-failed",
            post(retry_failed_batch),
        )
        .route("/bank/import-batches/{id}", delete(delete_import_batch))
        .route("/bank/compare/items", get(compare_items_route))
        .route("/bank/compare/snapshots", get(compare_snapshots_route))
        .route("/bank/merge-plan", post(merge_plan_route))
        .route("/bank/merge-plan/preview", post(merge_plan_preview_route))
        .route("/bank/related", get(list_related))
        .route("/bank/duplicates", get(list_duplicates))
        .route("/bank/items/{id}/play", post(play_item))
        .route("/bank/playing", get(get_playing))
}
