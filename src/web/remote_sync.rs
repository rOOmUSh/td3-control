use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::{Mutex, Notify};

use super::api_types::{
    RemoteSyncCommand, RemoteSyncCommandKind, RemoteSyncCommandResponse, RemoteSyncPollResponse,
    RemoteSyncProbeRequest, RemoteSyncProbeResponse, RemoteSyncRelayRequest,
};
use super::handlers::{json_payload, AppError};
use super::start_schedule::MAX_START_DELAY_MICROS;
use super::state::AppState;

const POLL_TIMEOUT: Duration = Duration::from_secs(25);
const RELAY_TIMEOUT: Duration = Duration::from_millis(1500);
const PROBE_TIMEOUT: Duration = Duration::from_millis(700);
const MAX_QUEUE: usize = 8;

#[derive(Default)]
pub struct RemoteSyncQueue {
    queue: Mutex<VecDeque<RemoteSyncCommand>>,
    notify: Notify,
    listener_count: AtomicUsize,
}

impl RemoteSyncQueue {
    pub fn new() -> Self {
        Self::default()
    }

    async fn push(&self, command: RemoteSyncCommand) -> Result<(), AppError> {
        if self.listener_count.load(Ordering::Acquire) == 0 {
            return Err(AppError::Conflict("remote UI not listening".to_string()));
        }

        let mut queue = self.queue.lock().await;
        if queue.len() >= MAX_QUEUE {
            let _ = queue.pop_front();
        }
        queue.push_back(command);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn pop_waiting(&self) -> Option<RemoteSyncCommand> {
        let _listener = PollListener::new(&self.listener_count);
        loop {
            if let Some(command) = self.queue.lock().await.pop_front() {
                return Some(command);
            }

            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(POLL_TIMEOUT) => return None,
            }
        }
    }

    #[cfg(test)]
    pub fn listener_count(&self) -> usize {
        self.listener_count.load(Ordering::Acquire)
    }
}

struct PollListener<'a> {
    count: &'a AtomicUsize,
}

impl<'a> PollListener<'a> {
    fn new(count: &'a AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self { count }
    }
}

impl Drop for PollListener<'_> {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/remote-sync/relay", post(relay))
        .route("/remote-sync/probe", post(probe))
        .route("/remote-sync/command", post(command))
        .route("/remote-sync/poll", get(poll))
}

async fn probe(
    payload: Result<Json<RemoteSyncProbeRequest>, JsonRejection>,
) -> Result<Json<RemoteSyncProbeResponse>, AppError> {
    let req = json_payload(payload, "remote sync probe")?;
    validate_port(req.port)?;
    probe_remote_server(req.port).await?;
    Ok(Json(RemoteSyncProbeResponse { ok: true }))
}

async fn relay(
    payload: Result<Json<RemoteSyncRelayRequest>, JsonRejection>,
) -> Result<Json<RemoteSyncCommandResponse>, AppError> {
    let req = json_payload(payload, "remote sync relay")?;
    validate_port(req.port)?;
    validate_command_fields(
        req.command.clone(),
        req.centibpm,
        req.target_epoch_micros,
        req.triplet,
    )?;

    let port = req.port;
    let command: RemoteSyncCommand = req.into();
    let url = format!("http://127.0.0.1:{}/api/remote-sync/command", port);
    let client = reqwest::Client::builder()
        .timeout(RELAY_TIMEOUT)
        .build()
        .map_err(|err| AppError::Internal(format!("remote sync client: {}", err)))?;

    let response = client
        .post(url)
        .json(&command)
        .send()
        .await
        .map_err(|err| AppError::BadRequest(format!("remote sync relay failed: {}", err)))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "remote sync relay failed: HTTP {} {}",
            status, text
        )));
    }

    Ok(Json(RemoteSyncCommandResponse {
        ok: true,
        queued: true,
    }))
}

async fn command(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<RemoteSyncCommand>, JsonRejection>,
) -> Result<Json<RemoteSyncCommandResponse>, AppError> {
    let command = json_payload(payload, "remote sync command")?;
    validate_command_fields(
        command.command.clone(),
        command.centibpm,
        command.target_epoch_micros,
        command.triplet,
    )?;
    state.playback.remote_sync.push(command).await?;
    Ok(Json(RemoteSyncCommandResponse {
        ok: true,
        queued: true,
    }))
}

async fn poll(State(state): State<Arc<AppState>>) -> Json<RemoteSyncPollResponse> {
    let command = state.playback.remote_sync.pop_waiting().await;
    Json(RemoteSyncPollResponse {
        ok: command.is_some(),
        command,
    })
}

fn validate_port(port: u16) -> Result<(), AppError> {
    if port == 0 {
        return Err(AppError::BadRequest(
            "remote port must be between 1 and 65535".to_string(),
        ));
    }
    Ok(())
}

fn validate_command_fields(
    command: RemoteSyncCommandKind,
    centibpm: Option<u32>,
    target_epoch_micros: Option<u64>,
    triplet: Option<bool>,
) -> Result<(), AppError> {
    if let Some(value) = centibpm {
        if value == 0 || value > 30_000 {
            return Err(AppError::BadRequest(format!(
                "centi-BPM must be 1-30000, got {}",
                value
            )));
        }
    }
    if let Some(target) = target_epoch_micros {
        let now = super::start_schedule::current_epoch_micros();
        if target > now.saturating_add(MAX_START_DELAY_MICROS) {
            return Err(AppError::BadRequest(
                "targetEpochMicros must be within 60 seconds".to_string(),
            ));
        }
    }
    match command {
        RemoteSyncCommandKind::Play => {
            if centibpm.is_none() || target_epoch_micros.is_none() {
                return Err(AppError::BadRequest(
                    "play requires centibpm and targetEpochMicros".to_string(),
                ));
            }
        }
        RemoteSyncCommandKind::Bpm => {
            if centibpm.is_none() {
                return Err(AppError::BadRequest("bpm requires centibpm".to_string()));
            }
        }
        RemoteSyncCommandKind::Triplet => {
            if triplet.is_none() {
                return Err(AppError::BadRequest("triplet requires triplet".to_string()));
            }
        }
        RemoteSyncCommandKind::Stop => {}
    }
    Ok(())
}

async fn probe_remote_server(port: u16) -> Result<(), AppError> {
    let url = format!("http://127.0.0.1:{}/api/status", port);
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|err| AppError::Internal(format!("remote sync probe client: {}", err)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| AppError::BadRequest(format!("No server on port {}", port)))?;

    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "No td3-control server on port {}",
            port
        )));
    }

    Ok(())
}
