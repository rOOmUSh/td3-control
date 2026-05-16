use super::shared::*;
use super::*;

pub(super) async fn compare_items_route(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ItemCompareQuery>,
) -> Result<Json<ItemCompareResponse>, AppError> {
    // Validate both items exist - a missing id is a 400 to give callers a
    // clear signal rather than a misleading "empty" report.
    let _a = state
        .library
        .store
        .get_item(&q.a)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("item '{}' not found", q.a)))?;
    let _b = state
        .library
        .store
        .get_item(&q.b)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("item '{}' not found", q.b)))?;

    let pat_a = load_pattern(&state, &q.a).map_err(AppError::BadRequest)?;
    let pat_b = load_pattern(&state, &q.b).map_err(AppError::BadRequest)?;

    let report = lib_compare_items(&pat_a, &pat_b);
    Ok(Json(ItemCompareResponse { report }))
}

pub(super) async fn compare_snapshots_route(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SnapshotCompareQuery>,
) -> Result<Json<SnapshotCompareResponse>, AppError> {
    require_snapshot_exists(&state, &q.src)?;
    require_snapshot_exists(&state, &q.dst)?;
    let src = state
        .library
        .store
        .list_snapshot_slots(&q.src)
        .map_err(AppError::Midi)?;
    let dst = state
        .library
        .store
        .list_snapshot_slots(&q.dst)
        .map_err(AppError::Midi)?;
    let store = state.library.store.clone();
    let report = compare_snapshots(&src, &dst, move |id| resolve_pattern(&store, id));
    Ok(Json(SnapshotCompareResponse { report }))
}

pub(super) async fn merge_plan_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergePlanRequest>,
) -> Result<Json<MergePlanResponse>, AppError> {
    let plan = run_merge_plan(&state, &req)?;
    Ok(Json(MergePlanResponse {
        plan,
        preview: false,
    }))
}

/// Same calculation as `/api/bank/merge-plan`, but the response is flagged
/// `preview: true` so the UI can wire a confirmation step on top.
/// The preview must be side-effect-free (no device write, no catalog write),
/// matching `merge_plan_route` exactly.
pub(super) async fn merge_plan_preview_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergePlanRequest>,
) -> Result<Json<MergePlanResponse>, AppError> {
    let plan = run_merge_plan(&state, &req)?;
    Ok(Json(MergePlanResponse {
        plan,
        preview: true,
    }))
}

/// Shared body between `/merge-plan` and `/merge-plan/preview`.
fn run_merge_plan(
    state: &Arc<AppState>,
    req: &MergePlanRequest,
) -> Result<crate::library::MergePlan, AppError> {
    require_snapshot_exists(state, &req.source_snapshot_id)?;
    require_snapshot_exists(state, &req.target_snapshot_id)?;
    let src = state
        .library
        .store
        .list_snapshot_slots(&req.source_snapshot_id)
        .map_err(AppError::Midi)?;
    let dst = state
        .library
        .store
        .list_snapshot_slots(&req.target_snapshot_id)
        .map_err(AppError::Midi)?;
    let store = state.library.store.clone();
    let report = compare_snapshots(&src, &dst, move |id| resolve_pattern(&store, id));
    Ok(build_merge_plan(
        &req.source_snapshot_id,
        &req.target_snapshot_id,
        &report,
        &req.selection,
    ))
}
