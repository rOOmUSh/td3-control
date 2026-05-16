//! HTTP-level tests for the `/api/control/queue/*` Add-to-Control handoff.
//!
//! Covers the durable backing store that survives a closed Control tab:
//! POST /append buffers patterns, GET /consume drains them atomically, the
//! queue caps at MAX_QUEUE and reports the drop count, malformed patterns
//! are rejected with 400, and consume on an empty queue returns an empty
//! list (not an error).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::library::LibraryStore;
use crate::web::api_types::{WebNote, WebPattern, WebStep, WebTime, WebTranspose};
use crate::web::control_queue::{self, AppendResponse, ConsumeResponse, MAX_QUEUE};
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_library() -> Arc<LibraryStore> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!(
        "td3-ctrlq-test-{}-{}-{}.json",
        pid,
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
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
        .nest("/api", control_queue::router())
        .with_state(state.clone());
    (router, state)
}

fn rest_step() -> WebStep {
    WebStep {
        note: WebNote::C,
        transpose: WebTranspose::Normal,
        accent: false,
        slide: false,
        time: WebTime::Rest,
    }
}

fn empty_pattern() -> WebPattern {
    WebPattern {
        active_steps: 16,
        triplet: false,
        steps: [rest_step(); 16],
    }
}

fn malformed_append_request_too_few_steps() -> Request<Body> {
    let valid = serde_json::to_string(&empty_pattern()).unwrap();
    let step = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"REST"}"#;
    let steps: Vec<&str> = (0..15).map(|_| step).collect();
    let invalid = format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    );
    Request::builder()
        .method("POST")
        .uri("/api/control/queue/append")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"patterns":[{},{}]}}"#,
            valid, invalid
        )))
        .unwrap()
}

async fn json_body<T: serde::de::DeserializeOwned>(body: Body) -> T {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn append_request(patterns: Vec<WebPattern>) -> Request<Body> {
    let body = serde_json::json!({ "patterns": patterns }).to_string();
    Request::builder()
        .method("POST")
        .uri("/api/control/queue/append")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn consume_request() -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/api/control/queue/consume")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn consume_empty_queue_returns_empty_list() {
    let (router, _state) = build_router();
    let resp = router.oneshot(consume_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: ConsumeResponse = json_body(resp.into_body()).await;
    assert!(payload.patterns.is_empty());
}

#[tokio::test]
async fn append_then_consume_roundtrips_in_order() {
    let (router, _state) = build_router();

    let mut p1 = empty_pattern();
    p1.active_steps = 8;
    let mut p2 = empty_pattern();
    p2.triplet = true;
    let mut p3 = empty_pattern();
    p3.active_steps = 12;

    let resp = router
        .clone()
        .oneshot(append_request(vec![p1.clone(), p2.clone(), p3.clone()]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let appended: AppendResponse = json_body(resp.into_body()).await;
    assert_eq!(appended.queued, 3);
    assert_eq!(appended.dropped, 0);
    assert_eq!(appended.queue_len, 3);

    let resp = router.oneshot(consume_request()).await.unwrap();
    let drained: ConsumeResponse = json_body(resp.into_body()).await;
    assert_eq!(drained.patterns.len(), 3);
    assert_eq!(drained.patterns[0].active_steps, 8);
    assert!(drained.patterns[1].triplet);
    assert_eq!(drained.patterns[2].active_steps, 12);
}

#[tokio::test]
async fn consume_drains_queue() {
    let (router, state) = build_router();
    let _ = router
        .clone()
        .oneshot(append_request(vec![empty_pattern(), empty_pattern()]))
        .await
        .unwrap();
    assert_eq!(state.playback.control_queue.len().await, 2);

    let _ = router.clone().oneshot(consume_request()).await.unwrap();
    assert_eq!(state.playback.control_queue.len().await, 0);

    let resp = router.oneshot(consume_request()).await.unwrap();
    let payload: ConsumeResponse = json_body(resp.into_body()).await;
    assert!(payload.patterns.is_empty());
}

#[tokio::test]
async fn appends_accumulate_across_calls() {
    let (router, _state) = build_router();
    let _ = router
        .clone()
        .oneshot(append_request(vec![empty_pattern(), empty_pattern()]))
        .await
        .unwrap();
    let _ = router
        .clone()
        .oneshot(append_request(vec![empty_pattern()]))
        .await
        .unwrap();
    let resp = router.oneshot(consume_request()).await.unwrap();
    let payload: ConsumeResponse = json_body(resp.into_body()).await;
    assert_eq!(payload.patterns.len(), 3);
}

#[tokio::test]
async fn empty_append_rejected() {
    let (router, _state) = build_router();
    let resp = router.oneshot(append_request(vec![])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn malformed_pattern_rejected_and_queue_unchanged() {
    let (router, state) = build_router();
    let _ = router
        .clone()
        .oneshot(append_request(vec![empty_pattern()]))
        .await
        .unwrap();
    assert_eq!(state.playback.control_queue.len().await, 1);

    let resp = router
        .oneshot(malformed_append_request_too_few_steps())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        state.playback.control_queue.len().await,
        1,
        "queue must not be partially mutated on validation failure"
    );
}

#[tokio::test]
async fn append_past_cap_drops_overflow_and_reports_count() {
    let (router, _state) = build_router();
    let batch1: Vec<WebPattern> = (0..MAX_QUEUE).map(|_| empty_pattern()).collect();
    let resp = router
        .clone()
        .oneshot(append_request(batch1))
        .await
        .unwrap();
    let appended: AppendResponse = json_body(resp.into_body()).await;
    assert_eq!(appended.queued, MAX_QUEUE);
    assert_eq!(appended.dropped, 0);
    assert_eq!(appended.queue_len, MAX_QUEUE);

    let resp = router
        .clone()
        .oneshot(append_request(vec![
            empty_pattern(),
            empty_pattern(),
            empty_pattern(),
        ]))
        .await
        .unwrap();
    let appended: AppendResponse = json_body(resp.into_body()).await;
    assert_eq!(appended.queued, 0);
    assert_eq!(appended.dropped, 3);
    assert_eq!(appended.queue_len, MAX_QUEUE);

    let resp = router.oneshot(consume_request()).await.unwrap();
    let drained: ConsumeResponse = json_body(resp.into_body()).await;
    assert_eq!(drained.patterns.len(), MAX_QUEUE);
}

#[tokio::test]
async fn partial_append_when_room_for_some() {
    let (router, _state) = build_router();
    let pre: Vec<WebPattern> = (0..MAX_QUEUE - 2).map(|_| empty_pattern()).collect();
    let _ = router.clone().oneshot(append_request(pre)).await.unwrap();

    let resp = router
        .oneshot(append_request(vec![
            empty_pattern(),
            empty_pattern(),
            empty_pattern(),
            empty_pattern(),
            empty_pattern(),
        ]))
        .await
        .unwrap();
    let appended: AppendResponse = json_body(resp.into_body()).await;
    assert_eq!(appended.queued, 2);
    assert_eq!(appended.dropped, 3);
    assert_eq!(appended.queue_len, MAX_QUEUE);
}
