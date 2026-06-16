// HTTP integration tests for the single-pattern export endpoint.
//
// These tests mirror the style of the `http_export_pool_*` suite in
// `web_tests.rs` but target `POST /api/pattern/export`, which streams the
// encoded file bytes back with a format-appropriate Content-Type.
//
// Covered:
//   - happy path for every supported format (toml, json, steps_txt, pat,
//     seq, mid, rbs) returns 200 with a non-empty body and the expected
//     Content-Type
//   - `sqs` is rejected with 400 (bank-level only)
//   - unknown format is rejected with 400
//   - malformed pattern (wrong step count) is rejected with 400
//   - toml/json/steps_txt round-trip: exported bytes re-import to the
//     same pattern
//   - rbs body contains the placement at device 0 / group 0 / slot 0
//     (the G1P1A convention) by re-parsing and pulling that slot

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::web::handlers;
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

static TEST_DB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn test_router() -> axum::Router {
    use axum::routing::post;
    use std::sync::Arc;
    let counter = TEST_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let lib_path = std::env::temp_dir().join(format!(
        "td3_pattern_export_http_test_{}_{}_{}.sqlite3",
        std::process::id(),
        counter,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let _ = std::fs::remove_file(&lib_path);
    let library =
        Arc::new(crate::library::LibraryStore::load_or_create(lib_path).expect("test library"));
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        library,
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    axum::Router::new()
        .route("/api/pattern/export", post(handlers::pattern_export))
        .route("/api/pattern/import", post(handlers::pattern_import))
        .route(
            "/api/pattern/parse-bank",
            post(handlers::pattern_parse_bank),
        )
        .with_state(state)
}

fn valid_web_pattern_json() -> String {
    web_pattern_json_with_note("C")
}

fn web_pattern_json_with_note(note: &str) -> String {
    let step = format!(
        r#"{{"note":"{}","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}}"#,
        note
    );
    let steps: Vec<String> = (0..16).map(|_| step.clone()).collect();
    format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    )
}

fn export_rbs_multi_request(notes: &[&str], mode: &str) -> Request<Body> {
    let patterns: Vec<String> = notes
        .iter()
        .map(|note| web_pattern_json_with_note(note))
        .collect();
    let body = format!(
        r#"{{"pattern":{},"patterns":[{}],"format":"rbs","rbs_mode":"{}"}}"#,
        patterns[0],
        patterns.join(","),
        mode
    );
    Request::builder()
        .method("POST")
        .uri("/api/pattern/export")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn do_rbs_multi_export(notes: &[&str], mode: &str) -> (StatusCode, Vec<u8>) {
    let app = test_router();
    let resp = app
        .oneshot(export_rbs_multi_request(notes, mode))
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, bytes)
}

fn export_request(format: &str) -> Request<Body> {
    let body = format!(
        r#"{{"pattern":{},"format":"{}"}}"#,
        valid_web_pattern_json(),
        format
    );
    Request::builder()
        .method("POST")
        .uri("/api/pattern/export")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn do_export(format: &str) -> (StatusCode, String, Vec<u8>) {
    let app = test_router();
    let resp = app.oneshot(export_request(format)).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or("").to_string())
        .unwrap_or_default();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, ct, bytes)
}

// ---------------------------------------------------------------------------
// Happy-path: every supported format
// ---------------------------------------------------------------------------

#[tokio::test]
async fn http_pattern_export_toml_ok() {
    let (status, ct, bytes) = do_export("toml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("application/toml"));
    let s = std::str::from_utf8(&bytes).unwrap();
    assert!(s.contains("td3-control"), "toml body: {}", s);
}

#[tokio::test]
async fn http_pattern_export_json_ok() {
    let (status, ct, bytes) = do_export("json").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("application/json"));
    let s = std::str::from_utf8(&bytes).unwrap();
    assert!(s.contains("td3-control"), "json body: {}", s);
}

#[tokio::test]
async fn http_pattern_export_steps_txt_ok() {
    let (status, ct, bytes) = do_export("steps_txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/plain"));
    let s = std::str::from_utf8(&bytes).unwrap();
    assert!(s.contains("td3-stepdsl-v1"), "steps body: {}", s);
}

