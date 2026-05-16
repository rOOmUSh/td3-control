use super::*;

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

pub(super) async fn list_tags(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TagsResponse>, AppError> {
    let tags = state.library.store.list_tags().map_err(AppError::Midi)?;
    Ok(Json(TagsResponse { tags }))
}

pub(super) async fn add_tag(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddTagRequest>,
) -> Result<Json<TagOpResponse>, AppError> {
    if req.label.trim().is_empty() {
        return Err(AppError::BadRequest("tag label must not be empty".into()));
    }
    state
        .library
        .store
        .add_tag_to_item(&id, &req.label)
        .map_err(AppError::Midi)?;
    Ok(Json(TagOpResponse { ok: true }))
}

pub(super) async fn remove_tag(
    State(state): State<Arc<AppState>>,
    Path((id, tag)): Path<(String, String)>,
) -> Result<Json<TagOpResponse>, AppError> {
    state
        .library
        .store
        .remove_tag_from_item(&id, &tag)
        .map_err(AppError::Midi)?;
    Ok(Json(TagOpResponse { ok: true }))
}

pub(super) async fn bulk_tag(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkTagRequest>,
) -> Result<Json<TagOpResponse>, AppError> {
    state
        .library
        .store
        .bulk_tag(&req.item_ids, &req.add, &req.remove)
        .map_err(AppError::Midi)?;
    Ok(Json(TagOpResponse { ok: true }))
}
