use super::*;

/// Scan the configured backup directory and import any new backup zips as
/// `SnapshotOrigin::Backup` snapshots. Idempotent by `backup_path`.
pub(super) async fn sync_backups(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SyncBackupsRequest>,
) -> Result<Json<SyncBackupsResponse>, AppError> {
    let dir_str = req
        .backup_dir
        .as_deref()
        .unwrap_or(state.library.backup_dir_path.as_str());
    if dir_str.trim().is_empty() {
        return Err(AppError::BadRequest(
            "no backup directory configured (set BACKUP_DIR_PATH in TD3_CONFIG.env or pass backup_dir in the request)"
                .into(),
        ));
    }
    let dir = std::path::PathBuf::from(dir_str);

    if !dir.exists() {
        return Err(AppError::BadRequest(format!(
            "backup directory does not exist: {}",
            dir.display()
        )));
    }

    let entries = bank::scan_backup_dir(&dir).map_err(AppError::Midi)?;
    let added = state
        .library
        .store
        .sync_backup_inventory(&entries)
        .map_err(AppError::Midi)?;
    let total = state
        .library
        .store
        .list_snapshots()
        .map_err(AppError::Midi)?
        .len();

    Ok(Json(SyncBackupsResponse {
        added: added as u32,
        total: total as u32,
    }))
}

/// Export selected slots from a snapshot as individual pattern files into a
/// backend-created sub-folder of the user's chosen target directory.
///
/// The sub-folder is named `{source}_export`, where `source` is the filename
/// stem of `snapshot.backup_path` when set (e.g. `idea.rbs` -> `idea.rbs_export`
/// after sanitization), or the sanitized `snapshot.name` otherwise. This
/// matches the user's contract: importing `idea.rbs` then exporting drops
/// files into `{target}/idea.rbs_export/G1P1A.steps.txt` etc.
///
/// Empty slots in the requested set are skipped and returned as `skipped` -
/// they are not an error because the UI sends the user's selection as-is.
pub(super) async fn export_snapshot_patterns(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExportSnapshotPatternsRequest>,
) -> Result<Json<ExportSnapshotPatternsResponse>, AppError> {
    let snapshot = state
        .library
        .store
        .get_snapshot(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("snapshot '{}' not found", id)))?;

    if req.target_dir.trim().is_empty() {
        return Err(AppError::BadRequest("target_dir must not be empty".into()));
    }
    if req.slot_keys.is_empty() {
        return Err(AppError::BadRequest("slot_keys must not be empty".into()));
    }
    // TD-3 hardware has 64 pattern slots; cap to bound allocation.
    const MAX_SLOTS: usize = 64;
    if req.slot_keys.len() > MAX_SLOTS {
        return Err(AppError::BadRequest(format!(
            "slot_keys length {} exceeds maximum {}",
            req.slot_keys.len(),
            MAX_SLOTS
        )));
    }

    // Index snapshot slots by key for O(1) item_id lookup.
    let raw_slots = state
        .library
        .store
        .list_snapshot_slots(&id)
        .map_err(AppError::Midi)?;
    let by_key: std::collections::HashMap<String, crate::library::model::SnapshotSlot> = raw_slots
        .into_iter()
        .map(|s| (s.slot_key.clone(), s))
        .collect();

    let mut slots: Vec<crate::web::snapshot_export::ExportSlot> = Vec::with_capacity(MAX_SLOTS);
    for key in &req.slot_keys {
        let payload = by_key
            .get(key)
            .and_then(|s| s.item_id.as_ref())
            .and_then(|item_id| state.library.store.pattern_bytes_for(item_id));
        slots.push(crate::web::snapshot_export::ExportSlot {
            slot_key: key.clone(),
            payload,
        });
    }

    // Prefer the backup_path filename (e.g. "idea.rbs") over the snapshot
    // name - users explicitly asked for the source-file-derived folder name.
    let folder_stem = match snapshot.backup_path.as_deref() {
        Some(bp) => PathBuf::from(bp)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| snapshot.name.clone()),
        None => snapshot.name.clone(),
    };

    let target_dir = PathBuf::from(&req.target_dir);
    let midi_opts = state.midi.export_options.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::web::snapshot_export::run(&crate::web::snapshot_export::ExportRequest {
            target_dir: &target_dir,
            folder_stem: &folder_stem,
            slots: &slots,
            formats: &req.formats,
            midi_opts: &midi_opts,
        })
    })
    .await
    .map_err(|e| AppError::BadRequest(format!("export task failed: {e}")))?
    // All failures from the export helper are user-input-driven (bad
    // format id, missing target dir, decode error on a stored payload):
    // surface them as 400s so the UI can render the message inline.
    .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(Json(ExportSnapshotPatternsResponse {
        folder_path: result.folder_path.display().to_string(),
        file_count: result.file_count,
        skipped: result.skipped,
    }))
}