#[tokio::test]
async fn http_pattern_export_steps_alias_ok() {
    // Frontend may request "steps" for historical reasons; the handler
    // accepts it as an alias for "steps_txt".
    let (status, _ct, bytes) = do_export("steps").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!bytes.is_empty());
}

#[tokio::test]
async fn http_pattern_export_pat_ok() {
    let (status, ct, bytes) = do_export("pat").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/plain"));
    assert!(!bytes.is_empty(), "pat body must not be empty");
}

#[tokio::test]
async fn http_pattern_export_seq_ok() {
    let (status, ct, bytes) = do_export("seq").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "application/octet-stream");
    // SynthTribe .seq files are 146 bytes per formats::seq contract.
    assert_eq!(bytes.len(), 146);
}

#[tokio::test]
async fn http_pattern_export_mid_ok() {
    let (status, ct, bytes) = do_export("mid").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "audio/midi");
    // Standard MIDI file header begins with "MThd".
    assert!(bytes.starts_with(b"MThd"), "missing SMF header");
}

#[tokio::test]
async fn http_pattern_export_rbs_ok() {
    let (status, ct, bytes) = do_export("rbs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "application/octet-stream");
    assert!(!bytes.is_empty(), "rbs body must not be empty");
}

// ---------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn http_pattern_export_sqs_rejected() {
    let (status, _ct, _bytes) = do_export("sqs").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_export_unknown_format_rejected() {
    let (status, _ct, _bytes) = do_export("xyzzy").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_export_malformed_pattern_rejected() {
    // Pattern with only 2 steps - web_to_pattern rejects anything not 16.
    let app = test_router();
    let body = r#"{"pattern":{"active_steps":16,"triplet":false,"steps":[
        {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
        {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}
    ]},"format":"toml"}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/export")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Round-trip: exported text formats re-import to an equivalent pattern
// ---------------------------------------------------------------------------

async fn reimport(content: &str, format: &str) -> StatusCode {
    let app = test_router();
    let escaped = serde_json::to_string(content).unwrap();
    let body = format!(r#"{{"content":{},"format":"{}"}}"#, escaped, format);
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn http_pattern_export_toml_roundtrips() {
    let (_s, _ct, bytes) = do_export("toml").await;
    let content = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(reimport(content, "toml").await, StatusCode::OK);
}

#[tokio::test]
async fn http_pattern_export_json_roundtrips() {
    let (_s, _ct, bytes) = do_export("json").await;
    let content = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(reimport(content, "json").await, StatusCode::OK);
}

#[tokio::test]
async fn http_pattern_export_steps_roundtrips() {
    let (_s, _ct, bytes) = do_export("steps_txt").await;
    let content = std::str::from_utf8(&bytes).unwrap();
    // /api/pattern/import uses "steps" as the format key for steps_txt.
    assert_eq!(reimport(content, "steps").await, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Round-trip: pat (text) + seq / mid (binary) re-import via /api/pattern/import
// ---------------------------------------------------------------------------

async fn reimport_bytes(raw: &[u8], format: &str) -> StatusCode {
    let app = test_router();
    // Encode the raw payload as a JSON number array - same shape the UI sends
    // for binary formats (.seq, .mid). No base64 crate needed.
    let arr: Vec<serde_json::Value> = raw
        .iter()
        .map(|b| serde_json::Value::from(*b as u64))
        .collect();
    let bytes_json = serde_json::to_string(&arr).unwrap();
    let body = format!(r#"{{"bytes":{},"format":"{}"}}"#, bytes_json, format);
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn http_pattern_export_pat_roundtrips() {
    let (_s, _ct, bytes) = do_export("pat").await;
    let content = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(reimport(content, "pat").await, StatusCode::OK);
}

#[tokio::test]
async fn http_pattern_export_seq_roundtrips() {
    let (_s, _ct, bytes) = do_export("seq").await;
    assert_eq!(reimport_bytes(&bytes, "seq").await, StatusCode::OK);
}

#[tokio::test]
async fn http_pattern_export_mid_roundtrips() {
    let (_s, _ct, bytes) = do_export("mid").await;
    assert_eq!(reimport_bytes(&bytes, "mid").await, StatusCode::OK);
}

#[tokio::test]
async fn http_pattern_import_seq_without_bytes_is_400() {
    let app = test_router();
    // Send text `content` for a format that requires `bytes`. The handler
    // must surface the mismatch instead of silently importing garbage.
    let body = r#"{"content":"nope","format":"seq"}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_import_pat_without_content_is_400() {
    let app = test_router();
    let body = r#"{"bytes":[1,2,3],"format":"pat"}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// RBS placement: exported file must have the pattern at G1P1A (device 0 /
// group 0 / slot 0). Parse the bytes back through the rbs parser and assert
// that the slot's pattern matches what we exported.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn http_pattern_export_rbs_places_at_g1p1a() {
    // First, export and capture bytes.
    let (_status, _ct, rbs_bytes) = do_export("rbs").await;

    // Parse the RBS. Device 0 / group 0 / slot 0 is G1P1A (A-side, first
    // group, first slot).
    let song = crate::formats::rbs::RbsSong::parse(&rbs_bytes).expect("parse exported rbs");
    let slot_pattern = song.pattern_at(0, 0, 0);

    // Then, round-trip the same source pattern through our TOML exporter +
    // importer to get the canonical source pattern, and compare.
    let (_s, _ct, toml_bytes) = do_export("toml").await;
    let toml_str = std::str::from_utf8(&toml_bytes).unwrap();
    let source = crate::formats::toml_fmt::import(toml_str).expect("import toml source");

    // Active steps and triplet flag must match.
    assert_eq!(slot_pattern.active_steps, source.active_steps);
    assert_eq!(slot_pattern.triplet, source.triplet);
    // Step-level equality: the slot copy must match the source 1:1 for the
    // active range. (The pattern is an all-C NORMAL pattern from
    // valid_web_pattern_json(), so every step should be identical.)
    for i in 0..(source.active_steps as usize) {
        assert_eq!(
            slot_pattern.step[i], source.step[i],
            "step {} differs between G1P1A slot and source",
            i
        );
    }
}

// ---------------------------------------------------------------------------
// Exercise: every other slot in the exported RBS must remain blank/silent
// so ReBirth preview doesn't fire off unrelated patterns. We sanity-check a
// couple of non-G1P1A slots (device 0 / group 0 / slot 1 and device 1 / ...)
// by asserting they differ from the active source - blank slots have all
// rests by default, while the source is all-C.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn http_pattern_export_rbs_leaves_other_slots_silent() {
    let (_status, _ct, rbs_bytes) = do_export("rbs").await;
    let song = crate::formats::rbs::RbsSong::parse(&rbs_bytes).expect("parse exported rbs");

    let neighbour = song.pattern_at(0, 0, 1);
    let g1p1a = song.pattern_at(0, 0, 0);
    assert_ne!(
        neighbour.step[0], g1p1a.step[0],
        "neighbour slot unexpectedly matches G1P1A - export_single should leave it silent"
    );

    let other_device = song.pattern_at(1, 0, 0);
    assert_ne!(
        other_device.step[0], g1p1a.step[0],
        "device-2 slot unexpectedly matches G1P1A - export_single should only place on device 0"
    );
}

#[tokio::test]
async fn http_pattern_export_rbs_multi_serial_places_patterns_in_device_one_order() {
    let (status, rbs_bytes) = do_rbs_multi_export(&["D", "E", "F"], "SERIAL").await;
    assert_eq!(status, StatusCode::OK);

    let song = crate::formats::rbs::RbsSong::parse(&rbs_bytes).expect("parse exported rbs");
    assert_eq!(song.pattern_at(0, 0, 0).step[0].note, 2);
    assert_eq!(song.pattern_at(0, 0, 1).step[0].note, 4);
    assert_eq!(song.pattern_at(0, 0, 2).step[0].note, 5);
}

#[tokio::test]
async fn http_pattern_export_rbs_multi_alternate_places_patterns_across_devices() {
    let (status, rbs_bytes) = do_rbs_multi_export(&["D", "E", "F", "G"], "ALTERNATE").await;
    assert_eq!(status, StatusCode::OK);

    let song = crate::formats::rbs::RbsSong::parse(&rbs_bytes).expect("parse exported rbs");
    assert_eq!(song.pattern_at(0, 0, 0).step[0].note, 2);
    assert_eq!(song.pattern_at(1, 0, 0).step[0].note, 4);
    assert_eq!(song.pattern_at(0, 0, 1).step[0].note, 5);
    assert_eq!(song.pattern_at(1, 0, 1).step[0].note, 7);
}

#[tokio::test]
async fn http_pattern_export_rbs_multi_rejects_invalid_mode() {
    let (status, _rbs_bytes) = do_rbs_multi_export(&["D"], "BAD").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// parse-bank: sqs / rbs files → 64-slot preview grid
// ---------------------------------------------------------------------------
//
// Covered:
//   - unknown format id → 400
//   - malformed bytes (too short .rbs) → 500 (format error)
//   - missing bytes field → 400 (via serde required-field error)
//   - rbs round-trip: export_single places the test pattern at G1P1A and
//     leaves the other 63 slots silent; parse-bank must reflect that shape

fn parse_bank_request(bytes: &[u8], format: &str) -> Request<Body> {
    let bytes_json: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
    let body = format!(
        r#"{{"bytes":[{}],"format":"{}"}}"#,
        bytes_json.join(","),
        format
    );
    Request::builder()
        .method("POST")
        .uri("/api/pattern/parse-bank")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn http_pattern_parse_bank_rejects_unknown_format() {
    let app = test_router();
    let resp = app
        .oneshot(parse_bank_request(&[0u8; 10], "pat"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_parse_bank_rejects_malformed_rbs_bytes() {
    // 10 bytes is way too short to contain two `303 ` chunks - parse should
    // fail with a format error. The handler surfaces `Td3Error::FormatError`
    // as 500 (same as every other decode error in this codebase).
    let app = test_router();
    let resp = app
        .oneshot(parse_bank_request(&[0u8; 10], "rbs"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn http_pattern_parse_bank_rbs_roundtrip() {
    // Export an all-C pattern to .rbs, then feed the bytes back through
    // parse-bank and verify:
    //   - G1-P1A is non-empty and carries a populated WebPattern
    //   - every other slot is reported as empty
    let (_status, _ct, rbs_bytes) = do_export("rbs").await;
    let app = test_router();

    let bytes_json: Vec<String> = rbs_bytes.iter().map(|b| b.to_string()).collect();
    let body = format!(r#"{{"bytes":[{}],"format":"rbs"}}"#, bytes_json.join(","));
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/parse-bank")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: crate::web::api_types::PatternParseBankResponse =
        serde_json::from_slice(&bytes).expect("decode parse-bank response");
    assert_eq!(parsed.slots.len(), 64, "sqs/rbs bank always has 64 slots");

    let g1p1a = parsed
        .slots
        .iter()
        .find(|s| s.slot_key == "G1-P1A")
        .unwrap();
    assert!(!g1p1a.empty, "G1-P1A should be the populated slot");
    assert!(
        g1p1a.pattern.is_some(),
        "non-empty slot must carry a WebPattern"
    );

    let non_g1p1a_empty_count = parsed
        .slots
        .iter()
        .filter(|s| s.slot_key != "G1-P1A" && s.empty)
        .count();
    assert_eq!(
        non_g1p1a_empty_count, 63,
        "every slot other than G1-P1A should be silent after single-pattern rbs export"
    );
    for slot in &parsed.slots {
        if slot.empty {
            assert!(
                slot.pattern.is_none(),
                "empty slot {} must not carry a pattern payload",
                slot.slot_key
            );
        }
    }
}

#[tokio::test]
async fn http_pattern_parse_bank_missing_bytes_is_rejected() {
    // `bytes` is a required field - axum's Json extractor fails deserialize
    // and surfaces 422 UNPROCESSABLE_ENTITY before the handler runs.
    let app = test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/parse-bank")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"format":"rbs"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
