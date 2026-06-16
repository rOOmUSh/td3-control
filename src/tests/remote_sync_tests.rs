use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use http_body_util::BodyExt;
use tokio::sync::Mutex;
use tower::ServiceExt;

use crate::library::LibraryStore;
use crate::web::api_types::{
    RemoteSyncCommand, RemoteSyncCommandKind, RemoteSyncCommandResponse, RemoteSyncPollResponse,
    RemoteSyncProbeResponse,
};
use crate::web::remote_sync;
use crate::web::start_schedule;
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_library() -> Arc<LibraryStore> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("td3-remote-sync-test-{}-{}.json", pid, n));
    let _ = std::fs::remove_file(&path);
    Arc::new(LibraryStore::load_or_create(path).expect("test library"))
}

fn build_router() -> (Router, Arc<AppState>) {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        temp_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let router = Router::new()
        .nest("/api", remote_sync::router())
        .with_state(state.clone());
    (router, state)
}

async fn json_body<T: serde::de::DeserializeOwned>(body: Body) -> T {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn post_json(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn poll_request() -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/api/remote-sync/poll")
        .body(Body::empty())
        .unwrap()
}

async fn spawn_status_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new().route(
        "/api/status",
        get(|| async { Json(serde_json::json!({ "connected": false })) }),
    );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    port
}

async fn spawn_remote_sync_server() -> (u16, Arc<Mutex<Vec<RemoteSyncCommand>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let command_log = commands.clone();
    let app = Router::new()
        .route(
            "/api/status",
            get(|| async { Json(serde_json::json!({ "connected": false })) }),
        )
        .route(
            "/api/remote-sync/command",
            post(move |Json(command): Json<RemoteSyncCommand>| {
                let command_log = command_log.clone();
                async move {
                    command_log.lock().await.push(command);
                    Json(RemoteSyncCommandResponse {
                        ok: true,
                        queued: true,
                        results: Vec::new(),
                    })
                }
            }),
        );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (port, commands)
}

async fn closed_local_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn captured_commands(
    commands: &Arc<Mutex<Vec<RemoteSyncCommand>>>,
) -> Vec<RemoteSyncCommand> {
    commands.lock().await.clone()
}

