use super::shared::*;
use super::*;

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

pub(super) async fn list_items(
    State(state): State<Arc<AppState>>,
    Query(filter): Query<ItemFilter>,
) -> Result<Json<BankItemsResponse>, AppError> {
    let items = state
        .library
        .store
        .list_items(&filter)
        .map_err(AppError::Midi)?;
    let total = items.len() as u32;
    Ok(Json(BankItemsResponse { items, total }))
}

pub(super) async fn get_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BankItemResponse>, AppError> {
    let item = state
        .library
        .store
        .get_item(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("item '{}' not found", id)))?;
    Ok(Json(BankItemResponse { item }))
}

pub(super) async fn get_item_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ItemPatternResponse>, AppError> {
    let pattern = load_pattern(&state, &id).map_err(AppError::BadRequest)?;
    Ok(Json(ItemPatternResponse {
        item_id: id,
        pattern: WebPattern::from_pattern(&pattern),
    }))
}

pub(super) async fn delete_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<DeleteBankItemResponse>, AppError> {
    let deleted = state
        .library
        .store
        .delete_item(&id)
        .map_err(AppError::Midi)?;
    if !deleted {
        return Err(AppError::BadRequest(format!("item '{}' not found", id)));
    }
    Ok(Json(DeleteBankItemResponse {
        item_id: id,
        deleted,
    }))
}

pub(super) async fn add_item_to_snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddItemToSnapshotRequest>,
) -> Result<Json<AddItemToSnapshotResponse>, AppError> {
    let item = state
        .library
        .store
        .get_item(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("item '{}' not found", id)))?;

    let requested_snapshot = req
        .snapshot_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let (snapshot, created_snapshot) = match requested_snapshot {
        Some(snapshot_id) => {
            let snapshot = state
                .library
                .store
                .get_snapshot(snapshot_id)
                .map_err(AppError::Midi)?
                .ok_or_else(|| {
                    AppError::BadRequest(format!("snapshot '{}' not found", snapshot_id))
                })?;
            (snapshot, false)
        }
        None => {
            let existing = state
                .library
                .store
                .list_snapshots()
                .map_err(AppError::Midi)?;
            if !existing.is_empty() {
                return Err(AppError::BadRequest(
                    "snapshot_id is required when snapshots exist".into(),
                ));
            }
            let snapshot = state
                .library
                .store
                .create_snapshot(
                    default_add_to_snapshot_name(),
                    Some("Created by Add to Snapshot".into()),
                    SnapshotOrigin::Manual,
                )
                .map_err(AppError::Midi)?;
            (snapshot, true)
        }
    };

    let current_slots = load_slot_views(&state, &snapshot.snapshot_id)?;
    let slot_key = match req
        .slot_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(key) => {
            if !is_valid_slot_key(key) {
                return Err(AppError::BadRequest(format!(
                    "slot_key '{}' is malformed; expected 'G{{1..4}}-P{{1..8}}{{A,B}}'",
                    key,
                )));
            }
            key.to_string()
        }
        None => current_slots
            .iter()
            .find(|slot| slot.empty || slot.item_id.is_none())
            .map(|slot| slot.slot_key.clone())
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "snapshot '{}' has no empty slots",
                    snapshot.snapshot_id
                ))
            })?,
    };

    if current_slots
        .iter()
        .any(|slot| slot.slot_key == slot_key && !slot.empty && slot.item_id.is_some())
    {
        return Err(AppError::BadRequest(format!(
            "snapshot '{}' slot '{}' is already occupied",
            snapshot.snapshot_id, slot_key,
        )));
    }

    let display_name = req
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(item.display_name.as_str())
        .to_string();
    let slot = SnapshotSlot {
        snapshot_id: snapshot.snapshot_id.clone(),
        slot_key: slot_key.clone(),
        item_id: Some(item.item_id.clone()),
        empty: false,
        display_name: Some(display_name),
    };
    state
        .library
        .store
        .upsert_snapshot_slot(slot)
        .map_err(AppError::Midi)?;
    let snapshot = state
        .library
        .store
        .refresh_snapshot_slot_count(&snapshot.snapshot_id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| {
            AppError::BadRequest(format!("snapshot '{}' not found", snapshot.snapshot_id))
        })?;
    let slots = load_slot_views(&state, &snapshot.snapshot_id)?;
    let slot = slots
        .iter()
        .find(|slot| slot.slot_key == slot_key)
        .cloned()
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "snapshot slot '{}' not found after insert",
                slot_key
            ))
        })?;

    Ok(Json(AddItemToSnapshotResponse {
        item_id: item.item_id,
        snapshot,
        slot,
        slots,
        created_snapshot,
    }))
}

