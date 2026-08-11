use super::*;

// ---------------------------------------------------------------------------
// POST /api/note/preview
// ---------------------------------------------------------------------------

pub async fn note_preview(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NotePreviewRequest>,
) -> Result<Json<NotePreviewResponse>, AppError> {
    let midi_note = req.midi_note().map_err(AppError::BadRequest)?;
    let velocity: u8 = if req.accent { 110 } else { 78 };
    // A device set to channel N discards channel-voice messages
    // addressed elsewhere, so the preview is inaudible unless this
    // matches the device. The transport bar's CH selector supplies it
    // when present; otherwise the configured default stands.
    let channel = match req.midi_channel {
        None => state.midi.runtime.device_channel,
        Some(value) if (1..=16).contains(&value) => value as u8,
        Some(value) => {
            return Err(AppError::BadRequest(format!(
                "midi channel must be 1-16, got {}",
                value
            )))
        }
    };

    let mut guard = state.midi.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or(AppError::BadRequest("not connected".into()))?;

    // Fails gracefully if the clock thread is currently holding the
    // output - the UI gates preview to idle anyway, so this is
    // belt-and-suspenders.
    let out_conn = session.out_conn.as_mut().ok_or(AppError::BadRequest(
        "transport is running - stop it first".into(),
    ))?;
    out_conn
        .send(&[channel_status(0x90, channel), midi_note, velocity])
        .map_err(|e| Td3Error::Midi(format!("note on: {}", e)))?;

    // Schedule Note Off after 150ms in background. If the transport
    // is started between now and then the output will be checked out
    // by the clock thread - the hanging note will be cleared by the
    // MIDI Start/Stop bytes the clock emits, so silently skipping is
    // safe.
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let mut guard = state_clone.midi.session.lock().await;
        if let Some(session) = guard.as_mut() {
            if let Some(out) = session.out_conn.as_mut() {
                let _ = out.send(&[channel_status(0x80, channel), midi_note, 64]);
            }
        }
    });

    Ok(Json(NotePreviewResponse { ok: true }))
}
