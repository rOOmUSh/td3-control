use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::Json;

use crate::error::Td3Error;
use crate::library::duplicates::pattern_hash;
use crate::library::{
    ids,
    model::{
        AnalysisStatus, DuplicateStatus, LibraryItem, SnapshotOrigin, SnapshotSlot, SourceKind,
    },
    store,
};
use crate::pattern::{pattern_to_sysex, Pattern};
use crate::web::api_types::{CreateSnapshotFromPatternsRequest, SnapshotDetailResponse};
use crate::web::bank_handlers::json_payload;
use crate::web::handlers::{web_to_pattern, AppError};
use crate::web::state::AppState;

use super::super::shared::{is_valid_slot_key, load_slot_views, unique_snapshot_name};

type DecodedSnapshotSlot = (String, Vec<u8>, String, Option<String>);

pub(in crate::web::bank_handlers) async fn create_snapshot_from_patterns(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<CreateSnapshotFromPatternsRequest>, JsonRejection>,
) -> Result<Json<SnapshotDetailResponse>, AppError> {
    let req = json_payload(payload, "snapshot from patterns")?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "snapshot name must not be empty".into(),
        ));
    }
    if req.slots.is_empty() {
        return Err(AppError::BadRequest("slots must not be empty".into()));
    }

    let decoded = decode_snapshot_slots(&req)?;
    let existing = state
        .library
        .store
        .list_snapshots()
        .map_err(AppError::Midi)?;
    let effective_name = unique_snapshot_name(&req.name, &existing);

    let snapshot = state
        .library
        .store
        .create_snapshot(effective_name, req.description, SnapshotOrigin::Manual)
        .map_err(AppError::Midi)?;

    let now = store::now_iso();
    for (slot_key, payload, content_hash, display_name_opt) in decoded {
        let item_id = upsert_slot_item(
            &state,
            SlotItemUpsert {
                snapshot_id: &snapshot.snapshot_id,
                snapshot_name: &snapshot.name,
                slot_key: &slot_key,
                payload: &payload,
                content_hash: &content_hash,
                display_name: display_name_opt.as_deref(),
                now: &now,
            },
        )?;

        let slot = SnapshotSlot {
            snapshot_id: snapshot.snapshot_id.clone(),
            slot_key: slot_key.clone(),
            item_id: Some(item_id),
            empty: false,
            display_name: Some(display_name_opt.unwrap_or_else(|| slot_key.clone())),
        };
        state
            .library
            .store
            .upsert_snapshot_slot(slot)
            .map_err(AppError::Midi)?;
    }

    let final_snap = state
        .library
        .store
        .get_snapshot(&snapshot.snapshot_id)
        .map_err(AppError::Midi)?
        .unwrap_or(snapshot);
    let slots = load_slot_views(&state, &final_snap.snapshot_id)?;
    Ok(Json(SnapshotDetailResponse {
        snapshot: final_snap,
        slots,
    }))
}

fn decode_snapshot_slots(
    req: &CreateSnapshotFromPatternsRequest,
) -> Result<Vec<DecodedSnapshotSlot>, AppError> {
    let mut decoded = Vec::with_capacity(req.slots.len());
    let mut seen_keys = HashSet::with_capacity(req.slots.len());

    for (i, slot) in req.slots.iter().enumerate() {
        let key = slot.slot_key.trim().to_string();
        if !is_valid_slot_key(&key) {
            return Err(AppError::BadRequest(format!(
                "slots[{}].slot_key '{}' is malformed; expected 'G{{1..4}}-P{{1..8}}{{A,B}}'",
                i, slot.slot_key,
            )));
        }
        if !seen_keys.insert(key.clone()) {
            return Err(AppError::BadRequest(format!(
                "slots[{}].slot_key '{}' is duplicated in the request",
                i, key,
            )));
        }
        let pattern: Pattern = web_to_pattern(&slot.pattern)?;
        let sysex = pattern_to_sysex(&pattern, 0, 0, 0).map_err(AppError::Midi)?;
        if sysex.len() < 3 || sysex[3..].len() != 112 {
            return Err(AppError::Midi(Td3Error::Other(format!(
                "slots[{}]: pattern_to_sysex produced unexpected length {}",
                i,
                sysex.len(),
            ))));
        }
        let payload = sysex[3..].to_vec();
        let hash = pattern_hash(&pattern);
        let display = slot
            .display_name
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        decoded.push((key, payload, hash, display));
    }

    Ok(decoded)
}

fn upsert_slot_item(state: &Arc<AppState>, input: SlotItemUpsert<'_>) -> Result<String, AppError> {
    let reuse = state
        .library
        .store
        .find_item_by_content_hash(input.content_hash)
        .map_err(AppError::Midi)?;

    if let Some(existing_item) = reuse {
        return Ok(existing_item.item_id);
    }

    let item_display = input
        .display_name
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| input.slot_key.to_string());
    let new_item = LibraryItem {
        item_id: ids::new_id("item"),
        display_name: item_display,
        source_kind: SourceKind::SnapshotSlot,
        source_label: format!("{} @ {}", input.snapshot_name, input.slot_key),
        source_path: None,
        created_at: input.now.to_string(),
        updated_at: input.now.to_string(),
        tags: vec!["snapshot-origin".to_string()],
        favorite: false,
        archived: false,
        slot_key: Some(input.slot_key.to_string()),
        snapshot_id: Some(input.snapshot_id.to_string()),
        snapshot_name: Some(input.snapshot_name.to_string()),
        format: Some("main-overflow".to_string()),
        scale_name: None,
        root_note: None,
        duplicate_status: DuplicateStatus::Unique,
        related_group_count: 0,
        analysis_status: AnalysisStatus::Unknown,
        notes: None,
        content_hash: Some(input.content_hash.to_string()),
    };
    state
        .library
        .store
        .write_pattern_bytes(&new_item.item_id, input.payload)
        .map_err(AppError::Midi)?;
    let saved = state
        .library
        .store
        .upsert_item(new_item)
        .map_err(AppError::Midi)?;
    if let Err(e) = state
        .library
        .store
        .add_tag_to_item(&saved.item_id, "snapshot-origin")
    {
        eprintln!(
            "[bank] warn: tag attach failed for {}: {}",
            saved.item_id, e
        );
    }
    Ok(saved.item_id)
}

struct SlotItemUpsert<'a> {
    snapshot_id: &'a str,
    snapshot_name: &'a str,
    slot_key: &'a str,
    payload: &'a [u8],
    content_hash: &'a str,
    display_name: Option<&'a str>,
    now: &'a str,
}
