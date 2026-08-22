//! HTTP-level tests for `/api/device/filter-cutoff` and
//! `/api/device/pitch-bend`. Without a MIDI port the handlers can only
//! be exercised through their validation paths: out-of-range values and
//! bad channels are rejected before any session lookup, and an in-range
//! value with no session is rejected as "not connected". Capability and
//! byte encoding are covered as pure functions in
//! `device_capabilities_tests.rs`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::library::LibraryStore;
use crate::web::api_types::ErrorBody;
use crate::web::handlers;
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_library() -> Arc<LibraryStore> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("td3-devctl-test-{}-{}.json", std::process::id(), n));
    let _ = std::fs::remove_file(&path);
    Arc::new(LibraryStore::load_or_create(path).expect("test library"))
}

fn build_router() -> Router {
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
    Router::new()
        .route(
            "/api/device/filter-cutoff",
            post(handlers::device_filter_cutoff),
        )
        .route("/api/device/pitch-bend", post(handlers::device_pitch_bend))
        .with_state(state)
}

async fn post_json(path: &str, body: &str) -> (StatusCode, ErrorBody) {
    let app = build_router();
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    let status = resp.status();
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    let parsed: ErrorBody = serde_json::from_slice(&bytes).expect("error body JSON");
    (status, parsed)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_cutoff_rejects_value_above_127() {
    let (status, body) = post_json("/api/device/filter-cutoff", r#"{"value":128}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.error.contains("0-127"), "got: {}", body.error);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_cutoff_rejects_when_not_connected() {
    let (status, body) = post_json("/api/device/filter-cutoff", r#"{"value":64}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.error, "not connected");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_cutoff_rejects_bad_channel_before_session_lookup() {
    let (status, body) = post_json(
        "/api/device/filter-cutoff",
        r#"{"value":64,"midiChannel":17}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.error.contains("midi channel must be 1-16"),
        "got: {}",
        body.error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_cutoff_rejects_negative_value_as_malformed_json() {
    let (status, body) = post_json("/api/device/filter-cutoff", r#"{"value":-1}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.error.contains("invalid filter cutoff JSON"),
        "got: {}",
        body.error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pitch_bend_rejects_value_above_16383() {
    let (status, body) = post_json("/api/device/pitch-bend", r#"{"value":16384}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.error.contains("0-16383"), "got: {}", body.error);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pitch_bend_rejects_when_not_connected() {
    let (status, body) = post_json("/api/device/pitch-bend", r#"{"value":8192}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.error, "not connected");
}
