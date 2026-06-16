use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinSet;

use super::api_types::{
    RemoteSyncCommand, RemoteSyncCommandKind, RemoteSyncCommandResponse, RemoteSyncPollResponse,
    RemoteSyncProbeRequest, RemoteSyncProbeResponse, RemoteSyncProbeResult, RemoteSyncRelayRequest,
    RemoteSyncRelayResult,
};
use super::handlers::{json_payload, AppError};
use super::start_schedule::MAX_START_DELAY_MICROS;
use super::state::AppState;

const POLL_TIMEOUT: Duration = Duration::from_secs(25);
const RELAY_TIMEOUT: Duration = Duration::from_millis(1500);
const PROBE_TIMEOUT: Duration = Duration::from_millis(700);
const MAX_QUEUE: usize = 8;
const MAX_REMOTE_PORTS: usize = 8;

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
    let ports = normalize_remote_ports(req.port, req.ports)?;
    let single_port_request = ports.len() == 1;
    let results = probe_remote_servers(&ports).await?;
    let response = probe_response_from_results(results);
    if !response.ok && single_port_request {
        return Err(AppError::BadRequest(first_probe_error(&response)));
    }
    Ok(Json(response))
}

async fn relay(
    payload: Result<Json<RemoteSyncRelayRequest>, JsonRejection>,
) -> Result<Json<RemoteSyncCommandResponse>, AppError> {
    let req = json_payload(payload, "remote sync relay")?;
    let command = req.command_payload();
    let ports = normalize_remote_ports(req.port, req.ports)?;
    validate_command_fields(
        command.command.clone(),
        command.centibpm,
        command.target_epoch_micros,
        command.triplet,
    )?;

    let single_port_request = ports.len() == 1;
    let results = relay_command_to_ports(&ports, command).await?;
    let response = command_response_from_results(results);
    if !response.ok && single_port_request {
        return Err(AppError::BadRequest(first_relay_error(&response)));
    }

    Ok(Json(response))
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
        results: Vec::new(),
    }))
}

async fn poll(State(state): State<Arc<AppState>>) -> Json<RemoteSyncPollResponse> {
    let command = state.playback.remote_sync.pop_waiting().await;
    Json(RemoteSyncPollResponse {
        ok: command.is_some(),
        command,
    })
}

pub(crate) fn normalize_remote_ports(
    port: Option<u32>,
    ports: Option<Vec<u32>>,
) -> Result<Vec<u16>, AppError> {
    let values = match (port, ports) {
        (Some(_), Some(_)) => {
            return Err(AppError::BadRequest(
                "remote sync request must not include both port and ports".to_string(),
            ));
        }
        (Some(value), None) => vec![value],
        (None, Some(values)) => values,
        (None, None) => {
            return Err(AppError::BadRequest(
                "remote sync request requires port or ports".to_string(),
            ));
        }
    };

    if values.is_empty() {
        return Err(AppError::BadRequest(
            "remote sync request requires at least one port".to_string(),
        ));
    }

    let mut normalized = Vec::new();
    for value in values {
        if value == 0 || value > u16::MAX as u32 {
            return Err(AppError::BadRequest(
                "remote port must be between 1 and 65535".to_string(),
            ));
        }
        let port = value as u16;
        if normalized.contains(&port) {
            continue;
        }
        normalized.push(port);
        if normalized.len() > MAX_REMOTE_PORTS {
            return Err(AppError::BadRequest(format!(
                "remote sync supports at most {} remote ports",
                MAX_REMOTE_PORTS
            )));
        }
    }

    if normalized.is_empty() {
        return Err(AppError::BadRequest(
            "remote sync request requires at least one port".to_string(),
        ));
    }

    Ok(normalized)
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

async fn probe_remote_servers(ports: &[u16]) -> Result<Vec<RemoteSyncProbeResult>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|err| AppError::Internal(format!("remote sync probe client: {}", err)))?;

    let mut tasks = JoinSet::new();
    for (index, port) in ports.iter().copied().enumerate() {
        let client = client.clone();
        tasks.spawn(async move { (index, probe_remote_server_with_client(client, port).await) });
    }

    let mut ordered = vec![None; ports.len()];
    while let Some(joined) = tasks.join_next().await {
        let (index, result) =
            joined.map_err(|err| AppError::Internal(format!("remote sync probe task: {}", err)))?;
        if let Some(slot) = ordered.get_mut(index) {
            *slot = Some(result);
        }
    }

    collect_probe_results(ordered)
}