pub(super) async fn set_favorite(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<FavoriteRequest>,
) -> Result<Json<BankItemFlagResponse>, AppError> {
    let result = state
        .library
        .store
        .set_favorite(&id, req.favorite)
        .map_err(AppError::Midi)?;
    Ok(Json(BankItemFlagResponse {
        item_id: id,
        favorite: result,
        archived: None,
    }))
}

pub(super) async fn set_archived(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ArchiveRequest>,
) -> Result<Json<BankItemFlagResponse>, AppError> {
    let result = state
        .library
        .store
        .set_archived(&id, req.archived)
        .map_err(AppError::Midi)?;
    Ok(Json(BankItemFlagResponse {
        item_id: id,
        favorite: None,
        archived: result,
    }))
}

pub(super) async fn save_patterns_to_bank(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<SavePatternsToBankRequest>, JsonRejection>,
) -> Result<Json<SavePatternsToBankResponse>, AppError> {
    let req = json_payload(payload, "bank pattern save")?;
    if req.entries.is_empty() {
        return Err(AppError::BadRequest("entries must not be empty".into()));
    }

    let root_note = clean_optional(req.root_note.as_deref());
    let scale_name = clean_optional(req.scale_name.as_deref());
    let mut decoded = Vec::with_capacity(req.entries.len());
    for (i, entry) in req.entries.iter().enumerate() {
        let slot_key = match clean_optional(entry.slot_key.as_deref()) {
            Some(key) => {
                if !is_valid_slot_key(&key) {
                    return Err(AppError::BadRequest(format!(
                        "entries[{}].slot_key '{}' is malformed; expected 'G{{1..4}}-P{{1..8}}{{A,B}}'",
                        i, key,
                    )));
                }
                Some(key)
            }
            None => None,
        };
        let pattern: Pattern = web_to_pattern(&entry.pattern)?;
        let sysex = pattern_to_sysex(&pattern, 0, 0, 0).map_err(AppError::Midi)?;
        if sysex.len() < 3 || sysex[3..].len() != 112 {
            return Err(AppError::Midi(Td3Error::Other(format!(
                "entries[{}]: pattern_to_sysex produced unexpected length {}",
                i,
                sysex.len(),
            ))));
        }
        let display_name = clean_optional(entry.display_name.as_deref())
            .unwrap_or_else(|| format!("Pattern {}", i + 1));
        decoded.push(DecodedBankPattern {
            display_name,
            preferred_slot_key: slot_key,
            payload: sysex[3..].to_vec(),
            content_hash: pattern_hash(&pattern),
        });
    }

    match req.destination.trim() {
        "single_item" => {
            let now = store::now_iso();
            let mut items = Vec::with_capacity(decoded.len());
            for decoded_entry in decoded {
                let item = materialize_bank_pattern_item(
                    &state,
                    decoded_entry,
                    SourceKind::Generated,
                    "multipattern canvas".to_string(),
                    SnapshotAssociation {
                        slot_key: None,
                        snapshot_id: None,
                        snapshot_name: None,
                    },
                    MusicalContext {
                        root_note: root_note.as_deref(),
                        scale_name: scale_name.as_deref(),
                    },
                    &now,
                )?;
                items.push(item);
            }
            Ok(Json(SavePatternsToBankResponse {
                items,
                snapshot: None,
                slots: Vec::new(),
                created_snapshot: false,
            }))
        }
        "new_snapshot" | "snapshot" => {
            let (snapshot, created_snapshot) = if req.destination.trim() == "snapshot" {
                let id = clean_optional(req.snapshot_id.as_deref()).ok_or_else(|| {
                    AppError::BadRequest("snapshot_id is required for destination=snapshot".into())
                })?;
                let snapshot = state
                    .library
                    .store
                    .get_snapshot(&id)
                    .map_err(AppError::Midi)?
                    .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;
                (snapshot, false)
            } else {
                let existing = state
                    .library
                    .store
                    .list_snapshots()
                    .map_err(AppError::Midi)?;
                let wanted = clean_optional(req.snapshot_name.as_deref())
                    .unwrap_or_else(default_add_to_snapshot_name);
                let name = unique_snapshot_name(&wanted, &existing);
                let snapshot = state
                    .library
                    .store
                    .create_snapshot(name, req.description, SnapshotOrigin::Manual)
                    .map_err(AppError::Midi)?;
                (snapshot, true)
            };

            let mut occupied: std::collections::HashSet<String> =
                load_slot_views(&state, &snapshot.snapshot_id)?
                    .into_iter()
                    .filter(|slot| !slot.empty && slot.item_id.is_some())
                    .map(|slot| slot.slot_key)
                    .collect();
            let mut assigned = Vec::with_capacity(decoded.len());
            for decoded_entry in decoded {
                let slot_key =
                    choose_snapshot_slot(decoded_entry.preferred_slot_key.as_deref(), &occupied)
                        .ok_or_else(|| {
                            AppError::BadRequest(format!(
                                "snapshot '{}' has no empty slots",
                                snapshot.snapshot_id
                            ))
                        })?;
                occupied.insert(slot_key.clone());
                assigned.push((decoded_entry, slot_key));
            }

            let now = store::now_iso();
            let mut items = Vec::with_capacity(assigned.len());
            for (decoded_entry, slot_key) in assigned {
                let item = materialize_bank_pattern_item(
                    &state,
                    decoded_entry,
                    SourceKind::SnapshotSlot,
                    format!("{} @ {}", snapshot.name, slot_key),
                    SnapshotAssociation {
                        slot_key: Some(slot_key.clone()),
                        snapshot_id: Some(snapshot.snapshot_id.clone()),
                        snapshot_name: Some(snapshot.name.clone()),
                    },
                    MusicalContext {
                        root_note: root_note.as_deref(),
                        scale_name: scale_name.as_deref(),
                    },
                    &now,
                )?;
                let slot = SnapshotSlot {
                    snapshot_id: snapshot.snapshot_id.clone(),
                    slot_key: slot_key.clone(),
                    item_id: Some(item.item_id.clone()),
                    empty: false,
                    display_name: Some(item.display_name.clone()),
                };
                state
                    .library
                    .store
                    .upsert_snapshot_slot(slot)
                    .map_err(AppError::Midi)?;
                items.push(item);
            }

            let snapshot = state
                .library
                .store
                .refresh_snapshot_slot_count(&snapshot.snapshot_id)
                .map_err(AppError::Midi)?
                .ok_or_else(|| {
                    AppError::BadRequest(format!("snapshot '{}' not found", snapshot.snapshot_id))
                })?;
            let slots = load_slot_views(&state, &snapshot.snapshot_id)?;
            Ok(Json(SavePatternsToBankResponse {
                items,
                snapshot: Some(snapshot),
                slots,
                created_snapshot,
            }))
        }
        other => Err(AppError::BadRequest(format!(
            "unsupported destination '{}'; expected new_snapshot, snapshot, or single_item",
            other,
        ))),
    }
}
