//! Cross-page handoff queue for "Add to Control" from the Bank UI.
//!
//! Bank-side surfaces (cards, snapshot detail) POST one or more decoded
//! patterns to `/api/control/queue/append`. The Control page consumes the
//! buffered patterns on boot via `GET /api/control/queue/consume`, which
//! drains the queue atomically. A `BroadcastChannel` on the JS side lets
//! a live Control tab pull immediately; this server-side queue is the
//! durable backing store that survives a closed Control tab.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::api_types::WebPattern;
use super::handlers::AppError;
use super::state::{AppState, PlaybackState};

/// Maximum patterns retained in the queue. Matches the Control page's
/// `MAX_PATTERNS = 64`; any extras posted past this are dropped and
/// reported back to the caller.
pub const MAX_QUEUE: usize = 64;

#[derive(Default)]
pub struct ControlQueue {
    inner: Mutex<Vec<WebPattern>>,
}

impl ControlQueue {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }
}

#[derive(Deserialize)]
pub struct AppendRequest {
    pub patterns: Vec<WebPattern>,
}

#[derive(Serialize, Deserialize)]
pub struct AppendResponse {
    pub queued: usize,
    pub dropped: usize,
    pub queue_len: usize,
}

#[derive(Serialize, Deserialize)]
pub struct ConsumeResponse {
    pub patterns: Vec<WebPattern>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/control/queue/append", post(append))
        .route("/control/queue/consume", get(consume))
}

async fn append(
    State(state): State<PlaybackState>,
    payload: Result<Json<AppendRequest>, JsonRejection>,
) -> Result<Json<AppendResponse>, AppError> {
    let req = payload
        .map(|Json(req)| req)
        .map_err(|err| AppError::BadRequest(format!("invalid control queue JSON: {}", err)))?;
    if req.patterns.is_empty() {
        return Err(AppError::BadRequest("patterns must be non-empty".into()));
    }
    for (idx, pat) in req.patterns.iter().enumerate() {
        pat.to_pattern().map_err(|e| {
            let mut message = String::from("pattern[");
            message.push_str(&idx.to_string());
            message.push_str("] invalid: ");
            message.push_str(&e.to_string());
            AppError::BadRequest(message)
        })?;
    }

    let mut q = state.control_queue.inner.lock().await;
    let room = MAX_QUEUE.saturating_sub(q.len());
    let take = req.patterns.len().min(room);
    let dropped = req.patterns.len() - take;
    for p in req.patterns.into_iter().take(take) {
        q.push(p);
    }
    Ok(Json(AppendResponse {
        queued: take,
        dropped,
        queue_len: q.len(),
    }))
}

async fn consume(State(state): State<PlaybackState>) -> Json<ConsumeResponse> {
    let mut q = state.control_queue.inner.lock().await;
    let patterns = std::mem::take(&mut *q);
    Json(ConsumeResponse { patterns })
}
