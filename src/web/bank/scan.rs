use super::*;

// ---------------------------------------------------------------------------
// Scan / Import
// ---------------------------------------------------------------------------
//
// Both `scan` and `import` drive `library::ingest::ingest_path`. They differ
// only in where the candidate paths come from:
//   - `scan` walks a directory (recursive by default) and ingests everything
//     whose extension is in the supported set;
//   - `import` takes an explicit list of paths from the request body.
//
// Every run creates exactly one `ImportBatch` whose tally is finalised after
// the loop. The per-path `FileIndexEntry` rows are written inside
// `ingest_path` so the batch drill-down view can show them immediately.

pub(super) async fn scan(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScanRequest>,
) -> Result<(StatusCode, Json<ScanStartResponse>), AppError> {
    if req.path.trim().is_empty() {
        return Err(AppError::BadRequest("scan path must not be empty".into()));
    }

    let start =
        state.scan.jobs.start(req.path.clone()).map_err(|active| {
            AppError::Conflict(format!("scan already running: {}", active.job_id))
        })?;
    let root = PathBuf::from(&req.path);
    let recursive = req.recursive.unwrap_or(true);

    reset_scan_progress(&state, &req.path);

    let state_for_task = state.clone();
    let job_id = start.job_id.clone();
    tokio::spawn(async move {
        let state_for_block = state_for_task.clone();
        let job_id_for_block = job_id.clone();
        let handle = tokio::task::spawn_blocking(move || {
            run_scan_job(&state_for_block, &job_id_for_block, root, recursive)
        });

        match handle.await {
            Ok(Ok(resp)) => {
                state_for_task.scan.jobs.complete(&job_id, resp);
                finish_scan_progress(&state_for_task, None);
            }
            Ok(Err(err)) => {
                let message = app_error_message(&err);
                state_for_task.scan.jobs.fail(&job_id, message.clone());
                finish_scan_progress(&state_for_task, Some(message));
            }
            Err(join_err) => {
                let mut message = String::from("scan task panicked: ");
                message.push_str(&join_err.to_string());
                state_for_task.scan.jobs.fail(&job_id, message.clone());
                finish_scan_progress(&state_for_task, Some(message));
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(start)))
}

fn run_scan_job(
    state: &Arc<AppState>,
    job_id: &str,
    root: PathBuf,
    recursive: bool,
) -> Result<ScanResponse, AppError> {
    state.scan.jobs.mark_running(job_id);

    if !root.exists() {
        return Err(AppError::BadRequest(format!(
            "scan path does not exist: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(AppError::BadRequest(format!(
            "scan path is not a directory: {}",
            root.display()
        )));
    }

    let batch = state
        .library
        .store
        .create_import_batch(Some(root.display().to_string()))
        .map_err(AppError::Midi)?;

    let paths = ingest::list_candidate_files(&root, recursive).map_err(AppError::Midi)?;
    state.scan.jobs.set_found(job_id, paths.len());
    set_scan_found(state, paths.len());

    let entries = run_ingest_batch(state, &batch.batch_id, &paths, Some(job_id))?;

    let tally = tally_entries(&entries);
    state
        .library
        .store
        .finish_import_batch(
            &batch.batch_id,
            tally.found,
            tally.imported,
            tally.duplicates,
            tally.unsupported,
            tally.failed,
        )
        .map_err(AppError::Midi)?;

    Ok(ScanResponse {
        batch_id: batch.batch_id,
        entries,
    })
}

pub(super) async fn scan_job_status(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<ScanJobResponse>, AppError> {
    state
        .scan
        .jobs
        .get(&job_id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("scan job '{}' not found", job_id)))
}

fn reset_scan_progress(state: &Arc<AppState>, path_value: &str) {
    use std::sync::atomic::Ordering;
    let p = &state.scan.progress;
    p.running.store(true, Ordering::SeqCst);
    p.found.store(0, Ordering::SeqCst);
    p.parsed.store(0, Ordering::SeqCst);
    p.generation.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut path) = p.path.lock() {
        *path = path_value.to_string();
    }
    if let Ok(mut err) = p.last_error.lock() {
        *err = None;
    }
}

fn set_scan_found(state: &Arc<AppState>, found: usize) {
    use std::sync::atomic::Ordering;
    state.scan.progress.found.store(found, Ordering::SeqCst);
}

fn finish_scan_progress(state: &Arc<AppState>, error: Option<String>) {
    use std::sync::atomic::Ordering;
    let p = &state.scan.progress;
    p.running.store(false, Ordering::SeqCst);
    if let Some(message) = error {
        if let Ok(mut err) = p.last_error.lock() {
            *err = Some(message);
        }
    }
}

fn app_error_message(err: &AppError) -> String {
    match err {
        AppError::BadRequest(msg)
        | AppError::Conflict(msg)
        | AppError::Internal(msg)
        | AppError::NotFound(msg) => msg.clone(),
        AppError::Midi(e) => e.to_string(),
    }
}

pub(super) async fn scan_progress(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ScanProgressResponse>, AppError> {
    use std::sync::atomic::Ordering;
    let p = &state.scan.progress;
    let path = p.path.lock().map(|g| g.clone()).unwrap_or_default();
    let error = p.last_error.lock().ok().and_then(|g| g.clone());
    Ok(Json(ScanProgressResponse {
        running: p.running.load(Ordering::SeqCst),
        found: p.found.load(Ordering::SeqCst),
        parsed: p.parsed.load(Ordering::SeqCst),
        path,
        error,
        generation: p.generation.load(Ordering::SeqCst),
    }))
}
