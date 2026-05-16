use super::*;

pub(super) async fn import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportResponse>, AppError> {
    if req.paths.is_empty() {
        return Err(AppError::BadRequest(
            "import: paths must not be empty".into(),
        ));
    }

    let batch = state
        .library
        .store
        .create_import_batch(None)
        .map_err(AppError::Midi)?;

    let paths: Vec<PathBuf> = req.paths.iter().map(PathBuf::from).collect();
    let entries = run_ingest_batch(&state, &batch.batch_id, &paths, None)?;

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

    Ok(Json(ImportResponse {
        batch_id: batch.batch_id,
        entries,
    }))
}

/// Drive `ingest::ingest_path` over each path and collect the resulting entry
/// rows. The store appends each entry internally so the batch view is always
/// consistent with what was attempted. One progress line is printed per file
/// (`[scan] N/M <status>: path`) so the operator sees activity during long
/// runs; the count is 1-based.
pub(super) fn run_ingest_batch(
    state: &Arc<AppState>,
    batch_id: &str,
    paths: &[PathBuf],
    scan_job_id: Option<&str>,
) -> Result<Vec<FileIndexEntry>, AppError> {
    use std::sync::atomic::Ordering;
    let total = paths.len();
    let mut entries: Vec<FileIndexEntry> = Vec::with_capacity(total);
    let import_opts = state.midi.import_options.clone();
    for (idx, p) in paths.iter().enumerate() {
        let outcome = ingest::ingest_path(&state.library.store, p, batch_id, &import_opts)
            .map_err(AppError::Midi)?;
        eprintln!(
            "[scan] {}/{} {:?}: {}",
            idx + 1,
            total,
            outcome.entry.status,
            p.display()
        );
        entries.push(outcome.entry);
        // Publish the running total so the UI's progress poll can draw a
        // live status bar ("Parsing N/M"). Ordering::SeqCst - correctness
        // beats micro-optimisation on a per-file atomic bump.
        state.scan.progress.parsed.store(idx + 1, Ordering::SeqCst);
        if let Some(job_id) = scan_job_id {
            state.scan.jobs.set_parsed(job_id, idx + 1);
        }
    }
    Ok(entries)
}

pub(super) async fn list_import_batches(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ImportBatchesResponse>, AppError> {
    let batches = state
        .library
        .store
        .list_import_batches()
        .map_err(AppError::Midi)?;
    Ok(Json(ImportBatchesResponse { batches }))
}

pub(super) async fn get_import_batch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ImportBatchResponse>, AppError> {
    // Sync SQLite on a blocking thread: scan/retry can hold writers long
    // enough to otherwise starve the tokio worker pool.
    tokio::task::spawn_blocking(move || -> Result<Json<ImportBatchResponse>, AppError> {
        let batch = state
            .library
            .store
            .get_import_batch(&id)
            .map_err(AppError::Midi)?
            .ok_or_else(|| AppError::BadRequest(format!("import batch '{}' not found", id)))?;
        let entries = state
            .library
            .store
            .list_batch_entries(&id)
            .map_err(AppError::Midi)?;
        Ok(Json(ImportBatchResponse { batch, entries }))
    })
    .await
    .map_err(|e| {
        AppError::Midi(crate::error::Td3Error::Other(format!(
            "join get_import_batch: {}",
            e
        )))
    })?
}

/// Re-run parsing for every `Failed` row in a batch. Non-failed rows are left
/// alone; each retried row is replaced in place (not appended) so the batch
/// view does not accumulate ghost history.
pub(super) async fn retry_failed_batch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RetryFailedResponse>, AppError> {
    // The retry loop re-parses each failed file (disk I/O + SQLite writes per
    // row). Running it directly on the async runtime holds a tokio worker for
    // the whole batch, which is what made this endpoint "hang" when the UI
    // dispatched retries concurrently with scan or view-details requests.
    tokio::task::spawn_blocking(move || -> Result<Json<RetryFailedResponse>, AppError> {
        // Verify the batch exists so callers get a 400 on typos.
        state
            .library
            .store
            .get_import_batch(&id)
            .map_err(AppError::Midi)?
            .ok_or_else(|| AppError::BadRequest(format!("import batch '{}' not found", id)))?;

        let originals = state
            .library
            .store
            .list_batch_entries(&id)
            .map_err(AppError::Midi)?;
        let failed: Vec<FileIndexEntry> = originals
            .into_iter()
            .filter(|e| e.status == FileIngestStatus::Failed)
            .collect();
        let processed = failed.len() as u32;

        let mut succeeded = 0u32;
        let mut still_failed = 0u32;
        let mut updated: Vec<FileIndexEntry> = Vec::with_capacity(failed.len());
        let import_opts = state.midi.import_options.clone();
        for entry in failed {
            let retried = ingest::retry_failed(&state.library.store, entry, &import_opts)
                .map_err(AppError::Midi)?;
            match retried.status {
                FileIngestStatus::Failed => still_failed += 1,
                _ => succeeded += 1,
            }
            state
                .library
                .store
                .replace_file_index_entry(retried.clone())
                .map_err(AppError::Midi)?;
            updated.push(retried);
        }

        // Refresh the batch counters to reflect the retry outcome.
        let all_entries = state
            .library
            .store
            .list_batch_entries(&id)
            .map_err(AppError::Midi)?;
        let tally = tally_entries(&all_entries);
        state
            .library
            .store
            .finish_import_batch(
                &id,
                tally.found,
                tally.imported,
                tally.duplicates,
                tally.unsupported,
                tally.failed,
            )
            .map_err(AppError::Midi)?;

        Ok(Json(RetryFailedResponse {
            processed,
            succeeded,
            still_failed,
            entries: updated,
        }))
    })
    .await
    .map_err(|e| {
        AppError::Midi(crate::error::Td3Error::Other(format!(
            "join retry_failed_batch: {}",
            e
        )))
    })?
}

/// Delete an import batch and every catalog row exclusively owned by it.
/// Files on disk are never touched - this only clears library-side state.
/// See `LibraryStore::delete_import_batch` for the exact semantics.
pub(super) async fn delete_import_batch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<DeleteImportBatchReport>, AppError> {
    state
        .library
        .store
        .get_import_batch(&id)
        .map_err(AppError::Midi)?
        .ok_or_else(|| AppError::BadRequest(format!("import batch '{}' not found", id)))?;

    let report = state
        .library
        .store
        .delete_import_batch(&id)
        .map_err(AppError::Midi)?;
    Ok(Json(report))
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BatchTally {
    pub(super) found: u32,
    pub(super) imported: u32,
    pub(super) duplicates: u32,
    pub(super) unsupported: u32,
    pub(super) failed: u32,
}

pub(super) fn tally_entries(entries: &[FileIndexEntry]) -> BatchTally {
    let mut t = BatchTally {
        found: entries.len() as u32,
        ..BatchTally::default()
    };
    for e in entries {
        match e.status {
            FileIngestStatus::Imported => t.imported += 1,
            FileIngestStatus::DuplicateSkipped => t.duplicates += 1,
            FileIngestStatus::Unsupported => t.unsupported += 1,
            FileIngestStatus::Failed => t.failed += 1,
            FileIngestStatus::Discovered | FileIngestStatus::Parsed => {}
        }
    }
    t
}