async fn relay_command_to_ports(
    ports: &[u16],
    command: RemoteSyncCommand,
) -> Result<Vec<RemoteSyncRelayResult>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(RELAY_TIMEOUT)
        .build()
        .map_err(|err| AppError::Internal(format!("remote sync client: {}", err)))?;

    let mut tasks = JoinSet::new();
    for (index, port) in ports.iter().copied().enumerate() {
        let client = client.clone();
        let command = command.clone();
        tasks.spawn(async move { (index, relay_command_to_port(client, port, command).await) });
    }

    let mut ordered = vec![None; ports.len()];
    while let Some(joined) = tasks.join_next().await {
        let (index, result) =
            joined.map_err(|err| AppError::Internal(format!("remote sync relay task: {}", err)))?;
        if let Some(slot) = ordered.get_mut(index) {
            *slot = Some(result);
        }
    }

    collect_relay_results(ordered)
}

async fn probe_remote_server_with_client(
    client: reqwest::Client,
    port: u16,
) -> RemoteSyncProbeResult {
    let url = format!("http://127.0.0.1:{}/api/status", port);
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(_) => return probe_failure(port, format!("No server on port {}", port)),
    };

    if !response.status().is_success() {
        return probe_failure(port, format!("No td3-control server on port {}", port));
    }

    RemoteSyncProbeResult {
        port,
        ok: true,
        error: None,
    }
}

async fn relay_command_to_port(
    client: reqwest::Client,
    port: u16,
    command: RemoteSyncCommand,
) -> RemoteSyncRelayResult {
    let url = format!("http://127.0.0.1:{}/api/remote-sync/command", port);
    let response = match client.post(url).json(&command).send().await {
        Ok(response) => response,
        Err(err) => {
            return relay_failure(port, format!("remote sync relay failed: {}", err));
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let text = match response.text().await {
            Ok(text) => text,
            Err(err) => format!("response body unavailable: {}", err),
        };
        return relay_failure(
            port,
            format!("remote sync relay failed: HTTP {} {}", status, text),
        );
    }

    let body = match response.json::<RemoteSyncCommandResponse>().await {
        Ok(body) => body,
        Err(err) => {
            return relay_failure(
                port,
                format!("remote sync relay response was invalid: {}", err),
            );
        }
    };

    if body.ok && body.queued {
        return RemoteSyncRelayResult {
            port,
            ok: true,
            queued: true,
            error: None,
        };
    }

    relay_failure(port, relay_body_error(&body))
}

fn collect_probe_results(
    ordered: Vec<Option<RemoteSyncProbeResult>>,
) -> Result<Vec<RemoteSyncProbeResult>, AppError> {
    let mut results = Vec::with_capacity(ordered.len());
    for result in ordered {
        match result {
            Some(result) => results.push(result),
            None => {
                return Err(AppError::Internal(
                    "remote sync probe result missing".to_string(),
                ));
            }
        }
    }
    Ok(results)
}

fn collect_relay_results(
    ordered: Vec<Option<RemoteSyncRelayResult>>,
) -> Result<Vec<RemoteSyncRelayResult>, AppError> {
    let mut results = Vec::with_capacity(ordered.len());
    for result in ordered {
        match result {
            Some(result) => results.push(result),
            None => {
                return Err(AppError::Internal(
                    "remote sync relay result missing".to_string(),
                ));
            }
        }
    }
    Ok(results)
}

fn command_response_from_results(results: Vec<RemoteSyncRelayResult>) -> RemoteSyncCommandResponse {
    let ok = results.iter().all(|result| result.ok);
    let queued = results.iter().all(|result| result.queued);
    RemoteSyncCommandResponse {
        ok,
        queued,
        results,
    }
}

fn probe_response_from_results(results: Vec<RemoteSyncProbeResult>) -> RemoteSyncProbeResponse {
    let ok = results.iter().all(|result| result.ok);
    RemoteSyncProbeResponse { ok, results }
}

fn relay_body_error(body: &RemoteSyncCommandResponse) -> String {
    for result in &body.results {
        if !result.ok || !result.queued {
            if let Some(error) = &result.error {
                return error.clone();
            }
        }
    }
    "remote server did not queue command".to_string()
}

fn first_relay_error(response: &RemoteSyncCommandResponse) -> String {
    response
        .results
        .first()
        .and_then(|result| result.error.clone())
        .unwrap_or_else(|| "remote sync relay failed".to_string())
}

fn first_probe_error(response: &RemoteSyncProbeResponse) -> String {
    response
        .results
        .first()
        .and_then(|result| result.error.clone())
        .unwrap_or_else(|| "remote sync probe failed".to_string())
}

fn relay_failure(port: u16, error: String) -> RemoteSyncRelayResult {
    RemoteSyncRelayResult {
        port,
        ok: false,
        queued: false,
        error: Some(error),
    }
}

fn probe_failure(port: u16, error: String) -> RemoteSyncProbeResult {
    RemoteSyncProbeResult {
        port,
        ok: false,
        error: Some(error),
    }
}