#[tokio::test]
async fn command_rejects_when_no_remote_ui_is_polling() {
    let (router, _state) = build_router();
    let target = start_schedule::current_epoch_micros() + 1_000_000;
    let body = serde_json::json!({
        "command": "play",
        "centibpm": 12500,
        "targetEpochMicros": target,
    })
    .to_string();

    let resp = router
        .oneshot(post_json("/api/remote-sync/command", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn command_delivers_to_waiting_poll() {
    let (router, state) = build_router();
    let poll_task = tokio::spawn(router.clone().oneshot(poll_request()));

    for _ in 0..50 {
        if state.playback.remote_sync.listener_count() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(state.playback.remote_sync.listener_count(), 1);

    let target = start_schedule::current_epoch_micros() + 1_000_000;
    let body = serde_json::json!({
        "command": "play",
        "centibpm": 12500,
        "targetEpochMicros": target,
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/command", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let poll_resp = poll_task.await.unwrap().unwrap();
    assert_eq!(poll_resp.status(), StatusCode::OK);
    let payload: RemoteSyncPollResponse = json_body(poll_resp.into_body()).await;
    assert!(payload.ok);
    let command = payload.command.expect("queued command");
    assert_eq!(command.command, RemoteSyncCommandKind::Play);
    assert_eq!(command.centibpm, Some(12500));
    assert_eq!(command.target_epoch_micros, Some(target));
}

#[tokio::test]
async fn command_delivers_triplet_to_waiting_poll() {
    let (router, state) = build_router();
    let poll_task = tokio::spawn(router.clone().oneshot(poll_request()));

    for _ in 0..50 {
        if state.playback.remote_sync.listener_count() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(state.playback.remote_sync.listener_count(), 1);

    let body = serde_json::json!({
        "command": "triplet",
        "triplet": true,
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/command", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let poll_resp = poll_task.await.unwrap().unwrap();
    assert_eq!(poll_resp.status(), StatusCode::OK);
    let payload: RemoteSyncPollResponse = json_body(poll_resp.into_body()).await;
    assert!(payload.ok);
    let command = payload.command.expect("queued command");
    assert_eq!(command.command, RemoteSyncCommandKind::Triplet);
    assert_eq!(command.triplet, Some(true));
}

#[tokio::test]
async fn probe_accepts_running_remote_server() {
    let port = spawn_status_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({ "port": port }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncProbeResponse = json_body(resp.into_body()).await;
    assert!(payload.ok);
}

#[tokio::test]
async fn probe_accepts_multiple_running_remote_servers() {
    let port_a = spawn_status_server().await;
    let port_b = spawn_status_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({ "ports": [port_a, port_b] }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncProbeResponse = json_body(resp.into_body()).await;
    assert!(payload.ok);
    assert_eq!(payload.results.len(), 2);
    assert_eq!(payload.results[0].port, port_a);
    assert!(payload.results[0].ok);
    assert_eq!(payload.results[1].port, port_b);
    assert!(payload.results[1].ok);
}

#[tokio::test]
async fn probe_deduplicates_ports_before_network_io() {
    let port = spawn_status_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({ "ports": [port, port] }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncProbeResponse = json_body(resp.into_body()).await;
    assert!(payload.ok);
    assert_eq!(payload.results.len(), 1);
    assert_eq!(payload.results[0].port, port);
}

#[tokio::test]
async fn probe_reports_partial_failure_with_per_port_results() {
    let good_port = spawn_status_server().await;
    let closed_port = closed_local_port().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({ "ports": [good_port, closed_port] }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncProbeResponse = json_body(resp.into_body()).await;
    assert!(!payload.ok);
    assert_eq!(payload.results.len(), 2);
    assert!(payload.results[0].ok);
    assert_eq!(payload.results[0].port, good_port);
    assert!(!payload.results[1].ok);
    assert_eq!(payload.results[1].port, closed_port);
    assert_eq!(
        payload.results[1].error.as_deref(),
        Some(format!("No server on port {}", closed_port).as_str())
    );
}

#[tokio::test]
async fn probe_reports_no_server_on_closed_port() {
    let port = closed_local_port().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({ "port": port }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let payload: serde_json::Value = json_body(resp.into_body()).await;
    assert_eq!(payload["error"], format!("No server on port {}", port));
}

#[tokio::test]
async fn probe_rejects_ambiguous_single_and_multi_port_payload() {
    let port = spawn_status_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({ "port": port, "ports": [port] }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn probe_rejects_empty_multi_port_payload() {
    let (router, _state) = build_router();
    let body = serde_json::json!({ "ports": [] }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn probe_rejects_invalid_multi_port_value() {
    let (router, _state) = build_router();
    let body = serde_json::json!({ "ports": [65536] }).to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/probe", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn command_rejects_invalid_centibpm() {
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "command": "bpm",
        "centibpm": 30001,
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/command", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn command_rejects_triplet_without_value() {
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "command": "triplet",
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/command", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn relay_rejects_port_zero_before_network_io() {
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "port": 0,
        "command": "stop",
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/relay", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn relay_accepts_single_port_request_shape() {
    let (remote_port, commands) = spawn_remote_sync_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "port": remote_port,
        "command": "stop",
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/relay", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncCommandResponse = json_body(resp.into_body()).await;
    assert!(payload.ok);
    assert!(payload.queued);
    assert_eq!(payload.results.len(), 1);
    assert_eq!(payload.results[0].port, remote_port);
    assert_eq!(captured_commands(&commands).await.len(), 1);
}

#[tokio::test]
async fn relay_sends_play_to_multiple_ports_with_same_target() {
    let (port_a, commands_a) = spawn_remote_sync_server().await;
    let (port_b, commands_b) = spawn_remote_sync_server().await;
    let (router, _state) = build_router();
    let target = start_schedule::current_epoch_micros() + 1_000_000;
    let body = serde_json::json!({
        "ports": [port_a, port_b],
        "command": "play",
        "centibpm": 12500,
        "targetEpochMicros": target,
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/relay", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncCommandResponse = json_body(resp.into_body()).await;
    assert!(payload.ok);
    assert!(payload.queued);
    assert_eq!(payload.results.len(), 2);
    assert_eq!(payload.results[0].port, port_a);
    assert_eq!(payload.results[1].port, port_b);

    let sent_a = captured_commands(&commands_a).await;
    let sent_b = captured_commands(&commands_b).await;
    assert_eq!(sent_a.len(), 1);
    assert_eq!(sent_b.len(), 1);
    assert_eq!(sent_a[0].command, RemoteSyncCommandKind::Play);
    assert_eq!(sent_b[0].command, RemoteSyncCommandKind::Play);
    assert_eq!(sent_a[0].centibpm, Some(12500));
    assert_eq!(sent_b[0].centibpm, Some(12500));
    assert_eq!(sent_a[0].target_epoch_micros, Some(target));
    assert_eq!(sent_b[0].target_epoch_micros, Some(target));
}

#[tokio::test]
async fn relay_deduplicates_ports_before_sending() {
    let (remote_port, commands) = spawn_remote_sync_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "ports": [remote_port, remote_port],
        "command": "stop",
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/relay", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncCommandResponse = json_body(resp.into_body()).await;
    assert!(payload.ok);
    assert_eq!(payload.results.len(), 1);
    assert_eq!(captured_commands(&commands).await.len(), 1);
}

#[tokio::test]
async fn relay_reports_partial_failure_with_per_port_results() {
    let (good_port, commands) = spawn_remote_sync_server().await;
    let closed_port = closed_local_port().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "ports": [good_port, closed_port],
        "command": "stop",
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/relay", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: RemoteSyncCommandResponse = json_body(resp.into_body()).await;
    assert!(!payload.ok);
    assert!(!payload.queued);
    assert_eq!(payload.results.len(), 2);
    assert!(payload.results[0].ok);
    assert!(payload.results[0].queued);
    assert_eq!(payload.results[0].port, good_port);
    assert!(!payload.results[1].ok);
    assert!(!payload.results[1].queued);
    assert_eq!(payload.results[1].port, closed_port);
    assert!(payload.results[1]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains(&closed_port.to_string()));
    assert_eq!(captured_commands(&commands).await.len(), 1);
}

#[tokio::test]
async fn relay_rejects_invalid_command_before_network_io() {
    let (remote_port, commands) = spawn_remote_sync_server().await;
    let (router, _state) = build_router();
    let body = serde_json::json!({
        "ports": [remote_port],
        "command": "play",
        "centibpm": 12500,
    })
    .to_string();
    let resp = router
        .oneshot(post_json("/api/remote-sync/relay", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(captured_commands(&commands).await.len(), 0);
}

#[test]
fn start_schedule_accepts_near_target() {
    let target = start_schedule::current_epoch_micros() + 1_000_000;
    let (epoch, delay) = start_schedule::resolve_start_target(Some(target)).unwrap();
    assert!(epoch >= target.saturating_sub(2_000));
    assert!(delay <= std::time::Duration::from_secs(1));
}

#[test]
fn start_schedule_rejects_far_target() {
    let target = start_schedule::current_epoch_micros() + 61_000_000;
    assert!(start_schedule::resolve_start_target(Some(target)).is_err());
}
