use serde::{Deserialize, Serialize};

use crate::library::model::{FileIndexEntry, ImportBatch};

#[derive(Deserialize)]
pub struct ScanRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: Option<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct ScanResponse {
    pub batch_id: String,
    pub entries: Vec<FileIndexEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Serialize, Deserialize)]
pub struct ScanStartResponse {
    pub job_id: String,
    pub status: ScanJobStatus,
    pub path: String,
    pub found: usize,
    pub parsed: usize,
    pub started_at_epoch_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ScanJobResponse {
    pub job_id: String,
    pub status: ScanJobStatus,
    pub found: usize,
    pub parsed: usize,
    pub path: String,
    pub error: Option<String>,
    pub batch_id: Option<String>,
    pub entries: Vec<FileIndexEntry>,
    pub started_at_epoch_ms: u64,
    pub finished_at_epoch_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct ScanProgressResponse {
    pub running: bool,
    pub found: usize,
    pub parsed: usize,
    pub path: String,
    pub error: Option<String>,
    pub generation: usize,
}

#[derive(Serialize, Deserialize)]
pub struct BrowseFolderResponse {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct ImportRequest {
    pub paths: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ImportResponse {
    pub batch_id: String,
    pub entries: Vec<FileIndexEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct ImportBatchesResponse {
    pub batches: Vec<ImportBatch>,
}

#[derive(Serialize, Deserialize)]
pub struct ImportBatchResponse {
    pub batch: ImportBatch,
    #[serde(default)]
    pub entries: Vec<FileIndexEntry>,
}

/// Response for `POST /api/bank/import-batches/:id/retry-failed`.
/// Reports how many previously-failed rows were re-processed and the split
/// between new successes and lingering failures.
#[derive(Serialize, Deserialize)]
pub struct RetryFailedResponse {
    pub processed: u32,
    pub succeeded: u32,
    pub still_failed: u32,
    pub entries: Vec<FileIndexEntry>,
}
