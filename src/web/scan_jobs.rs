use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::library::FileIndexEntry;

use super::api_types::{ScanJobResponse, ScanJobStatus, ScanResponse, ScanStartResponse};

pub(crate) struct ScanJobRegistry {
    next_id: AtomicU64,
    jobs: Mutex<BTreeMap<String, ScanJob>>,
}

#[derive(Clone)]
struct ScanJob {
    job_id: String,
    status: ScanJobStatus,
    found: usize,
    parsed: usize,
    path: String,
    error: Option<String>,
    batch_id: Option<String>,
    entries: Vec<FileIndexEntry>,
    started_at_epoch_ms: u64,
    finished_at_epoch_ms: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct ActiveScan {
    pub job_id: String,
}

impl ScanJobRegistry {
    pub(crate) fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            jobs: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn start(&self, path: String) -> Result<ScanStartResponse, ActiveScan> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(active) = jobs.values().find(|job| job.is_active()) {
            return Err(ActiveScan {
                job_id: active.job_id.clone(),
            });
        }

        let n = self.next_id.fetch_add(1, Ordering::Relaxed);
        let job_id = format!("scan_{}_{}", std::process::id(), n);
        let started_at_epoch_ms = current_epoch_millis();
        let job = ScanJob {
            job_id: job_id.clone(),
            status: ScanJobStatus::Queued,
            found: 0,
            parsed: 0,
            path: path.clone(),
            error: None,
            batch_id: None,
            entries: Vec::new(),
            started_at_epoch_ms,
            finished_at_epoch_ms: None,
        };
        jobs.insert(job_id.clone(), job);

        Ok(ScanStartResponse {
            job_id,
            status: ScanJobStatus::Queued,
            path,
            found: 0,
            parsed: 0,
            started_at_epoch_ms,
        })
    }

    pub(crate) fn mark_running(&self, job_id: &str) {
        self.update(job_id, |job| {
            if job.status == ScanJobStatus::Queued {
                job.status = ScanJobStatus::Running;
            }
        });
    }

    pub(crate) fn set_found(&self, job_id: &str, found: usize) {
        self.update(job_id, |job| {
            job.found = found;
        });
    }

    pub(crate) fn set_parsed(&self, job_id: &str, parsed: usize) {
        self.update(job_id, |job| {
            job.parsed = parsed;
        });
    }

    pub(crate) fn complete(&self, job_id: &str, response: ScanResponse) {
        self.update(job_id, |job| {
            job.status = ScanJobStatus::Completed;
            job.parsed = response.entries.len();
            job.found = job.found.max(response.entries.len());
            job.batch_id = Some(response.batch_id);
            job.entries = response.entries;
            job.finished_at_epoch_ms = Some(current_epoch_millis());
        });
    }

    pub(crate) fn fail(&self, job_id: &str, message: String) {
        self.update(job_id, |job| {
            job.status = ScanJobStatus::Failed;
            job.error = Some(message);
            job.finished_at_epoch_ms = Some(current_epoch_millis());
        });
    }

    pub(crate) fn get(&self, job_id: &str) -> Option<ScanJobResponse> {
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.get(job_id).map(ScanJob::response)
    }

    fn update<F>(&self, job_id: &str, f: F)
    where
        F: FnOnce(&mut ScanJob),
    {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(job) = jobs.get_mut(job_id) {
            f(job);
        }
    }
}

impl Default for ScanJobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanJob {
    fn is_active(&self) -> bool {
        matches!(self.status, ScanJobStatus::Queued | ScanJobStatus::Running)
    }

    fn response(&self) -> ScanJobResponse {
        ScanJobResponse {
            job_id: self.job_id.clone(),
            status: self.status,
            found: self.found,
            parsed: self.parsed,
            path: self.path.clone(),
            error: self.error.clone(),
            batch_id: self.batch_id.clone(),
            entries: self.entries.clone(),
            started_at_epoch_ms: self.started_at_epoch_ms,
            finished_at_epoch_ms: self.finished_at_epoch_ms,
        }
    }
}

fn current_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
