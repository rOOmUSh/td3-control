use super::shared::*;
use super::*;

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

pub(super) async fn list_snapshots(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SnapshotsResponse>, AppError> {
    let snapshots = state
        .library
        .store
        .list_snapshots()
        .map_err(AppError::Midi)?;
    Ok(Json(SnapshotsResponse { snapshots }))
}

pub(super) async fn get_snapshot(
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

pub(super) async fn delete_snapshot(
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

pub(super) async fn create_snapshot(
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

/// Atomic helper for the main-page PUSH TO TD-3 overflow flow.
///
/// Creates a new snapshot filled with one `SnapshotSlot` per supplied
/// `{ slot_key, pattern }` pair. Each pattern is decoded via
/// `web_to_pattern` + `pattern_to_sysex`, the resulting 112-byte body is
/// content-hashed for library-item dedupe, a sidecar is written for
/// audition/compare parity with bank-ingested slots, and the slot row is
/// upserted. Name collisions are resolved by appending ` (2)`, ` (3)`, ...
/// until a free name is found; the effective name lands in the returned
/// `Snapshot`.
///
/// Validation is strict: empty names, empty slot lists, malformed slot
/// keys, and any per-slot decode failure cause the whole request to fail
/// before any row is written, so the catalog never ends up with a
/// half-populated snapshot.
pub(super) async fn create_snapshot_from_patterns(
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

    // Pre-validate every slot *before* touching the catalog so a late
    // decode failure can't leave a half-filled snapshot behind. We keep
    // the decoded `(slot_key, payload, hash, display_name)` tuples around so
    // the write loop below doesn't have to repeat the work. `display_name`
    // is the optional per-slot label; when None the write loop uses the
    // slot_key as the visible name (legacy main-overflow behavior).
    let mut decoded: Vec<(String, Vec<u8>, String, Option<String>)> =
        Vec::with_capacity(req.slots.len());
    let mut seen_keys: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(req.slots.len());
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

    // Resolve name collisions. The list_snapshots call is cheap (catalog
    // is in-memory) and we retry by appending " (N)".
    // "if main-overflow-1970-01-01 already exists ... append ` (N)`".
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

    // Populate slots. Per-slot failures here are surfaced but the
    // already-created snapshot stays - callers can retry by deleting it
    // and submitting again. We still record the slot row (empty=true) so
    // the 64-wide grid stays intact for bank UI consumers.
    let now = store::now_iso();
    for (slot_key, payload, content_hash, display_name_opt) in decoded {
        let reuse = state
            .library
            .store
            .find_item_by_content_hash(&content_hash)
            .map_err(AppError::Midi)?;

        let item_id = if let Some(existing_item) = reuse {
            // Dedupe: if the catalog already holds this exact pattern,
            // attach the new snapshot slot to that item rather than
            // creating a duplicate row. Existing item.display_name stays
            // as-is - the caller's per-slot display_name only governs the
            // SnapshotSlot label (visible in the snapshot grid).
            existing_item.item_id
        } else {
            // New item: caller-supplied display_name wins; fallback is the
            // slot_key so legacy callers (main-overflow) keep their shape.
            let item_display = display_name_opt.clone().unwrap_or_else(|| slot_key.clone());
            let new_item = LibraryItem {
                item_id: ids::new_id("item"),
                display_name: item_display,
                source_kind: SourceKind::SnapshotSlot,
                source_label: format!("{} @ {}", snapshot.name, slot_key),
                source_path: None,
                created_at: now.clone(),
                updated_at: now.clone(),
                tags: vec!["snapshot-origin".to_string()],
                favorite: false,
                archived: false,
                slot_key: Some(slot_key.clone()),
                snapshot_id: Some(snapshot.snapshot_id.clone()),
                snapshot_name: Some(snapshot.name.clone()),
                format: Some("main-overflow".to_string()),
                scale_name: None,
                root_note: None,
                duplicate_status: DuplicateStatus::Unique,
                related_group_count: 0,
                analysis_status: AnalysisStatus::Unknown,
                notes: None,
                content_hash: Some(content_hash.clone()),
            };
            state
                .library
                .store
                .write_pattern_bytes(&new_item.item_id, &payload)
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
            saved.item_id
        };

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

    // Re-fetch so `slot_count` reflects the freshly-written slots, not
    // the zero it had at create-time.
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

pub(super) async fn patch_snapshot(
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

/// Remove the listed `slot_keys` from `snapshot_id`. The padded 64-cell view
/// transparently fills the holes back in with `empty = true` placeholders, so
/// callers see the same grid shape afterwards - only the previously-occupied
/// slots become empty. The underlying `LibraryItem`s are left alone (they may
/// still be referenced by other snapshots). Validates each key with
/// `is_valid_slot_key` so a malformed request fails fast before touching SQLite.
pub(super) async fn delete_snapshot_slots(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DeleteSnapshotSlotsRequest>,
) -> Result<Json<DeleteSnapshotSlotsResponse>, AppError> {
    if req.slot_keys.is_empty() {
        return Err(AppError::BadRequest("slot_keys must not be empty".into()));
    }
    // TD-3 hardware has 64 pattern slots; cap to bound allocation.
    const MAX_SLOTS: usize = 64;
    if req.slot_keys.len() > MAX_SLOTS {
        return Err(AppError::BadRequest(format!(
            "slot_keys length {} exceeds maximum {}",
            req.slot_keys.len(),
            MAX_SLOTS
        )));
    }
    let mut keys: Vec<String> = Vec::with_capacity(MAX_SLOTS);
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(MAX_SLOTS);
    for k in req.slot_keys.iter() {
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

/// Move (or swap) the slot at `from_key` to `to_key` inside `snapshot_id`. The
/// destination may be empty (move) or occupied (swap). Validates both keys
/// against the canonical `G{1..4}-P{1..8}{A,B}` shape and returns the fresh
/// padded 64-cell view so the caller can re-render in one round trip.
pub(super) async fn move_snapshot_slot(
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
