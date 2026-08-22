use super::*;

const FIRST_CLOCK_PULSE_TIMEOUT: Duration = Duration::from_secs(3);

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

    let scheduled_start_epoch_ms = start_epoch_micros / 1_000;
    let transport_id = next_transport_id(&state);
    let start_snapshot = start_clock_transport(
        &state,
        centibpm,
        scheduled_start_epoch_ms,
        transport_id,
        start_delay,
    )
    .await?;
    let started_at_epoch_ms = start_snapshot.epoch_micros / 1_000;

    // The legacy transport doesn't know which LibraryItem (if any) is sitting
    // in the scratch slot, so clear the Bank audition tracker - anything the
    // Bank UI was showing as "playing" is no longer authoritative.
    *state.playback.playing_item_id.lock().await = None;

    Ok(Json(TransportResponse {
        ok: true,
        started_at_epoch_ms,
        transport_id,
        ppqn: clock::PPQN,
        centibpm: Some(start_snapshot.centibpm),
        tempo_revision: Some(start_snapshot.tempo_revision),
        effective_pulse_index: Some(start_snapshot.pulse_index),
        effective_at_epoch_micros: Some(start_snapshot.epoch_micros),
        server_epoch_micros: Some(super::super::start_schedule::current_epoch_micros()),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/transport/stop
// ---------------------------------------------------------------------------

pub async fn transport_stop(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TransportResponse>, AppError> {
    // A clock thread that reached MIDI Start emits MIDI Stop (0xFC) on its own
    // during shutdown, so no separate `send_stop` call is needed.
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
        centibpm: None,
        tempo_revision: None,
        effective_pulse_index: None,
        effective_at_epoch_micros: None,
        server_epoch_micros: None,
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

    let (started_at_epoch_ms, transport_id, tempo_wait) = {
        let mut clock_guard = state.playback.clock.lock().await;
        match clock_guard.as_mut() {
            Some(clock) => {
                let tempo_wait = clock.runner.as_ref().map(|runner| {
                    let revision = runner.set_centibpm(centibpm);
                    (runner.pulse_monitor(), revision)
                });
                if tempo_wait.is_none() {
                    clock.centibpm = centibpm;
                }
                (clock.started_at_epoch_ms, clock.transport_id, tempo_wait)
            }
            None => (0, 0, None),
        }
    };

    let applied = if let Some((monitor, revision)) = tempo_wait {
        let snapshot =
            tokio::task::spawn_blocking(move || monitor.wait_for_tempo_revision(revision))
                .await
                .map_err(|err| AppError::Internal(format!("tempo wait task failed: {}", err)))?
                .ok_or_else(|| {
                    AppError::Conflict("transport stopped before tempo was applied".to_string())
                })?;

        let mut clock_guard = state.playback.clock.lock().await;
        if let Some(clock) = clock_guard.as_mut() {
            if clock.transport_id == transport_id {
                clock.centibpm = snapshot.centibpm;
            }
        }
        Some(snapshot)
    } else {
        None
    };

    Ok(Json(TransportResponse {
        ok: true,
        started_at_epoch_ms,
        transport_id,
        ppqn: clock::PPQN,
        centibpm: Some(applied.map_or(centibpm, |snapshot| snapshot.centibpm)),
        tempo_revision: applied.map(|snapshot| snapshot.tempo_revision),
        effective_pulse_index: applied.map(|snapshot| snapshot.pulse_index),
        effective_at_epoch_micros: applied.map(|snapshot| snapshot.epoch_micros),
        server_epoch_micros: Some(super::super::start_schedule::current_epoch_micros()),
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

    let (centibpm, started_at_epoch_ms, transport_id, pulse_monitor) =
        current_transport_sync(&state).await?;
    if transport_id != req.transport_id {
        return Err(AppError::BadRequest("stale transport sync request".into()));
    }

    if let (Some(anchor_pulse_index), Some(monitor)) = (req.anchor_pulse_index, pulse_monitor) {
        let cycle_pulses = clock::pattern_wrap_pulses(req.active_steps, req.triplet);
        let target_pulse = anchor_pulse_index.saturating_add(cycle_pulses);
        let latest_monitor = monitor.clone();
        let reached = tokio::task::spawn_blocking(move || monitor.wait_for_pulse(target_pulse))
            .await
            .map_err(|err| AppError::Internal(format!("pulse wait task failed: {}", err)))?;
        let (snapshot, exact_boundary, wrap_index) = if let Some(snapshot) = reached {
            (snapshot, true, req.wrap_index.saturating_add(1))
        } else if let Some(snapshot) = latest_monitor.latest_running_pulse() {
            (snapshot, false, req.wrap_index.saturating_add(1))
        } else {
            return Ok(transport_wrap_pulse_inactive_response(
                &req,
                current_epoch_millis(),
                None,
            ));
        };

        let (_, _, latest_transport_id, _) = match current_transport_sync(&state).await {
            Ok(sync) => sync,
            Err(AppError::BadRequest(_)) => {
                return Ok(transport_wrap_pulse_inactive_response(
                    &req,
                    snapshot.epoch_micros / 1_000,
                    Some(snapshot.pulse_index),
                ));
            }
            Err(err) => return Err(err),
        };
        if latest_transport_id != req.transport_id {
            return Ok(transport_wrap_pulse_inactive_response(
                &req,
                snapshot.epoch_micros / 1_000,
                Some(snapshot.pulse_index),
            ));
        }

        return Ok(Json(TransportWrapPulseResponse {
            ok: true,
            exact_boundary: Some(exact_boundary),
            transport_id,
            wrap_index,
            wrap_epoch_ms: snapshot.epoch_micros / 1_000,
            wrap_epoch_micros: Some(snapshot.epoch_micros),
            server_epoch_ms: current_epoch_millis(),
            ppqn: clock::PPQN,
            pulse_index: Some(snapshot.pulse_index),
        }));
    }

    let anchor = req.anchor_epoch_ms.max(started_at_epoch_ms);
    let wrap_duration = clock::pattern_wrap_duration(centibpm, req.active_steps, req.triplet);
    let wrap_epoch_ms = anchor.saturating_add(wrap_duration.as_millis() as u64);
    let now = current_epoch_millis();
    if wrap_epoch_ms > now {
        tokio::time::sleep(Duration::from_millis(wrap_epoch_ms - now)).await;
    }

    let (_, _, latest_transport_id, _) = match current_transport_sync(&state).await {
        Ok(sync) => sync,
        Err(AppError::BadRequest(_)) => {
            return Ok(transport_wrap_pulse_inactive_response(
                &req,
                wrap_epoch_ms,
                None,
            ));
        }
        Err(err) => return Err(err),
    };
    if latest_transport_id != req.transport_id {
        return Ok(transport_wrap_pulse_inactive_response(
            &req,
            wrap_epoch_ms,
            None,
        ));
    }

    Ok(Json(TransportWrapPulseResponse {
        ok: true,
        exact_boundary: Some(true),
        transport_id,
        wrap_index: req.wrap_index.saturating_add(1),
        wrap_epoch_ms,
        wrap_epoch_micros: None,
        server_epoch_ms: current_epoch_millis(),
        ppqn: clock::PPQN,
        pulse_index: None,
    }))
}

fn transport_wrap_pulse_inactive_response(
    req: &TransportWrapPulseRequest,
    wrap_epoch_ms: u64,
    pulse_index: Option<u64>,
) -> Json<TransportWrapPulseResponse> {
    Json(TransportWrapPulseResponse {
        ok: false,
        exact_boundary: None,
        transport_id: req.transport_id,
        wrap_index: req.wrap_index,
        wrap_epoch_ms,
        wrap_epoch_micros: None,
        server_epoch_ms: current_epoch_millis(),
        ppqn: clock::PPQN,
        pulse_index,
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

async fn current_transport_sync(
    state: &Arc<AppState>,
) -> Result<(u32, u64, u64, Option<clock::ClockPulseMonitor>), AppError> {
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
        clock.runner.as_ref().map(|runner| runner.pulse_monitor()),
    ))
}

/// Stop any running clock thread and return the `MidiOutputConnection`
/// it was using back into the session. After this returns the session's
/// `out_conn` is populated (assuming the session still exists) and
/// SysEx paths are unblocked again.
pub(crate) async fn stop_clock(state: &Arc<AppState>) {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    stop_clock_inner(state).await;
}

async fn stop_clock_inner(state: &Arc<AppState>) {
    // Take the ClockState out of the Mutex so the thread join below
    // happens *without* holding `state.playback.clock`. Holding it across the
    // blocking join would serialize every other endpoint that checks
    // clock state (e.g. `/api/status`).
    let clock_state = {
        let mut clock_guard = state.playback.clock.lock().await;
        clock_guard.take()
    };

    let Some(clock) = clock_state else { return };
    stop_taken_clock(state, clock).await;
}

/// Stop a failed or canceled start only if it still owns the current transport
/// generation. A delayed acknowledgement must never stop a newer runner.
async fn stop_clock_transport(state: &Arc<AppState>, transport_id: u64) {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    let clock_state = {
        let mut clock_guard = state.playback.clock.lock().await;
        if clock_guard
            .as_ref()
            .is_some_and(|clock| clock.transport_id == transport_id)
        {
            clock_guard.take()
        } else {
            None
        }
    };

    if let Some(clock) = clock_state {
        stop_taken_clock(state, clock).await;
    }
}

/// Remove a timed-out transport immediately, signal its runner, and finish
/// the potentially blocking MIDI-driver join away from the request path. The
/// connection is discarded when the join eventually returns because it is no
/// longer safe to restore it into a session that may have reconnected.
async fn stop_clock_transport_detached(state: &Arc<AppState>, transport_id: u64) {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    let (clock_state, invalidated_session) = {
        let mut session_guard = state.midi.session.lock().await;
        let mut clock_guard = state.playback.clock.lock().await;
        if clock_guard
            .as_ref()
            .is_some_and(|clock| clock.transport_id == transport_id)
        {
            let clock = clock_guard.take();
            let invalidated_session =
                if let (Some(session), Some(clock)) = (session_guard.as_ref(), clock.as_ref()) {
                    if session.generation == clock.session_generation {
                        session_guard.take()
                    } else {
                        None
                    }
                } else {
                    None
                };
            (clock, invalidated_session)
        } else {
            (None, None)
        }
    };

    let Some(mut clock) = clock_state else {
        return;
    };
    clock.playing = false;
    let Some(runner) = clock.runner.take() else {
        return;
    };

    runner.stop_detached(Arc::clone(&state.playback.midi_cleanup_pending));
    if let Some(session) = invalidated_session {
        drop_midi_session_detached(session, Arc::clone(&state.playback.midi_cleanup_pending));
    }
}

fn drop_midi_session_detached(
    session: MidiSession,
    cleanup_pending: Arc<std::sync::atomic::AtomicUsize>,
) {
    cleanup_pending.fetch_add(1, Ordering::AcqRel);
    let holder = Arc::new(std::sync::Mutex::new(Some(session)));
    let worker_holder = Arc::clone(&holder);
    let cleanup_flag = Arc::clone(&cleanup_pending);
    let spawned = std::thread::Builder::new()
        .name("td3-midi-session-close".to_string())
        .spawn(move || {
            if let Ok(mut session) = worker_holder.lock() {
                drop(session.take());
                cleanup_flag.fetch_sub(1, Ordering::AcqRel);
            }
        });
    if spawned.is_err() {
        // A thread-creation failure must not move a potentially blocking
        // driver close back onto the timed-out request path.
        std::mem::forget(holder);
    }
}

pub(crate) async fn invalidate_midi_session_generation(
    state: &Arc<AppState>,
    session_generation: u64,
) {
    let invalidated_session = {
        let mut session_guard = state.midi.session.lock().await;
        if session_guard.as_ref().is_some_and(|session| {
            session.generation == session_generation && session.out_conn.is_none()
        }) {
            session_guard.take()
        } else {
            None
        }
    };
    if let Some(session) = invalidated_session {
        drop_midi_session_detached(session, Arc::clone(&state.playback.midi_cleanup_pending));
    }
}

async fn restore_clock_connection(
    state: &Arc<AppState>,
    session_generation: u64,
    out_conn: midir::MidiOutputConnection,
) {
    let mut guard = state.midi.session.lock().await;
    if let Some(session) = guard.as_mut() {
        if session.generation == session_generation && session.out_conn.is_none() {
            session.out_conn = Some(out_conn);
        }
    }
}

async fn stop_taken_clock(state: &Arc<AppState>, mut clock: ClockState) {
    clock.playing = false;
    let session_generation = clock.session_generation;
    let Some(runner) = clock.runner.take() else {
        return;
    };

    // `stop()` signals the thread, waits for an already-started transport to
    // emit MIDI Stop (0xFC), and joins. Use `spawn_blocking` so the tokio
    // worker isn't parked on the OS-thread join.
    let out_conn = tokio::task::spawn_blocking(move || runner.stop())
        .await
        .ok()
        .flatten();

    // Put the connection back into the session so the next SysEx
    // operation can use it. If the session is gone (parallel
    // disconnect) the connection drops here and the port closes -
    // that matches what disconnect wanted anyway.
    if let Some(out_conn) = out_conn {
        restore_clock_connection(state, session_generation, out_conn).await;
    }
}

/// Stop any running host-audition thread and return the
/// `MidiOutputConnection` it was using back into the session. The
/// audition thread silences sounding notes on its way out. After this
/// returns the session's `out_conn` is populated again (assuming the
/// session still exists). Safe to call when no audition is running.
pub(crate) async fn stop_audition(state: &Arc<AppState>) {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    stop_audition_inner(state).await;
}

async fn stop_audition_inner(state: &Arc<AppState>) {
    // Take the runner out of the Mutex so the blocking join below does
    // not hold `state.playback.audition` across the wait.
    let audition = {
        let mut guard = state.playback.audition.lock().await;
        guard.take()
    };

    let Some(audition) = audition else {
        return;
    };
    finish_audition(state, audition).await;
}

pub(crate) async fn stop_audition_if_id(state: &Arc<AppState>, audition_id: u64) {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    let audition = {
        let mut guard = state.playback.audition.lock().await;
        if guard
            .as_ref()
            .is_some_and(|audition| audition.audition_id == audition_id)
        {
            guard.take()
        } else {
            None
        }
    };

    if let Some(audition) = audition {
        finish_audition(state, audition).await;
    }
}

pub(crate) async fn stop_audition_detached_if_id(state: &Arc<AppState>, audition_id: u64) {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    let (audition, invalidated_session) = {
        let mut session_guard = state.midi.session.lock().await;
        let mut audition_guard = state.playback.audition.lock().await;
        if audition_guard
            .as_ref()
            .is_some_and(|audition| audition.audition_id == audition_id)
        {
            let audition = audition_guard.take();
            let invalidated_session = if let (Some(session), Some(audition)) =
                (session_guard.as_ref(), audition.as_ref())
            {
                if session.generation == audition.session_generation {
                    session_guard.take()
                } else {
                    None
                }
            } else {
                None
            };
            (audition, invalidated_session)
        } else {
            (None, None)
        }
    };

    if let Some(audition) = audition {
        audition
            .runner
            .stop_detached(Arc::clone(&state.playback.midi_cleanup_pending));
    }
    if let Some(session) = invalidated_session {
        drop_midi_session_detached(session, Arc::clone(&state.playback.midi_cleanup_pending));
    }
}

async fn finish_audition(state: &Arc<AppState>, audition: AuditionState) {
    let session_generation = audition.session_generation;
    let runner = audition.runner;

    // `stop()` signals the thread, waits for it to silence sounding
    // notes, and joins. Use `spawn_blocking` so the tokio worker isn't
    // parked on the OS-thread join.
    let out_conn = tokio::task::spawn_blocking(move || runner.stop())
        .await
        .ok()
        .flatten();

    if let Some(out_conn) = out_conn {
        restore_clock_connection(state, session_generation, out_conn).await;
    }
}

/// Stop every output owner and clear the MIDI session as one lifecycle
/// operation so a concurrent start cannot install an orphan runner.
pub(crate) async fn disconnect_midi_session(state: &Arc<AppState>) -> bool {
    let _lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    stop_clock_inner(state).await;
    stop_audition_inner(state).await;
    let mut session_guard = state.midi.session.lock().await;
    session_guard.take().is_some()
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
async fn spawn_clock_runner(
    state: &Arc<AppState>,
    centibpm: u32,
    start_delay: Duration,
) -> Result<(clock::ClockRunner, u64), AppError> {
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

    let lane_inbox = Arc::clone(&state.playback.cutoff_lane);
    let runner =
        match clock::ClockRunner::spawn_scheduled(out_conn, centibpm, start_delay, lane_inbox) {
            Ok(runner) => runner,
            Err(err) => {
                invalidate_midi_session_generation(state, session_generation).await;
                return Err(AppError::Midi(Td3Error::Midi(format!(
                    "{}; MIDI connection closed, reconnect before retrying",
                    err
                ))));
            }
        };
    Ok((runner, session_generation))
}

/// Install a clock runner and return only after MIDI Start and the first MIDI
/// Clock pulse were accepted by the output connection. Failure cleanup is
/// scoped to this transport ID so a delayed acknowledgement cannot stop a
/// newer transport.
pub(crate) async fn start_clock_transport(
    state: &Arc<AppState>,
    centibpm: u32,
    started_at_epoch_ms: u64,
    transport_id: u64,
    start_delay: Duration,
) -> Result<clock::ClockPulseSnapshot, AppError> {
    let lifecycle = state.playback.midi_owner_lifecycle.lock().await;
    let (runner, session_generation) = spawn_clock_runner(state, centibpm, start_delay).await?;
    let start_monitor = runner.start_monitor();
    let pulse_monitor = runner.pulse_monitor();

    *state.playback.clock.lock().await = Some(ClockState {
        session_generation,
        centibpm,
        started_at_epoch_ms,
        transport_id,
        playing: true,
        runner: Some(runner),
    });
    drop(lifecycle);

    let start_result = match tokio::task::spawn_blocking(move || start_monitor.wait_for_start())
        .await
    {
        Ok(result) => result,
        Err(err) => {
            stop_clock_transport_detached(state, transport_id).await;
            return Err(AppError::Internal(format!(
                    "MIDI Start acknowledgement task failed: {}; MIDI connection closed, reconnect before retrying",
                    err
                )));
        }
    };
    if let Err(err) = start_result {
        if let Td3Error::Timeout { operation } = err {
            stop_clock_transport_detached(state, transport_id).await;
            return Err(AppError::Midi(Td3Error::Timeout {
                operation: format!(
                    "{}; MIDI connection closed, reconnect before retrying",
                    operation
                ),
            }));
        }
        stop_clock_transport(state, transport_id).await;
        return Err(AppError::Midi(err));
    }

    let first_pulse_task = tokio::task::spawn_blocking(move || pulse_monitor.wait_for_pulse(0));
    let first_pulse = match tokio::time::timeout(FIRST_CLOCK_PULSE_TIMEOUT, first_pulse_task).await
    {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(err)) => {
            stop_clock_transport_detached(state, transport_id).await;
            return Err(AppError::Internal(format!(
                "first clock pulse wait failed: {}; MIDI connection closed, reconnect before retrying",
                err
            )));
        }
        Err(_) => {
            stop_clock_transport_detached(state, transport_id).await;
            return Err(AppError::Midi(Td3Error::Timeout {
                operation:
                    "first MIDI Clock pulse; MIDI connection closed, reconnect before retrying"
                        .to_string(),
            }));
        }
    };
    let Some(snapshot) = first_pulse else {
        stop_clock_transport_detached(state, transport_id).await;
        return Err(AppError::Conflict(
            "transport stopped before the first MIDI Clock pulse; MIDI connection closed, reconnect before retrying"
                .to_string(),
        ));
    };

    let mut clock_guard = state.playback.clock.lock().await;
    let Some(clock) = clock_guard
        .as_mut()
        .filter(|clock| clock.transport_id == transport_id)
    else {
        return Err(AppError::Conflict(
            "transport was superseded before startup completed".to_string(),
        ));
    };
    clock.started_at_epoch_ms = snapshot.epoch_micros / 1_000;
    clock.centibpm = snapshot.centibpm;
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// GET /api/scratch-pattern
// ---------------------------------------------------------------------------
