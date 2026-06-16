use super::*;

// ---------------------------------------------------------------------------
// GET /api/status
// ---------------------------------------------------------------------------

pub async fn status(
    State(midi): State<MidiState>,
    State(playback): State<PlaybackState>,
    State(config): State<ConfigState>,
) -> Json<StatusResponse> {
    let session_guard = midi.session.lock().await;
    let clock_guard = playback.clock.lock().await;

    let (connected, product_name, firmware, sync_source) = match session_guard.as_ref() {
        Some(s) => (
            true,
            Some(s.product_name.clone()),
            Some(s.firmware_version.clone()),
            Some(s.sync_source.as_str().to_string()),
        ),
        None => (false, None, None, None),
    };
    let (playing, centibpm) = match clock_guard.as_ref() {
        Some(c) => (c.playing, c.centibpm),
        None => (false, config.ui_config.ui_default_bpm.saturating_mul(100)),
    };
    let bpm = centibpm / 100;

    Json(StatusResponse {
        connected,
        product_name,
        firmware,
        playing,
        bpm,
        centibpm,
        sync_source,
    })
}

// ---------------------------------------------------------------------------
// GET /api/ports
// ---------------------------------------------------------------------------

pub async fn ports() -> Result<Json<PortsResponse>, AppError> {
    let (inputs, outputs) = tokio::task::block_in_place(|| -> Result<_, Td3Error> {
        let ports = crate::midi_ports::list_port_names()?;
        Ok((ports.inputs, ports.outputs))
    })?;

    Ok(Json(PortsResponse { inputs, outputs }))
}

// ---------------------------------------------------------------------------
// POST /api/midi/connect
// ---------------------------------------------------------------------------

pub async fn connect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, AppError> {
    let mut guard = state.midi.session.lock().await;

    // If already connected, return existing session info instead of an error
    if let Some(session) = guard.as_ref() {
        return Ok(Json(ConnectResponse {
            product_name: session.product_name.clone(),
            firmware: session.firmware_version.clone(),
        }));
    }

    // Manual connect now follows TD3_CONFIG.env: the env-derived port
    // substring is the fallback when the request omits one, and the
    // strict-name flag is also env-driven so a single source of truth
    // governs auto-connect AND manual connect.
    let in_port_name = req
        .in_port
        .clone()
        .unwrap_or_else(|| state.midi.runtime.input_port_name.clone());
    let out_port_name = req
        .out_port
        .clone()
        .unwrap_or_else(|| state.midi.runtime.output_port_name.clone());
    let strict = state.midi.runtime.strict_name_match;
    let probe_timeout = state.midi.runtime.timeout;

    let established = tokio::task::block_in_place(|| {
        establish_td3_midi_session(Td3MidiSessionConfig {
            input_port_name: &in_port_name,
            output_port_name: &out_port_name,
            strict_name_match: strict,
            timeout: probe_timeout,
            sync_source_policy: td3_protocol::SyncSourceFailurePolicy::DefaultToUsb,
        })
    })?;

    let response = ConnectResponse {
        product_name: established.info.product_name.clone(),
        firmware: established.info.firmware_version.clone(),
    };
    if let Some(err) = &established.info.sync_source_error {
        log::warn!("read sync source failed, defaulting to USB: {}", err);
    }

    *guard = Some(MidiSession {
        out_conn: Some(established.out_conn),
        rx: established.rx,
        _in_conn: established.in_conn,
        product_name: established.info.product_name,
        firmware_version: established.info.firmware_version,
        sync_source: established.info.sync_source,
    });

    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// POST /api/midi/disconnect
// ---------------------------------------------------------------------------

pub async fn disconnect(State(state): State<Arc<AppState>>) -> Json<DisconnectResponse> {
    // Stop the clock first so its dedicated thread releases its own
    // MIDI output handle before we drop the main session. Otherwise
    // disconnect leaves the port half-owned and a subsequent reconnect
    // can fail with "port already in use" on some drivers.
    stop_clock(&state).await;

    let mut guard = state.midi.session.lock().await;
    let was_connected = guard.is_some();
    *guard = None;
    Json(DisconnectResponse {
        disconnected: was_connected,
    })
}

// ---------------------------------------------------------------------------
// POST /api/midi/sync-source
// ---------------------------------------------------------------------------

pub async fn set_sync_source(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetSyncSourceRequest>,
) -> Result<Json<SetSyncSourceResponse>, AppError> {
    let target = td3_protocol::SyncSource::from_str(&req.source)
        .map_err(|_| AppError::BadRequest(format!("invalid sync source '{}'", req.source)))?;

    let mut session_guard = state.midi.session.lock().await;
    let mut clock_guard = state.playback.clock.lock().await;
    let session = session_guard
        .as_mut()
        .ok_or(AppError::BadRequest("not connected".into()))?;

    let timeout = state.midi.runtime.timeout;
    tokio::task::block_in_place(|| {
        with_sender(session, clock_guard.as_mut(), |sender, rx| {
            td3_protocol::set_sync_source(sender, rx, target, timeout)
        })
    })?;

    session.sync_source = target;
    Ok(Json(SetSyncSourceResponse {
        source: target.as_str().to_string(),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/pattern/load
