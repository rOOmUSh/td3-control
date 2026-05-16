use super::*;

// ---------------------------------------------------------------------------
// Related groups + duplicates
// ---------------------------------------------------------------------------

pub(super) async fn list_related(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RelatedQuery>,
) -> Result<Json<RelatedGroupsResponse>, AppError> {
    let mut groups: Vec<RelatedGroup> =
        compute_related_groups(&state.library.store).map_err(AppError::Midi)?;
    // Optional kind filter - empty string is treated as "no filter" so the
    // UI can pass the value verbatim from a select element.
    if let Some(kind_str) = &q.kind {
        if !kind_str.trim().is_empty() {
            let kind = GroupKind::parse(kind_str).ok_or_else(|| {
                AppError::BadRequest(format!("unknown related group kind: '{}'", kind_str))
            })?;
            groups.retain(|g| g.kind == kind);
        }
    }
    let relations = state
        .library
        .store
        .list_pattern_relations()
        .map_err(AppError::Midi)?;
    Ok(Json(RelatedGroupsResponse { groups, relations }))
}

pub(super) async fn list_duplicates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DuplicatesResponse>, AppError> {
    let clusters = compute_clusters(&state.library.store).map_err(AppError::Midi)?;
    // Best-effort write-through of duplicate_status. A failure here should
    // not fail the handler - the status is a derived field.
    let all_items = state
        .library
        .store
        .list_items(&ItemFilter::default())
        .map_err(AppError::Midi)?;
    let all_ids: Vec<String> = all_items.iter().map(|i| i.item_id.clone()).collect();
    let decoded_ids: Vec<String> = all_ids
        .iter()
        .filter(|id| state.library.store.pattern_bytes_for(id).is_some())
        .cloned()
        .collect();
    let updates = statuses_from_clusters(&clusters, &all_ids, &decoded_ids);
    let _ = state.library.store.set_duplicate_statuses(&updates);
    Ok(Json(DuplicatesResponse { clusters }))
}

// ---------------------------------------------------------------------------
// Audition: play/stop a LibraryItem on the device from the Bank UI
// ---------------------------------------------------------------------------
//
// The Bank UI lets the user click a small play button on any surface that
// shows a single pattern (card, table row, drawer, snapshot slot, duplicate
// member, related representative, imported entry). `play_item` uploads the
// cached 112-byte payload to the configured scratch slot byte-for-byte via
// `upload_raw_payload` - no Pattern decode/re-encode round-trip - then kicks
// the transport so the pattern is audible immediately.
//
// Stop is not a separate endpoint: the UI calls the existing
