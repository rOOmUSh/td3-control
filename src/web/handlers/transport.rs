use super::*;

pub async fn transport_start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BpmRequest>,
) -> Result<Json<TransportResponse>, AppError> {
    let centibpm = req
        .resolve_centibpm()
        .ok_or_else(|| AppError::BadRequest("BPM must be supplied".into()))?;
    if centibpm == 0 || centibpm > 30_000 {
        return Err(AppError::BadRequest(format!(
            "centi-BPM must be 1-30000 (0.01-300.00 BPM), got {}",
            centibpm
        )));
    }

    let (start_epoch_micros, start_delay) =
        super::super::start_schedule::resolve_start_target(req.target_epoch_micros)
            .map_err(AppError::BadRequest)?;

    // Stop any existing clock or host audition so their threads release
    // the MIDI port before we open a fresh output connection.
    stop_clock(&state).await;
    stop_audition(&state).await;

    let started_at_epoch_ms = start_epoch_micros / 1_000;
    let transport_id = next_transport_id(&state);
    let runner = spawn_clock_runner(&state, centibpm, start_delay).await?;

    let mut clock_guard = state.playback.clock.lock().await;
    *clock_guard = Some(ClockState {
        centibpm,
        started_at_epoch_ms,
        transport_id,
        playing: true,
        runner: Some(runner),
    });

    // The legacy transport doesn't know which LibraryItem (if any) is sitting
    // in the scratch slot, so clear the Bank audition tracker - anything the
    // Bank UI was showing as "playing" is no longer authoritative.
    *state.playback.playing_item_id.lock().await = None;

    Ok(Json(TransportResponse {
        ok: true,
        started_at_epoch_ms,
        transport_id,
        ppqn: clock::PPQN,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/transport/stop
// ---------------------------------------------------------------------------

pub async fn transport_stop(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TransportResponse>, AppError> {
    // The clock thread emits MIDI Stop (0xFC) on its own as part of
    // the shutdown sequence, so no separate `send_stop` call is needed.
    stop_clock(&state).await;
    stop_audition(&state).await;

    // Clear the Bank UI audition tracker so every play button renders its
    // idle state again. Without this, stopping from the legacy transport bar
    // would leave the Bank UI showing "stop" on a stale item.
    *state.playback.playing_item_id.lock().await = None;

    Ok(Json(TransportResponse {
        ok: true,
        started_at_epoch_ms: 0,
        transport_id: 0,
        ppqn: clock::PPQN,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/transport/bpm
// ---------------------------------------------------------------------------

pub async fn transport_bpm(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BpmRequest>,
) -> Result<Json<TransportResponse>, AppError> {
    let centibpm = req
        .resolve_centibpm()
        .ok_or_else(|| AppError::BadRequest("BPM must be supplied".into()))?;
    if centibpm == 0 || centibpm > 30_000 {
        return Err(AppError::BadRequest(format!(
            "centi-BPM must be 1-30000 (0.01-300.00 BPM), got {}",
            centibpm
        )));
    }

    let mut clock_guard = state.playback.clock.lock().await;
    if let Some(ref mut clock) = *clock_guard {
        clock.centibpm = centibpm;
        if let Some(runner) = clock.runner.as_ref() {
            runner.set_centibpm(centibpm);
        }
    }

    let (started_at_epoch_ms, transport_id) = clock_guard
        .as_ref()
        .map(|clock| (clock.started_at_epoch_ms, clock.transport_id))
        .unwrap_or((0, 0));

    Ok(Json(TransportResponse {
        ok: true,
        started_at_epoch_ms,
        transport_id,
        ppqn: clock::PPQN,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/transport/wrap-pulse
// ---------------------------------------------------------------------------

pub async fn transport_wrap_pulse(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TransportWrapPulseRequest>,
) -> Result<Json<TransportWrapPulseResponse>, AppError> {
    if req.active_steps == 0 || req.active_steps > 16 {
        return Err(AppError::BadRequest(format!(
            "active_steps must be 1-16, got {}",
            req.active_steps
        )));
    }

    let (centibpm, started_at_epoch_ms, transport_id) = current_transport_sync(&state).await?;
    if transport_id != req.transport_id {
        return Err(AppError::BadRequest("stale transport sync request".into()));
    }

    let anchor = req.anchor_epoch_ms.max(started_at_epoch_ms);
    let wrap_duration = clock::pattern_wrap_duration(centibpm, req.active_steps, req.triplet);
    let wrap_epoch_ms = anchor.saturating_add(wrap_duration.as_millis() as u64);
    let now = current_epoch_millis();
    if wrap_epoch_ms > now {
        tokio::time::sleep(Duration::from_millis(wrap_epoch_ms - now)).await;
    }

    let (_, _, latest_transport_id) = match current_transport_sync(&state).await {
        Ok(sync) => sync,
        Err(AppError::BadRequest(_)) => {
            return Ok(transport_wrap_pulse_inactive_response(&req, wrap_epoch_ms));
        }
        Err(err) => return Err(err),
    };
    if latest_transport_id != req.transport_id {
        return Ok(transport_wrap_pulse_inactive_response(&req, wrap_epoch_ms));
    }

    Ok(Json(TransportWrapPulseResponse {
        ok: true,
        transport_id,
        wrap_index: req.wrap_index.saturating_add(1),
        wrap_epoch_ms,
        server_epoch_ms: current_epoch_millis(),
        ppqn: clock::PPQN,
    }))
}

fn transport_wrap_pulse_inactive_response(
    req: &TransportWrapPulseRequest,
    wrap_epoch_ms: u64,
) -> Json<TransportWrapPulseResponse> {
    Json(TransportWrapPulseResponse {
        ok: false,
        transport_id: req.transport_id,
        wrap_index: req.wrap_index,
        wrap_epoch_ms,
        server_epoch_ms: current_epoch_millis(),
        ppqn: clock::PPQN,
    })
}

pub(crate) fn current_epoch_millis_for_clock() -> u64 {
    super::super::start_schedule::current_epoch_millis()
}

pub(super) fn current_epoch_millis() -> u64 {
    current_epoch_millis_for_clock()
}

pub(crate) fn next_transport_id(state: &Arc<AppState>) -> u64 {
    state
        .playback
        .transport_generation
        .fetch_add(1, Ordering::AcqRel)
}

async fn current_transport_sync(state: &Arc<AppState>) -> Result<(u32, u64, u64), AppError> {
    let clock_guard = state.playback.clock.lock().await;
    let clock = clock_guard
        .as_ref()
        .ok_or(AppError::BadRequest("transport is not running".into()))?;
    if !clock.playing {
        return Err(AppError::BadRequest("transport is not playing".into()));
    }
    Ok((
        clock.centibpm,
        clock.started_at_epoch_ms,
        clock.transport_id,
    ))
}

/// Stop any running clock thread and return the `MidiOutputConnection`
/// it was using back into the session. After this returns the session's
/// `out_conn` is populated (assuming the session still exists) and
/// SysEx paths are unblocked again.
pub(crate) async fn stop_clock(state: &Arc<AppState>) {
    // Take the ClockState out of the Mutex so the thread join below
    // happens *without* holding `state.playback.clock`. Holding it across the
    // blocking join would serialize every other endpoint that checks
    // clock state (e.g. `/api/status`).
    let clock_state = {
        let mut clock_guard = state.playback.clock.lock().await;
        clock_guard.take()
    };

    let Some(mut clock) = clock_state else { return };
    clock.playing = false;
    let Some(runner) = clock.runner.take() else {
        return;
    };

    // `stop()` signals the thread, waits for it to emit MIDI Stop
    // (0xFC), and joins. Use `spawn_blocking` so the tokio worker
    // isn't parked on the OS-thread join.
    let out_conn = tokio::task::spawn_blocking(move || runner.stop())
        .await
        .ok()
        .flatten();

    // Put the connection back into the session so the next SysEx
    // operation can use it. If the session is gone (parallel
    // disconnect) the connection drops here and the port closes -
    // that matches what disconnect wanted anyway.
    if let Some(out_conn) = out_conn {
        let mut guard = state.midi.session.lock().await;
        if let Some(session) = guard.as_mut() {
            session.out_conn = Some(out_conn);
        }
    }
}

/// Stop any running host-audition thread and return the
/// `MidiOutputConnection` it was using back into the session. The
/// audition thread silences sounding notes on its way out. After this
/// returns the session's `out_conn` is populated again (assuming the
/// session still exists). Safe to call when no audition is running.
pub(crate) async fn stop_audition(state: &Arc<AppState>) {
    // Take the runner out of the Mutex so the blocking join below does
    // not hold `state.playback.audition` across the wait.
    let runner = {
        let mut guard = state.playback.audition.lock().await;
        guard.take()
    };

    let Some(runner) = runner else {
        return;
    };

    // `stop()` signals the thread, waits for it to silence sounding
    // notes, and joins. Use `spawn_blocking` so the tokio worker isn't
    // parked on the OS-thread join.
    let out_conn = tokio::task::spawn_blocking(move || runner.stop())
        .await
        .ok()
        .flatten();

    if let Some(out_conn) = out_conn {
        let mut guard = state.midi.session.lock().await;
        if let Some(session) = guard.as_mut() {
            session.out_conn = Some(out_conn);
        }
    }
}

/// Pick the right SysEx transport for the current session/clock state
/// and hand it to `f`.
///
/// - **Idle** (no clock thread running): the session owns the
///   `MidiOutputConnection` directly; the closure gets it.
/// - **Playing** (clock thread holds the port): the session's
///   `out_conn` slot is `None`. The closure is given the
///   `ClockRunner` instead, which queues the bytes for the clock
///   thread to forward between ticks. This is what keeps the
///   progression feature working mid-playback (it calls
///   `POST /api/pattern/save` to pre-load the next pattern 8 steps
///   before the device wraps).
///
/// The caller must hold the session and clock guards in that order
///   - matching every other site that locks both. Do not hold either
///     across an `.await`; the closure is synchronous.
pub(crate) fn with_sender<F, R>(
    session: &mut MidiSession,
    clock_state: Option<&mut ClockState>,
    f: F,
) -> Result<R, Td3Error>
where
    F: FnOnce(&mut dyn SysexSender, &std::sync::mpsc::Receiver<Vec<u8>>) -> Result<R, Td3Error>,
{
    // Split borrow: `session.out_conn` and `session.rx` are different
    // fields, so the mutable `out_conn` borrow and the shared `rx`
    // borrow coexist inside each match arm.
    match session.out_conn.as_mut() {
        Some(out_conn) => f(out_conn, &session.rx),
        None => {
            let runner = clock_state.and_then(|c| c.runner.as_mut()).ok_or_else(|| {
                Td3Error::Midi(
                    "MIDI output unavailable (clock thread missing during playback)".to_string(),
                )
            })?;
            f(runner, &session.rx)
        }
    }
}

/// Take the session's `MidiOutputConnection`, hand it to a fresh
/// clock runner, and return the runner. On success the session's
/// `out_conn` is `None` until `stop_clock` puts it back.
pub(crate) async fn spawn_clock_runner(
    state: &Arc<AppState>,
    centibpm: u32,
    start_delay: Duration,
) -> Result<clock::ClockRunner, AppError> {
    let out_conn = {
        let mut guard = state.midi.session.lock().await;
        let session = guard
            .as_mut()
            .ok_or(AppError::BadRequest("not connected".into()))?;
        session.out_conn.take().ok_or(AppError::BadRequest(
            "transport already running - stop it first".into(),
        ))?
    };

    clock::ClockRunner::spawn_scheduled(out_conn, centibpm, start_delay).map_err(AppError::Midi)
}

// ---------------------------------------------------------------------------
// GET /api/scratch-pattern
// ---------------------------------------------------------------------------
