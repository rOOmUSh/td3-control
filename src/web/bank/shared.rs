use super::*;

/// Return `true` if `s` is a valid "G{1..4}-P{1..8}{A,B}" slot key. Strict
/// to match the store's canonical shape.
pub(super) fn is_valid_slot_key(s: &str) -> bool {
    let bytes = s.as_bytes();
    // Exactly 6 chars: G, digit, '-', P, digit, side
    if bytes.len() != 6 {
        return false;
    }
    if bytes[0] != b'G' {
        return false;
    }
    if !(b'1'..=b'4').contains(&bytes[1]) {
        return false;
    }
    if bytes[2] != b'-' {
        return false;
    }
    if bytes[3] != b'P' {
        return false;
    }
    if !(b'1'..=b'8').contains(&bytes[4]) {
        return false;
    }
    if bytes[5] != b'A' && bytes[5] != b'B' {
        return false;
    }
    true
}

/// Resolve a snapshot name against an existing catalog so the caller gets a
/// unique name back. If `wanted` is already free, it's returned as-is. On
/// collision, append " (2)", " (3)", ..., up to 99 (beyond which we give up
/// and just return the 99th variant - astronomically unlikely in practice).
pub(super) fn unique_snapshot_name(
    wanted: &str,
    existing: &[crate::library::model::Snapshot],
) -> String {
    let existing_names: std::collections::HashSet<&str> =
        existing.iter().map(|s| s.name.as_str()).collect();
    if !existing_names.contains(wanted) {
        return wanted.to_string();
    }
    for n in 2..=99 {
        let candidate = format!("{} ({})", wanted, n);
        if !existing_names.contains(candidate.as_str()) {
            return candidate;
        }
    }
    format!("{} (99)", wanted)
}

pub(super) fn default_add_to_snapshot_name() -> String {
    format!("SN_{}", store::now_iso().trim_end_matches('Z'))
}

pub(super) struct DecodedBankPattern {
    pub(super) display_name: String,
    pub(super) preferred_slot_key: Option<String>,
    pub(super) payload: Vec<u8>,
    pub(super) content_hash: String,
}

pub(super) fn clean_optional(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

pub(super) fn choose_snapshot_slot(
    preferred: Option<&str>,
    occupied: &std::collections::HashSet<String>,
) -> Option<String> {
    if let Some(key) = preferred {
        if !occupied.contains(key) {
            return Some(key.to_string());
        }
    }
    for g in 1..=4u8 {
        for p in 1..=8u8 {
            for side in ['A', 'B'] {
                let key = format!("G{}-P{}{}", g, p, side);
                if !occupied.contains(&key) {
                    return Some(key);
                }
            }
        }
    }
    None
}

/// Snapshot-side association for a materialized bank pattern. All three
/// fields are either all `None` (free-standing item) or all `Some`
/// (slot inside a snapshot), so they travel as a single value.
pub(super) struct SnapshotAssociation {
    pub slot_key: Option<String>,
    pub snapshot_id: Option<String>,
    pub snapshot_name: Option<String>,
}

/// Optional musical context tagged onto the materialized item.
#[derive(Clone, Copy)]
pub(super) struct MusicalContext<'a> {
    pub root_note: Option<&'a str>,
    pub scale_name: Option<&'a str>,
}

