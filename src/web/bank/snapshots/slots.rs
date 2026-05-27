use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::error::Td3Error;
use crate::web::api_types::{
    DeleteSnapshotSlotsRequest, DeleteSnapshotSlotsResponse, MoveSnapshotSlotRequest,
    MoveSnapshotSlotResponse,
};
use crate::web::handlers::AppError;
use crate::web::state::AppState;

use super::super::shared::{is_valid_slot_key, load_slot_views};

pub(in crate::web::bank_handlers) async fn delete_snapshot_slots(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DeleteSnapshotSlotsRequest>,
) -> Result<Json<DeleteSnapshotSlotsResponse>, AppError> {
    if req.slot_keys.is_empty() {
        return Err(AppError::BadRequest("slot_keys must not be empty".into()));
    }
    const MAX_SLOTS: usize = 64;
    if req.slot_keys.len() > MAX_SLOTS {
        return Err(AppError::BadRequest(format!(
            "slot_keys length {} exceeds maximum {}",
            req.slot_keys.len(),
            MAX_SLOTS
        )));
    }
    let keys = validated_slot_keys(&req.slot_keys)?;

    state
        .library
        .store
        .get_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;

    let removed = state
        .library
        .store
        .delete_snapshot_slots(&id, &keys)
        .map_err(AppError::Midi)?;
    Ok(Json(DeleteSnapshotSlotsResponse {
        deleted: removed as u32,
    }))
}

pub(in crate::web::bank_handlers) async fn move_snapshot_slot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<MoveSnapshotSlotRequest>,
) -> Result<Json<MoveSnapshotSlotResponse>, AppError> {
    let from = req.from_key.trim().to_string();
    let to = req.to_key.trim().to_string();
    if !is_valid_slot_key(&from) {
        return Err(AppError::BadRequest(format!(
            "from_key '{}' is malformed; expected 'G{{1..4}}-P{{1..8}}{{A,B}}'",
            req.from_key,
        )));
    }
    if !is_valid_slot_key(&to) {
        return Err(AppError::BadRequest(format!(
            "to_key '{}' is malformed; expected 'G{{1..4}}-P{{1..8}}{{A,B}}'",
            req.to_key,
        )));
    }
    if from == to {
        return Err(AppError::BadRequest(
            "from_key and to_key must differ".into(),
        ));
    }

    state
        .library
        .store
        .get_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;

    let swapped = state
        .library
        .store
        .move_snapshot_slot(&id, &from, &to)
        .map_err(|e| match &e {
            Td3Error::Other(msg) if msg.contains("source slot") => {
                AppError::BadRequest(msg.clone())
            }
            _ => AppError::Midi(e),
        })?;

    let snapshot = state
        .library
        .store
        .get_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;
    let slots = load_slot_views(&state, &id)?;
    Ok(Json(MoveSnapshotSlotResponse {
        swapped,
        snapshot,
        slots,
    }))
}

fn validated_slot_keys(slot_keys: &[String]) -> Result<Vec<String>, AppError> {
    let mut keys = Vec::with_capacity(slot_keys.len());
    let mut seen = HashSet::with_capacity(slot_keys.len());
    for k in slot_keys {
        let trimmed = k.trim().to_string();
        if !is_valid_slot_key(&trimmed) {
            return Err(AppError::BadRequest(format!(
                "slot_key '{}' is malformed; expected 'G{{1..4}}-P{{1..8}}{{A,B}}'",
                k,
            )));
        }
        if seen.insert(trimmed.clone()) {
            keys.push(trimmed);
        }
    }
    Ok(keys)
}
