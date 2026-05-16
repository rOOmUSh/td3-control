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

    let mut guard = state.midi.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or(AppError::BadRequest("not connected".into()))?;

    // Note On (channel 0). Fails gracefully if the clock thread is
    // currently holding the output - the UI gates preview to idle
    // anyway, so this is belt-and-suspenders.
    let out_conn = session.out_conn.as_mut().ok_or(AppError::BadRequest(
        "transport is running - stop it first".into(),
    ))?;
    out_conn
        .send(&[0x90, midi_note, velocity])
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
                let _ = out.send(&[0x80, midi_note, 64]);
            }
        }
    });

    Ok(Json(NotePreviewResponse { ok: true }))
}
