use super::*;

pub async fn pattern_load(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatternRequest>,
) -> Result<Json<PatternLoadResponse>, AppError> {
    let patgroup = validate_group(req.patgroup)?;
    let (slot, side) = validate_pattern(req.pattern, &req.side)?;

    let mut session_guard = state.midi.session.lock().await;
    let mut clock_guard = state.playback.clock.lock().await;
    let session = session_guard
        .as_mut()
        .ok_or(AppError::BadRequest("not connected".into()))?;

    let timeout = state.midi.runtime.timeout;
    let (_raw, pattern) = tokio::task::block_in_place(|| {
        with_sender(session, clock_guard.as_mut(), |sender, rx| {
            td3_protocol::download_pattern(sender, rx, patgroup, slot, side, timeout)
        })
    })?;

    let address = formats::format_address(patgroup, slot, side);
    let web = pattern_to_web(&pattern);

    Ok(Json(PatternLoadResponse {
        address,
        pattern: web,
    }))
}

pub async fn pattern_save(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PatternSaveRequest>, JsonRejection>,
) -> Result<Json<PatternSaveResponse>, AppError> {
    let req = json_payload(payload, "pattern save")?;
    let patgroup = validate_group(req.patgroup)?;
    let (slot, side) = validate_pattern(req.pattern, &req.side)?;
    let pattern = web_to_pattern(&req.data)?;

    let mut session_guard = state.midi.session.lock().await;
    let mut clock_guard = state.playback.clock.lock().await;
    let session = session_guard
        .as_mut()
        .ok_or(AppError::BadRequest("not connected".into()))?;

    let timeout = state.midi.runtime.timeout;
    tokio::task::block_in_place(|| {
        with_sender(session, clock_guard.as_mut(), |sender, rx| {
            td3_protocol::upload_pattern(sender, rx, &pattern, patgroup, slot, side, timeout)
        })
    })?;

    drop(session_guard);
    drop(clock_guard);

    *state.playback.playing_item_id.lock().await = None;

    let address = formats::format_address(patgroup, slot, side);
    Ok(Json(PatternSaveResponse {
        address,
        saved: true,
    }))
}
