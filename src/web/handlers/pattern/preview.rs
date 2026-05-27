use super::*;

pub async fn pattern_play_preview(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PatternPlayPreviewRequest>, JsonRejection>,
) -> Result<Json<PatternPlayPreviewResponse>, AppError> {
    let req = json_payload(payload, "pattern play preview")?;
    let centibpm = req
        .resolve_centibpm()
        .unwrap_or_else(|| state.config.ui_config.ui_default_bpm.saturating_mul(100));
    if centibpm == 0 || centibpm > 30_000 {
        return Err(AppError::BadRequest(format!(
            "centi-BPM must be 1-30000 (0.01-300.00 BPM), got {}",
            centibpm
        )));
    }

    let pattern = web_to_pattern(&req.pattern)?;
    let sysex = crate::pattern::pattern_to_sysex(&pattern, 0, 0, 0)?;
    if sysex.len() < 3 || sysex[3..].len() != 112 {
        return Err(AppError::Midi(Td3Error::Other(
            "unexpected sysex length from pattern_to_sysex".into(),
        )));
    }
    let payload = &sysex[3..];

    let scratch = state.midi.scratch;
    let slot_addr = scratch.slot + (scratch.side << 3);

    stop_clock(&state).await;
    stop_audition(&state).await;

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
                payload,
                timeout,
            )
        })?;
    }

    let started_at_epoch_ms = current_epoch_millis();
    let transport_id = next_transport_id(&state);
    let runner = spawn_clock_runner(&state, centibpm, Duration::ZERO).await?;
    *state.playback.clock.lock().await = Some(ClockState {
        centibpm,
        started_at_epoch_ms,
        transport_id,
        playing: true,
        runner: Some(runner),
    });

    *state.playback.playing_item_id.lock().await = None;

    Ok(Json(PatternPlayPreviewResponse {
        ok: true,
        bpm: centibpm / 100,
        centibpm,
    }))
}
