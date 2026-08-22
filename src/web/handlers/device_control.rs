use super::*;
use crate::web::device_capabilities::{
    filter_cutoff_bytes, pitch_bend_bytes, MAX_FILTER_CUTOFF, MAX_PITCH_BEND,
};

// ---------------------------------------------------------------------------
// POST /api/device/filter-cutoff
// ---------------------------------------------------------------------------

/// Send Filter Cutoff (CC 74) to a device that supports it. Rejects values
/// above 127, a missing session, and a session without device controls.
pub async fn device_filter_cutoff(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<DeviceControlRequest>, JsonRejection>,
) -> Result<Json<DeviceControlResponse>, AppError> {
    let req = json_payload(payload, "filter cutoff")?;
    let value = bounded_value(req.value, MAX_FILTER_CUTOFF, "filter cutoff")?;
    let channel = resolve_channel(&state, req.midi_channel)?;
    let bytes = filter_cutoff_bytes(channel_status(0xB0, channel), value)
        .ok_or_else(|| AppError::BadRequest("filter cutoff out of range".into()))?;
    send_device_control(&state, &bytes).await?;
    Ok(Json(DeviceControlResponse {
        ok: true,
        value: u32::from(value),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/device/pitch-bend
// ---------------------------------------------------------------------------

/// Send 14-bit Pitch Bend to a device that supports it. Rejects values
/// above 16383, a missing session, and a session without device controls.
pub async fn device_pitch_bend(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<DeviceControlRequest>, JsonRejection>,
) -> Result<Json<DeviceControlResponse>, AppError> {
    let req = json_payload(payload, "pitch bend")?;
    let value = bounded_value(req.value, MAX_PITCH_BEND, "pitch bend")?;
    let channel = resolve_channel(&state, req.midi_channel)?;
    let bytes = pitch_bend_bytes(channel_status(0xE0, channel), value)
        .ok_or_else(|| AppError::BadRequest("pitch bend out of range".into()))?;
    send_device_control(&state, &bytes).await?;
    Ok(Json(DeviceControlResponse {
        ok: true,
        value: u32::from(value),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/transport/step-lane
// ---------------------------------------------------------------------------

/// Hand a per-step cutoff lane to the clock thread. The lane is stored in
/// the playback inbox, so it needs no session and no running transport:
/// a running clock picks it up on its next pulse and a later start reads
/// it from the first pulse.
pub async fn transport_step_lane(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<StepLaneRequest>, JsonRejection>,
) -> Result<Json<StepLaneResponse>, AppError> {
    use crate::web::clock::cutoff_lane::{CutoffLane, LaneRequest};

    let req = json_payload(payload, "step lane")?;
    if !(1..=16).contains(&req.active_steps) {
        return Err(AppError::BadRequest(format!(
            "activeSteps must be 1-16, got {}",
            req.active_steps
        )));
    }
    let channel = resolve_channel(&state, req.midi_channel)?;
    let values = match &req.cutoffs {
        None => None,
        Some(list) => {
            if list.len() != 16 {
                return Err(AppError::BadRequest(format!(
                    "cutoffs must have exactly 16 values, got {}",
                    list.len()
                )));
            }
            let mut out = [0u8; 16];
            for (i, value) in list.iter().enumerate() {
                if *value > u32::from(MAX_FILTER_CUTOFF) {
                    return Err(AppError::BadRequest(format!(
                        "cutoffs[{}] must be 0-127, got {}",
                        i, value
                    )));
                }
                out[i] = *value as u8;
            }
            Some(out)
        }
    };
    let lane = CutoffLane {
        values,
        active_steps: req.active_steps as u8,
        triplet: req.triplet,
        channel,
    };
    let request = LaneRequest {
        lane: Some(lane),
        at_cycle_boundary: req.at_cycle_boundary,
    };
    match state.playback.cutoff_lane.lock() {
        Ok(mut slot) => *slot = Some(request),
        Err(poisoned) => *poisoned.into_inner() = Some(request),
    }
    Ok(Json(StepLaneResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// Shared validation and transport
// ---------------------------------------------------------------------------

fn bounded_value(value: u32, max: u16, name: &str) -> Result<u16, AppError> {
    if value > u32::from(max) {
        return Err(AppError::BadRequest(format!(
            "{} must be 0-{}, got {}",
            name, max, value
        )));
    }
    Ok(value as u16)
}

/// Same channel rule as `/api/note/preview`: an omitted channel means the
/// configured device channel; anything outside 1 through 16 is rejected.
fn resolve_channel(state: &AppState, requested: Option<u32>) -> Result<u8, AppError> {
    match requested {
        None => Ok(state.midi.runtime.device_channel),
        Some(value) if (1..=16).contains(&value) => Ok(value as u8),
        Some(value) => Err(AppError::BadRequest(format!(
            "midi channel must be 1-16, got {}",
            value
        ))),
    }
}

/// Write one channel-voice message to the device and return only after
/// the write result is known.
///
/// Locks session, then clock, then audition, the order the other
/// handlers use. An idle session writes directly; during transport
/// playback the bytes go through the clock thread; during host audition
/// they go through the audition thread, which writes them between its
/// scheduled events. Each thread reports the driver result back, so `Ok`
/// means the port accepted the bytes.
async fn send_device_control(state: &Arc<AppState>, bytes: &[u8]) -> Result<(), AppError> {
    let mut session_guard = state.midi.session.lock().await;
    let session = session_guard
        .as_mut()
        .ok_or(AppError::BadRequest("not connected".into()))?;

    if !supports_device_controls(&session.firmware_version) {
        return Err(AppError::BadRequest(
            "device does not support filter cutoff or pitch bend".into(),
        ));
    }

    if let Some(out_conn) = session.out_conn.as_mut() {
        tokio::task::block_in_place(|| out_conn.send_bytes(bytes))?;
        return Ok(());
    }

    let mut clock_guard = state.playback.clock.lock().await;
    if let Some(runner) = clock_guard.as_mut().and_then(|c| c.runner.as_mut()) {
        tokio::task::block_in_place(|| runner.send_bytes(bytes))?;
        return Ok(());
    }
    drop(clock_guard);

    let mut audition_guard = state.playback.audition.lock().await;
    let session_generation = session.generation;
    match audition_guard.as_mut() {
        Some(audition) if audition.session_generation == session_generation => {
            tokio::task::block_in_place(|| audition.runner.send_bytes(bytes))?;
            Ok(())
        }
        _ => Err(AppError::Conflict(
            "MIDI output is not available right now - try again".into(),
        )),
    }
}
