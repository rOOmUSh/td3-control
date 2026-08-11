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
    let gate_percent = validate_gate_percent(req.gate_percent)?;
    let morph_amount = resolve_morph_amount(&req)?;

    let channel = resolve_midi_channel(req.midi_channel, state.midi.runtime.device_channel)?;

    let pattern = web_to_pattern(&req.pattern)?;
    let (schedule, morph_plan) =
        build_audition_schedule(&pattern, centibpm, gate_percent, morph_amount, channel)?;
    let looping = req.looping;
    let (target_epoch_micros, _) =
        super::super::super::start_schedule::resolve_start_target(req.target_epoch_micros)
            .map_err(AppError::BadRequest)?;

    // Release the output port from any running clock or prior audition so
    // we can take it for this audition. Both own `session.out_conn`
    // exclusively, so they must be torn down first.
    stop_clock(&state).await;
    stop_audition(&state).await;

    let lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    let (out_conn, session_generation) = {
        let mut guard = state.midi.session.lock().await;
        let session = guard
            .as_mut()
            .ok_or(AppError::BadRequest("not connected".into()))?;
        let generation = session.generation;
        let out_conn = session.out_conn.take().ok_or(AppError::BadRequest(
            "transport already running - stop it first".into(),
        ))?;
        (out_conn, generation)
    };

    let (runner, start_rx) = match clock::AuditionRunner::spawn_scheduled(
        out_conn,
        schedule,
        looping,
        target_epoch_micros,
        centibpm,
    ) {
        Ok(spawned) => spawned,
        Err(err) => {
            invalidate_midi_session_generation(&state, session_generation).await;
            return Err(audition_update_error(
                clock::AuditionUpdateError::PlaybackFailed(format!(
                    "{}; MIDI connection closed, reconnect before retrying",
                    err
                )),
            ));
        }
    };

    let audition_id = state
        .playback
        .transport_generation
        .fetch_add(1, Ordering::AcqRel);
    *state.playback.audition.lock().await = Some(AuditionState {
        session_generation,
        audition_id,
        looping,
        runner,
    });
    drop(lifecycle);
    monitor_audition_completion(Arc::clone(&state), audition_id);

    // A host audition is not a Bank item playing on the device.
    *state.playback.playing_item_id.lock().await = None;

    let remaining_delay = Duration::from_micros(
        target_epoch_micros
            .saturating_sub(super::super::super::start_schedule::current_epoch_micros()),
    );
    let start_wait_timeout = remaining_delay.saturating_add(Duration::from_secs(2));
    let start_result =
        tokio::task::spawn_blocking(move || start_rx.recv_timeout(start_wait_timeout)).await;
    let start_result = match start_result {
        Ok(result) => result,
        Err(err) => {
            stop_audition_detached_if_id(&state, audition_id).await;
            return Err(AppError::Internal(format!(
                "audition start wait task failed: {}",
                err
            )));
        }
    };
    let start_acknowledgement = match start_result {
        Ok(result) => match result {
            Ok(acknowledgement) => acknowledgement,
            Err(error) => {
                stop_audition_if_id(&state, audition_id).await;
                return Err(audition_update_error(error));
            }
        },
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            stop_audition_detached_if_id(&state, audition_id).await;
            return Err(audition_update_error(
                clock::AuditionUpdateError::PlaybackFailed(
                    "initial audition dispatch timed out".to_string(),
                ),
            ));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            stop_audition_if_id(&state, audition_id).await;
            return Err(audition_update_error(
                clock::AuditionUpdateError::PlaybackFailed(
                    "audition thread exited before initial dispatch".to_string(),
                ),
            ));
        }
    };

    Ok(Json(PatternAuditionResponse {
        ok: true,
        bpm: centibpm / 100,
        centibpm,
        looping,
        schedule_generation: Some(0),
        effective_at_epoch_micros: Some(start_acknowledgement.effective_at_epoch_micros),
        cycle_epoch_micros: Some(start_acknowledgement.cycle_epoch_micros),
        cycle_period_micros: Some(start_acknowledgement.cycle_period_micros),
        phase_micros: Some(start_acknowledgement.phase_micros),
        triplet_morph: morph_diagnostics(morph_amount, morph_plan.as_ref(), &start_acknowledgement),
    }))
}

