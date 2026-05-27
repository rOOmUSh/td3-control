use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::web::api_types::{
    CreateSnapshotRequest, DeleteSnapshotResponse, PatchSnapshotRequest, SnapshotDetailResponse,
    SnapshotsResponse,
};
use crate::web::handlers::AppError;
use crate::web::state::AppState;

use super::super::shared::load_slot_views;

pub(in crate::web::bank_handlers) async fn list_snapshots(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SnapshotsResponse>, AppError> {
    let snapshots = state
        .library
        .store
        .list_snapshots()
        .map_err(AppError::Midi)?;
    Ok(Json(SnapshotsResponse { snapshots }))
}

pub(in crate::web::bank_handlers) async fn get_snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SnapshotDetailResponse>, AppError> {
    let snapshot = state
        .library
        .store
        .get_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;
    let slots = load_slot_views(&state, &id)?;
    Ok(Json(SnapshotDetailResponse { snapshot, slots }))
}

pub(in crate::web::bank_handlers) async fn delete_snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<DeleteSnapshotResponse>, AppError> {
    let report = state
        .library
        .store
        .delete_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;
    Ok(Json(DeleteSnapshotResponse {
        snapshot_id: report.snapshot_id,
        removed_slots: report.removed_slots,
        removed_items: report.removed_items,
    }))
}

pub(in crate::web::bank_handlers) async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<Json<SnapshotDetailResponse>, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "snapshot name must not be empty".into(),
        ));
    }
    let snap = state
        .library
        .store
        .create_snapshot(req.name, req.description, req.origin)
        .map_err(AppError::Midi)?;
    let slots = load_slot_views(&state, &snap.snapshot_id)?;
    Ok(Json(SnapshotDetailResponse {
        snapshot: snap,
        slots,
    }))
}

pub(in crate::web::bank_handlers) async fn patch_snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PatchSnapshotRequest>,
) -> Result<Json<SnapshotDetailResponse>, AppError> {
    if let Some(name) = req.name {
        state
            .library
            .store
            .rename_snapshot(&id, name)
            .map_err(AppError::Midi)?;
    }
    if let Some(pinned) = req.pinned {
        state
            .library
            .store
            .pin_snapshot(&id, pinned)
            .map_err(AppError::Midi)?;
    }
    if req.description.is_some() {
        state
            .library
            .store
            .update_snapshot_description(&id, req.description)
            .map_err(AppError::Midi)?;
    }
    let snapshot = state
        .library
        .store
        .get_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;
    let slots = load_slot_views(&state, &id)?;
    Ok(Json(SnapshotDetailResponse { snapshot, slots }))
}
