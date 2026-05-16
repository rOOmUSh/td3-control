use super::*;

// play-button across the Bank repaints back to its idle state.

pub(super) async fn play_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<PlayItemQuery>,
) -> Result<Json<PlayItemResponse>, AppError> {
    let centibpm = query
        .resolve_centibpm()
        .unwrap_or_else(|| state.config.ui_config.ui_default_bpm.saturating_mul(100));
    if centibpm == 0 || centibpm > 30_000 {
        return Err(AppError::BadRequest(format!(
            "centi-BPM must be 1-30000 (0.01-300.00 BPM), got {}",
            centibpm
        )));
    }

    let bytes = state.library.store.pattern_bytes_for(&id).ok_or_else(|| {
        AppError::BadRequest(format!(
            "item '{}' has no cached pattern payload - re-ingest the source",
            id
        ))
    })?;
    if bytes.len() != 112 {
        return Err(AppError::BadRequest(format!(
            "item '{}' sidecar is {} bytes, expected 112",
            id,
            bytes.len()
        )));
    }

    let scratch = state.midi.scratch;
    let slot_addr = scratch.slot + (scratch.side << 3);

    // Stop any currently-running clock before we retarget the scratch slot -
    // otherwise the device could emit ticks for the old pattern between our
    // upload and the fresh MIDI Start below.
    stop_clock(&state).await;

    // Upload the raw bytes. The session guard is held only for the blocking
    // MIDI exchange, then dropped before we touch the clock (which also
    // locks the session to emit Start/tick bytes).
    {
        let mut guard = state.midi.session.lock().await;
        let session = guard
            .as_mut()
            .ok_or(AppError::BadRequest("not connected".into()))?;
        let out_conn = session.out_conn.as_mut().ok_or(AppError::BadRequest(
            "transport is running - stop it first".into(),
        ))?;
        let rx = &session.rx;
        let timeout = state.midi.runtime.timeout;
        tokio::task::block_in_place(|| {
            td3_protocol::upload_raw_payload(
                out_conn,
                rx,
                scratch.patgroup,
                slot_addr,
                &bytes,
                timeout,
            )
        })?;
    }

    // Spawn the dedicated-thread clock runner. Its thread emits MIDI
    // Start (0xFA) on the connection it took from the session, then
    // ticks until stopped. Byte-compatible with the legacy
    // `/api/transport/start` path produces.
    let started_at_epoch_ms = crate::web::handlers::current_epoch_millis_for_clock();
    let transport_id = crate::web::handlers::next_transport_id(&state);
    let runner = spawn_clock_runner(&state, centibpm).await?;
    *state.playback.clock.lock().await = Some(ClockState {
        centibpm,
        started_at_epoch_ms,
        transport_id,
        playing: true,
        runner: Some(runner),
    });

    *state.playback.playing_item_id.lock().await = Some(id.clone());

    Ok(Json(PlayItemResponse {
        ok: true,
        item_id: id,
        bpm: centibpm / 100,
        centibpm,
    }))
}

pub(super) async fn get_playing(State(state): State<Arc<AppState>>) -> Json<PlayingItemResponse> {
    let guard = state.playback.playing_item_id.lock().await;
    Json(PlayingItemResponse {
        item_id: guard.clone(),
    })
}