fn monitor_audition_completion(state: Arc<AppState>, audition_id: u64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let finished = {
                let guard = state.playback.audition.lock().await;
                match guard.as_ref() {
                    Some(audition) if audition.audition_id == audition_id => {
                        audition.runner.is_finished()
                    }
                    _ => return,
                }
            };
            if finished {
                stop_audition_if_id(&state, audition_id).await;
                return;
            }
        }
    });
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
        schedule_generation: None,
        effective_at_epoch_micros: None,
        cycle_epoch_micros: None,
        cycle_period_micros: None,
        phase_micros: None,
        triplet_morph: None,
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
    let gate_percent = validate_gate_percent(req.gate_percent)?;
    let morph_amount = resolve_morph_amount(&req)?;

    let channel = resolve_midi_channel(req.midi_channel, state.midi.runtime.device_channel)?;

    let pattern = web_to_pattern(&req.pattern)?;
    let (schedule, morph_plan) =
        build_audition_schedule(&pattern, centibpm, gate_percent, morph_amount, channel)?;

    let (acknowledgement_rx, looping) = {
        let guard = state.playback.audition.lock().await;
        let runner = guard
            .as_ref()
            .ok_or_else(|| audition_update_error(clock::AuditionUpdateError::AuditionStopped))?;
        let acknowledgement_rx = runner
            .runner
            .update_schedule(schedule, centibpm, req.expected_schedule_generation)
            .map_err(AppError::Midi)?;
        (acknowledgement_rx, runner.looping)
    };

    let acknowledgement = tokio::task::spawn_blocking(move || acknowledgement_rx.recv())
        .await
        .map_err(|err| AppError::Internal(format!("audition update wait task failed: {}", err)))?
        .map_err(|_| audition_update_error(clock::AuditionUpdateError::AuditionStopped))?
        .map_err(audition_update_error)?;

    Ok(Json(PatternAuditionResponse {
        ok: true,
        bpm: acknowledgement.centibpm / 100,
        centibpm: acknowledgement.centibpm,
        looping,
        schedule_generation: Some(acknowledgement.schedule_generation),
        effective_at_epoch_micros: Some(acknowledgement.effective_at_epoch_micros),
        cycle_epoch_micros: Some(acknowledgement.cycle_epoch_micros),
        cycle_period_micros: Some(acknowledgement.cycle_period_micros),
        phase_micros: Some(acknowledgement.phase_micros),
        triplet_morph: morph_diagnostics(morph_amount, morph_plan.as_ref(), &acknowledgement),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/pattern/audition/queue-next-cycle
// ---------------------------------------------------------------------------

pub async fn pattern_audition_queue_next_cycle(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PatternAuditionRequest>, JsonRejection>,
) -> Result<Json<PatternAuditionResponse>, AppError> {
    let req = json_payload(payload, "pattern audition next-cycle update")?;
    let centibpm = req
        .resolve_centibpm()
        .unwrap_or_else(|| state.config.ui_config.ui_default_bpm.saturating_mul(100));
    if centibpm == 0 || centibpm > 30_000 {
        return Err(AppError::BadRequest(format!(
            "centi-BPM must be 1-30000 (0.01-300.00 BPM), got {}",
            centibpm
        )));
    }
    let gate_percent = validate_gate_percent(req.gate_percent)?;
    let morph_amount = resolve_morph_amount(&req)?;

    let channel = resolve_midi_channel(req.midi_channel, state.midi.runtime.device_channel)?;

    let pattern = web_to_pattern(&req.pattern)?;
    let (schedule, morph_plan) =
        build_audition_schedule(&pattern, centibpm, gate_percent, morph_amount, channel)?;

    let (acknowledgement_rx, looping) = {
        let guard = state.playback.audition.lock().await;
        let runner = guard
            .as_ref()
            .ok_or_else(|| audition_update_error(clock::AuditionUpdateError::AuditionStopped))?;
        let acknowledgement_rx = runner
            .runner
            .queue_next_cycle(schedule, centibpm, req.expected_schedule_generation)
            .map_err(AppError::Midi)?;
        (acknowledgement_rx, runner.looping)
    };

    let acknowledgement = tokio::task::spawn_blocking(move || acknowledgement_rx.recv())
        .await
        .map_err(|err| {
            AppError::Internal(format!("audition transition wait task failed: {}", err))
        })?
        .map_err(|_| audition_update_error(clock::AuditionUpdateError::AuditionStopped))?
        .map_err(audition_update_error)?;

    Ok(Json(PatternAuditionResponse {
        ok: true,
        bpm: acknowledgement.centibpm / 100,
        centibpm: acknowledgement.centibpm,
        looping,
        schedule_generation: Some(acknowledgement.schedule_generation),
        effective_at_epoch_micros: Some(acknowledgement.effective_at_epoch_micros),
        cycle_epoch_micros: Some(acknowledgement.cycle_epoch_micros),
        cycle_period_micros: Some(acknowledgement.cycle_period_micros),
        phase_micros: Some(acknowledgement.phase_micros),
        triplet_morph: morph_diagnostics(morph_amount, morph_plan.as_ref(), &acknowledgement),
    }))
}

/// Validate the optional morph amount. Invalid values are rejected with
/// a stable HTTP 400 message before any device interaction.
fn resolve_morph_amount(
    req: &PatternAuditionRequest,
) -> Result<Option<crate::triplet_morph::MorphAmount>, AppError> {
    match req.triplet_morph_percent {
        None => Ok(None),
        Some(value) => crate::triplet_morph::MorphAmount::new(value)
            .map(Some)
            .map_err(|err| AppError::BadRequest(err.to_string())),
    }
}

/// Build the audition schedule. An omitted morph amount keeps the legacy
/// path, including native `pattern.triplet` audition. A present amount
/// requires an eligible 16-step straight source; ineligibility is a
/// client error, never a silent coercion.
fn build_audition_schedule(
    pattern: &crate::pattern::Pattern,
    centibpm: u32,
    gate_percent: u32,
    morph_amount: Option<crate::triplet_morph::MorphAmount>,
    channel: u8,
) -> Result<
    (
        clock::AuditionSchedule,
        Option<crate::triplet_morph::TripletMorphPlan>,
    ),
    AppError,
> {
    match morph_amount {
        None => {
            let schedule =
                clock::prepare_schedule_with_gate(pattern, centibpm, gate_percent, channel)
                    .map_err(AppError::Midi)?;
            Ok((schedule, None))
        }
        Some(amount) => {
            clock::prepare_morph_schedule(pattern, centibpm, gate_percent, amount, channel)
                .map(|(schedule, plan)| (schedule, Some(plan)))
                .map_err(|err| match err {
                    Td3Error::TripletMorph(morph_err) => {
                        AppError::BadRequest(morph_err.to_string())
                    }
                    other => AppError::Midi(other),
                })
        }
    }
}

/// Morph response diagnostics derived from the runner acknowledgement.
/// `tripletMorphFullyAppliedEpochMicros` is the epoch of the first cycle
/// fully governed by the requested amount: the acknowledged cycle for a
/// boundary install, or the following cycle for an in-place install.
fn morph_diagnostics(
    morph_amount: Option<crate::triplet_morph::MorphAmount>,
    plan: Option<&crate::triplet_morph::TripletMorphPlan>,
    acknowledgement: &clock::AuditionUpdateAck,
) -> Option<TripletMorphDiagnostics> {
    let (amount, plan) = match (morph_amount, plan) {
        (Some(amount), Some(plan)) => (amount, plan),
        _ => return None,
    };
    let (apply_mode, fully_applied_epoch_micros) = match acknowledgement.apply_mode {
        clock::AuditionApplyMode::NextCycle => ("nextCycle", acknowledgement.cycle_epoch_micros),
        clock::AuditionApplyMode::CurrentCycleFuture => (
            "currentCycleFuture",
            acknowledgement
                .cycle_epoch_micros
                .saturating_add(acknowledgement.cycle_period_micros),
        ),
    };
    Some(TripletMorphDiagnostics {
        triplet_morph_percent: u32::from(amount.value()),
        triplet_morph_plan_version: plan.version,
        triplet_morph_apply_mode: apply_mode,
        triplet_morph_fully_applied_epoch_micros: fully_applied_epoch_micros,
    })
}

/// Resolve the MIDI channel to address the device on.
///
/// An explicit request value wins so the transport bar's CH selector
/// takes effect without a restart; omitted falls back to the channel
/// resolved from `MIDI_DEVICE_CHANNEL`. Out-of-range values are rejected
/// rather than clamped: a caller asking for channel 17 has a bug, and
/// silently playing on 16 would hide it.
fn resolve_midi_channel(requested: Option<u32>, configured: u8) -> Result<u8, AppError> {
    match requested {
        None => Ok(configured),
        Some(value) => {
            if !(1..=16).contains(&value) {
                return Err(AppError::BadRequest(format!(
                    "midi channel must be 1-16, got {}",
                    value
                )));
            }
            Ok(value as u8)
        }
    }
}

fn validate_gate_percent(gate_percent: u32) -> Result<u32, AppError> {
    if !(1..=100).contains(&gate_percent) {
        return Err(AppError::BadRequest(format!(
            "gate percent must be 1-100, got {}",
            gate_percent
        )));
    }
    Ok(gate_percent)
}

pub(crate) fn audition_update_error(error: clock::AuditionUpdateError) -> AppError {
    let (status, code, message) = match error {
        clock::AuditionUpdateError::GenerationConflict => (
            StatusCode::CONFLICT,
            "generation_conflict",
            "audition schedule generation changed before the update was applied".to_string(),
        ),
        clock::AuditionUpdateError::Superseded => (
            StatusCode::CONFLICT,
            "superseded",
            "audition update was superseded by a newer request".to_string(),
        ),
        clock::AuditionUpdateError::AuditionStopped => (
            StatusCode::CONFLICT,
            "audition_stopped",
            "audition stopped before the update was applied".to_string(),
        ),
        clock::AuditionUpdateError::PlaybackFailed(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "playback_failed",
            format!(
                "audition playback failed before the update was applied: {}",
                message
            ),
        ),
    };
    AppError::Coded {
        status,
        code,
        message,
    }
}
