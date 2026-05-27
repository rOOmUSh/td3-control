use super::*;

// ---------------------------------------------------------------------------
// POST /api/pattern/audition
// ---------------------------------------------------------------------------
//
// Host-sequenced, non-saving pattern audition. Encodes the supplied pattern
// into a timed Note On/Off schedule and plays it from a dedicated thread that
// owns the MIDI output connection. No MIDI Start (0xFA) is sent and the
// scratch slot is never written, so the device sequencer stays idle and device
// pattern memory is untouched. Contrast `pattern_play_preview`, which uploads
// to the scratch slot and starts the device clock.

pub async fn pattern_audition(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PatternAuditionRequest>, JsonRejection>,
) -> Result<Json<PatternAuditionResponse>, AppError> {
    let req = json_payload(payload, "pattern audition")?;
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
    let schedule = clock::prepare_schedule(&pattern, centibpm).map_err(AppError::Midi)?;
    let looping = req.looping;
    let (_, start_delay) =
        super::super::super::start_schedule::resolve_start_target(req.target_epoch_micros)
            .map_err(AppError::BadRequest)?;

    // Release the output port from any running clock or prior audition so
    // we can take it for this audition. Both own `session.out_conn`
    // exclusively, so they must be torn down first.
    stop_clock(&state).await;
    stop_audition(&state).await;

    let out_conn = {
        let mut guard = state.midi.session.lock().await;
        let session = guard
            .as_mut()
            .ok_or(AppError::BadRequest("not connected".into()))?;
        session.out_conn.take().ok_or(AppError::BadRequest(
            "transport already running - stop it first".into(),
        ))?
    };

    let runner = clock::AuditionRunner::spawn_scheduled(out_conn, schedule, looping, start_delay)
        .map_err(AppError::Midi)?;

    *state.playback.audition.lock().await = Some(runner);

    // A host audition is not a Bank item playing on the device.
    *state.playback.playing_item_id.lock().await = None;

    Ok(Json(PatternAuditionResponse {
        ok: true,
        bpm: centibpm / 100,
        centibpm,
        looping,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/pattern/audition/stop
// ---------------------------------------------------------------------------

pub async fn pattern_audition_stop(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PatternAuditionResponse>, AppError> {
    // The audition thread silences sounding notes (explicit Note Off plus
    // All Notes Off) as part of its shutdown, so no separate silence call
    // is needed here.
    stop_audition(&state).await;

    Ok(Json(PatternAuditionResponse {
        ok: true,
        bpm: 0,
        centibpm: 0,
        looping: false,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/pattern/audition/update
// ---------------------------------------------------------------------------

pub async fn pattern_audition_update(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PatternAuditionRequest>, JsonRejection>,
) -> Result<Json<PatternAuditionResponse>, AppError> {
    let req = json_payload(payload, "pattern audition update")?;
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
    let schedule = clock::prepare_schedule(&pattern, centibpm).map_err(AppError::Midi)?;

    {
        let guard = state.playback.audition.lock().await;
        let runner = guard
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("audition is not running".into()))?;
        runner.update_schedule(schedule).map_err(AppError::Midi)?;
    }

    Ok(Json(PatternAuditionResponse {
        ok: true,
        bpm: centibpm / 100,
        centibpm,
        looping: req.looping,
    }))
}