pub(super) fn materialize_bank_pattern_item(
    state: &Arc<AppState>,
    decoded: DecodedBankPattern,
    source_kind: SourceKind,
    source_label: String,
    assoc: SnapshotAssociation,
    music: MusicalContext<'_>,
    now: &str,
) -> Result<LibraryItem, AppError> {
    let SnapshotAssociation {
        slot_key,
        snapshot_id,
        snapshot_name,
    } = assoc;
    let MusicalContext {
        root_note,
        scale_name,
    } = music;
    let mut tags = vec!["multipattern".to_string()];
    if slot_key.is_some() {
        tags.push("snapshot-origin".to_string());
    }
    if let Some(root) = root_note {
        tags.push(format!("root:{}", root));
    }
    if let Some(scale) = scale_name {
        tags.push(format!("scale:{}", scale));
    }
    tags.sort();
    tags.dedup();

    let mut item = match state
        .library
        .store
        .find_item_by_content_hash(&decoded.content_hash)
        .map_err(AppError::Midi)?
    {
        Some(mut existing) => {
            for tag in &tags {
                if !existing.tags.contains(tag) {
                    existing.tags.push(tag.clone());
                }
            }
            if existing.root_note.is_none() {
                existing.root_note = root_note.map(ToString::to_string);
            }
            if existing.scale_name.is_none() {
                existing.scale_name = scale_name.map(ToString::to_string);
            }
            existing.updated_at = now.to_string();
            existing
        }
        None => LibraryItem {
            item_id: ids::new_id("item"),
            display_name: decoded.display_name,
            source_kind,
            source_label,
            source_path: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            tags: tags.clone(),
            favorite: false,
            archived: false,
            slot_key,
            snapshot_id,
            snapshot_name,
            format: Some("multipattern".to_string()),
            scale_name: scale_name.map(ToString::to_string),
            root_note: root_note.map(ToString::to_string),
            duplicate_status: DuplicateStatus::Unique,
            related_group_count: 0,
            analysis_status: AnalysisStatus::Unknown,
            notes: None,
            content_hash: Some(decoded.content_hash.clone()),
        },
    };

    state
        .library
        .store
        .write_pattern_bytes(&item.item_id, &decoded.payload)
        .map_err(AppError::Midi)?;

    item = state
        .library
        .store
        .upsert_item(item)
        .map_err(AppError::Midi)?;
    for tag in tags {
        if let Err(e) = state.library.store.add_tag_to_item(&item.item_id, &tag) {
            eprintln!("[bank] warn: tag attach failed for {}: {}", item.item_id, e);
        }
    }
    state
        .library
        .store
        .get_item(&item.item_id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| {
            AppError::BadRequest(format!("item '{}' not found after save", item.item_id))
        })
}

/// Load the padded 64-slot view for `id`. The store already pads missing
/// slots; this helper defensively re-pads to 64 entries in canonical order
/// so store regressions cannot silently return a shorter grid.
pub(super) fn load_slot_views(
    state: &Arc<AppState>,
    id: &str,
) -> Result<Vec<SnapshotSlotView>, AppError> {
    let raw = state
        .library
        .store
        .list_snapshot_slots(id)
        .map_err(AppError::Midi)?;

    let mut by_key: std::collections::HashMap<String, crate::library::model::SnapshotSlot> =
        std::collections::HashMap::with_capacity(64);
    for slot in raw {
        by_key.insert(slot.slot_key.clone(), slot);
    }

    let mut views: Vec<SnapshotSlotView> = Vec::with_capacity(64);
    for g in 1..=4u8 {
        for p in 1..=8u8 {
            for side in ['A', 'B'] {
                let key = format!("G{}-P{}{}", g, p, side);
                if let Some(slot) = by_key.remove(&key) {
                    views.push(SnapshotSlotView::from_slot(slot));
                } else {
                    views.push(SnapshotSlotView {
                        slot_key: key,
                        empty: true,
                        item_id: None,
                        display_name: None,
                        changed: None,
                        duplicate: None,
                    });
                }
            }
        }
    }
    Ok(views)
}

pub(super) fn require_snapshot_exists(
    state: &Arc<AppState>,
    snapshot_id: &str,
) -> Result<(), AppError> {
    state
        .library
        .store
        .get_snapshot(snapshot_id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", snapshot_id)))?;
    Ok(())
}

/// Load a Pattern for `item_id` from the sidecar cache. Returns `Err(String)`
/// with a user-readable reason when the sidecar is missing or corrupt - the
/// handler surfaces this as a 400.
pub(super) fn load_pattern(state: &Arc<AppState>, item_id: &str) -> Result<Pattern, String> {
    let Some(payload) = state.library.store.pattern_bytes_for(item_id) else {
        return Err(format!(
            "item '{}' has no cached pattern payload - re-ingest the source",
            item_id
        ));
    };
    if payload.len() != 112 {
        return Err(format!(
            "item '{}' sidecar is {} bytes, expected 112",
            item_id,
            payload.len()
        ));
    }
    let mut sysex = Vec::with_capacity(115);
    sysex.push(0x78);
    sysex.push(0x00);
    sysex.push(0x00);
    sysex.extend_from_slice(&payload);
    sysex_to_pattern(&sysex).map_err(|e| format!("item '{}' decode: {}", item_id, e))
}

/// Soft-fail pattern resolver for snapshot compares. Returns `None` for any
/// failure so a missing sidecar simply falls back to structural comparison
/// (slot-level identity only).
pub(super) fn resolve_pattern(
    store: &crate::library::LibraryStore,
    item_id: &str,
) -> Option<Pattern> {
    let payload = store.pattern_bytes_for(item_id)?;
    if payload.len() != 112 {
        return None;
    }
    let mut sysex = Vec::with_capacity(115);
    sysex.push(0x78);
    sysex.push(0x00);
    sysex.push(0x00);
    sysex.extend_from_slice(&payload);
    sysex_to_pattern(&sysex).ok()
}
