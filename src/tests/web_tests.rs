//! Tests for the web Control UI backend.
//!
//! Covers:
//! - Input validation (patgroup, pattern, side)
//! - Pattern domain↔web JSON conversion (round-trip)
//! - WebPattern/WebStep serde correctness
//! - Clock tick interval calculation
//! - HTTP integration tests for stateless endpoints
//! - AppState behavior (status, disconnect)

use crate::pattern::Pattern;
use crate::step;
use crate::web::api_types::*;
use crate::web::clock;
use crate::web::handlers::{pattern_to_web, validate_group, validate_pattern, web_to_pattern};
use crate::web::state::{AppState, ClockState, ScratchSlot, UiConfigSnapshot};

// =========================================================================
// Validation: validate_group
// =========================================================================

#[test]
fn validate_group_accepts_1_through_4() {
    for g in 1..=4 {
        let result = validate_group(g);
        assert!(result.is_ok(), "patgroup {} should be valid", g);
        assert_eq!(
            result.unwrap(),
            g - 1,
            "patgroup {} should return 0-indexed {}",
            g,
            g - 1
        );
    }
}

#[test]
fn validate_group_rejects_zero() {
    assert!(validate_group(0).is_err());
}

#[test]
fn validate_group_rejects_five() {
    assert!(validate_group(5).is_err());
}

#[test]
fn validate_group_rejects_255() {
    assert!(validate_group(255).is_err());
}

// =========================================================================
// Validation: validate_pattern
// =========================================================================

#[test]
fn validate_pattern_accepts_1_through_8_a() {
    for p in 1..=8 {
        let result = validate_pattern(p, "A");
        assert!(result.is_ok(), "pattern {} A should be valid", p);
        let (slot, side) = result.unwrap();
        assert_eq!(slot, p - 1);
        assert_eq!(side, 0);
    }
}

#[test]
fn validate_pattern_accepts_side_b() {
    let (slot, side) = validate_pattern(1, "B").unwrap();
    assert_eq!(slot, 0);
    assert_eq!(side, 1);
}

#[test]
fn validate_pattern_case_insensitive_side() {
    assert_eq!(validate_pattern(1, "a").unwrap(), (0, 0));
    assert_eq!(validate_pattern(1, "b").unwrap(), (0, 1));
}

#[test]
fn validate_pattern_rejects_zero() {
    assert!(validate_pattern(0, "A").is_err());
}

#[test]
fn validate_pattern_rejects_nine() {
    assert!(validate_pattern(9, "A").is_err());
}

#[test]
fn validate_pattern_rejects_invalid_side() {
    assert!(validate_pattern(1, "C").is_err());
    assert!(validate_pattern(1, "").is_err());
    assert!(validate_pattern(1, "AB").is_err());
}

// =========================================================================
// Pattern conversion: pattern_to_web
// =========================================================================

fn test_pattern() -> Pattern {
    let mut steps: [step::Step; 16] = Default::default();
    steps[0] = step::Step {
        note: 0,
        transpose: step::Transpose::Normal,
        accent: step::Accent::On,
        slide: step::Slide::Off,
        time: step::Time::Normal,
    };
    steps[1] = step::Step {
        note: 4,
        transpose: step::Transpose::Up,
        accent: step::Accent::Off,
        slide: step::Slide::On,
        time: step::Time::Normal,
    };
    steps[2] = step::Step {
        note: 7,
        transpose: step::Transpose::Down,
        accent: step::Accent::Off,
        slide: step::Slide::Off,
        time: step::Time::Rest,
    };
    steps[3] = step::Step {
        note: 0,
        transpose: step::Transpose::Normal,
        accent: step::Accent::Off,
        slide: step::Slide::Off,
        time: step::Time::Tie,
    };
    steps[4] = step::Step {
        note: 12,
        transpose: step::Transpose::Normal,
        accent: step::Accent::On,
        slide: step::Slide::On,
        time: step::Time::Normal,
    };
    Pattern::new(false, 16, steps).unwrap()
}

#[test]
fn pattern_to_web_preserves_active_steps() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    assert_eq!(web.active_steps, 16);
}

#[test]
fn pattern_to_web_preserves_triplet() {
    let p = Pattern::new(true, 8, Default::default()).unwrap();
    let web = pattern_to_web(&p);
    assert!(web.triplet);
}

#[test]
fn pattern_to_web_has_16_steps() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    assert_eq!(web.steps.len(), 16);
}

#[test]
fn pattern_to_web_note_names_correct() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    assert_eq!(web.steps[0].note.as_str(), "C");
    assert_eq!(web.steps[1].note.as_str(), "E");
    assert_eq!(web.steps[2].note.as_str(), "G");
    assert_eq!(web.steps[4].note.as_str(), "C^");
}

#[test]
fn pattern_to_web_transpose_strings() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    assert_eq!(web.steps[0].transpose.as_str(), "NORMAL");
    assert_eq!(web.steps[1].transpose.as_str(), "UP");
    assert_eq!(web.steps[2].transpose.as_str(), "DOWN");
}

#[test]
fn pattern_to_web_accent_slide_bools() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    assert!(web.steps[0].accent);
    assert!(!web.steps[0].slide);
    assert!(!web.steps[1].accent);
    assert!(web.steps[1].slide);
    assert!(web.steps[4].accent);
    assert!(web.steps[4].slide);
}

#[test]
fn pattern_to_web_time_strings() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    assert_eq!(web.steps[0].time.as_str(), "NORMAL");
    assert_eq!(web.steps[2].time.as_str(), "REST");
    assert_eq!(web.steps[3].time.as_str(), "TIE");
}

// =========================================================================
// Pattern conversion: web_to_pattern
// =========================================================================

#[test]
fn web_to_pattern_valid_roundtrip() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    let p2 = web_to_pattern(&web).unwrap();

    // Compare field by field through web representation
    let web2 = pattern_to_web(&p2);
    assert_eq!(web.active_steps, web2.active_steps);
    assert_eq!(web.triplet, web2.triplet);
    for i in 0..16 {
        assert_eq!(
            web.steps[i].note, web2.steps[i].note,
            "step {} note mismatch",
            i
        );
        assert_eq!(
            web.steps[i].transpose, web2.steps[i].transpose,
            "step {} transpose mismatch",
            i
        );
        assert_eq!(
            web.steps[i].accent, web2.steps[i].accent,
            "step {} accent mismatch",
            i
        );
        assert_eq!(
            web.steps[i].slide, web2.steps[i].slide,
            "step {} slide mismatch",
            i
        );
        assert_eq!(
            web.steps[i].time, web2.steps[i].time,
            "step {} time mismatch",
            i
        );
    }
}

#[test]
fn web_pattern_json_rejects_wrong_step_count() {
    let step = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let steps: Vec<&str> = (0..15).map(|_| step).collect();
    let json = format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    );
    assert!(serde_json::from_str::<WebPattern>(&json).is_err());
}

#[test]
fn web_pattern_json_rejects_zero_steps() {
    let json = r#"{"active_steps":16,"triplet":false,"steps":[]}"#;
    assert!(serde_json::from_str::<WebPattern>(json).is_err());
}

#[test]
fn web_pattern_json_rejects_invalid_note() {
    let bad = r#"{"note":"Z#","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let good = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let mut steps = vec![bad.to_string()];
    steps.extend((1..16).map(|_| good.to_string()));
    let json = format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    );
    assert!(serde_json::from_str::<WebPattern>(&json).is_err());
}

#[test]
fn web_pattern_json_rejects_invalid_transpose() {
    let bad = r#"{"note":"C","transpose":"SIDEWAYS","accent":false,"slide":false,"time":"NORMAL"}"#;
    let good = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let mut steps = vec![bad.to_string()];
    steps.extend((1..16).map(|_| good.to_string()));
    let json = format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    );
    assert!(serde_json::from_str::<WebPattern>(&json).is_err());
}

#[test]
fn web_pattern_json_rejects_invalid_time() {
    let bad = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"INVALID"}"#;
    let good = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let mut steps = vec![bad.to_string()];
    steps.extend((1..16).map(|_| good.to_string()));
    let json = format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    );
    assert!(serde_json::from_str::<WebPattern>(&json).is_err());
}

#[test]
fn web_to_pattern_rejects_active_steps_zero() {
    let web = WebPattern {
        active_steps: 0,
        triplet: false,
        steps: [WebStep::default(); 16],
    };
    assert!(web_to_pattern(&web).is_err());
}

#[test]
fn web_to_pattern_rejects_active_steps_17() {
    let web = WebPattern {
        active_steps: 17,
        triplet: false,
        steps: [WebStep::default(); 16],
    };
    assert!(web_to_pattern(&web).is_err());
}

// =========================================================================
// Pattern conversion: all notes round-trip
// =========================================================================

#[test]
fn all_13_notes_survive_web_roundtrip() {
    let note_names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C^",
    ];
    for (i, expected_name) in note_names.iter().enumerate() {
        let mut steps: [step::Step; 16] = Default::default();
        steps[0].note = i as u8;
        let p = Pattern::new(false, 1, steps).unwrap();
        let web = pattern_to_web(&p);
        assert_eq!(
            web.steps[0].note.as_str(),
            *expected_name,
            "note index {} should be {}",
            i,
            expected_name
        );
        let p2 = web_to_pattern(&web).unwrap();
        let web2 = pattern_to_web(&p2);
        assert_eq!(
            web2.steps[0].note.as_str(),
            *expected_name,
            "roundtrip failed for note {}",
            expected_name
        );
    }
}

#[test]
fn all_time_states_survive_web_roundtrip() {
    let times = [
        (step::Time::Normal, "NORMAL"),
        (step::Time::Tie, "TIE"),
        (step::Time::Rest, "REST"),
        (step::Time::TieRest, "TIE_REST"),
    ];
    for (time_val, time_str) in &times {
        let mut steps: [step::Step; 16] = Default::default();
        steps[0].time = *time_val;
        let p = Pattern::new(false, 1, steps).unwrap();
        let web = pattern_to_web(&p);
        assert_eq!(
            web.steps[0].time.as_str(),
            *time_str,
            "time {:?} should serialize as {}",
            time_val,
            time_str
        );
        let p2 = web_to_pattern(&web).unwrap();
        let web2 = pattern_to_web(&p2);
        assert_eq!(
            web2.steps[0].time.as_str(),
            *time_str,
            "roundtrip failed for time {}",
            time_str
        );
    }
}

// =========================================================================
// WebPattern JSON serde
// =========================================================================

#[test]
fn web_pattern_serialization_snapshot_matches_shape() {
    let web = WebPattern {
        active_steps: 8,
        triplet: true,
        steps: [WebStep {
            note: WebNote::CSharp,
            transpose: WebTranspose::Up,
            accent: true,
            slide: false,
            time: WebTime::Rest,
        }; 16],
    };
    let value = serde_json::to_value(&web).unwrap();
    assert_eq!(value["active_steps"], serde_json::json!(8));
    assert_eq!(value["triplet"], serde_json::json!(true));
    assert_eq!(value["steps"].as_array().unwrap().len(), 16);
    assert_eq!(
        value["steps"][0],
        serde_json::json!({
            "note": "C#",
            "transpose": "UP",
            "accent": true,
            "slide": false,
            "time": "REST"
        })
    );
}

#[test]
fn web_pattern_serialization_always_writes_16_steps() {
    let web = pattern_to_web(&test_pattern());
    let value = serde_json::to_value(&web).unwrap();
    assert_eq!(value["steps"].as_array().unwrap().len(), 16);
}

#[test]
fn web_pattern_deserializes_from_json() {
    let json = r#"{
        "active_steps": 4,
        "triplet": false,
        "steps": [
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"D","transpose":"UP","accent":true,"slide":true,"time":"TIE"},
            {"note":"E","transpose":"DOWN","accent":false,"slide":false,"time":"REST"},
            {"note":"F","transpose":"NORMAL","accent":false,"slide":false,"time":"TIE_REST"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}
        ]
    }"#;
    let web: WebPattern = serde_json::from_str(json).unwrap();
    assert_eq!(web.active_steps, 4);
    assert!(!web.triplet);
    assert_eq!(web.steps.len(), 16);
    assert_eq!(web.steps[1].note.as_str(), "D");
    assert_eq!(web.steps[1].transpose.as_str(), "UP");
    assert!(web.steps[1].accent);
    assert!(web.steps[1].slide);
    assert_eq!(web.steps[1].time.as_str(), "TIE");
}

#[test]
fn web_pattern_json_roundtrip() {
    let p = test_pattern();
    let web = pattern_to_web(&p);
    let json = serde_json::to_string(&web).unwrap();
    let web2: WebPattern = serde_json::from_str(&json).unwrap();
    assert_eq!(web.active_steps, web2.active_steps);
    assert_eq!(web.triplet, web2.triplet);
    for i in 0..16 {
        assert_eq!(web.steps[i].note, web2.steps[i].note);
        assert_eq!(web.steps[i].transpose, web2.steps[i].transpose);
        assert_eq!(web.steps[i].accent, web2.steps[i].accent);
        assert_eq!(web.steps[i].slide, web2.steps[i].slide);
        assert_eq!(web.steps[i].time, web2.steps[i].time);
    }
}

// =========================================================================
// Note preview request helper
// =========================================================================

#[test]
fn note_preview_request_midi_note_mapping() {
    let req = NotePreviewRequest {
        note: "C#".into(),
        transpose: "UP".into(),
        accent: false,
    };
    assert_eq!(req.midi_note().unwrap(), 49);
}

#[test]
fn note_preview_request_accepts_case_insensitive_note() {
    let req = NotePreviewRequest {
        note: "c#".into(),
        transpose: "normal".into(),
        accent: true,
    };
    assert_eq!(req.midi_note().unwrap(), 37);
}

#[test]
fn note_preview_request_rejects_unknown_transpose() {
    let req = NotePreviewRequest {
        note: "C".into(),
        transpose: "SIDEWAYS".into(),
        accent: false,
    };
    assert!(req.midi_note().is_err());
}

// =========================================================================
// Clock tick interval
// =========================================================================

#[test]
fn tick_interval_120_bpm() {
    // 120.00 BPM (centibpm 12000), 24 ppqn → 250_000_000 / 12000 = 20833 µs
    let interval = clock::tick_interval(12_000);
    assert_eq!(interval.as_micros(), 20833);
}

#[test]
fn tick_interval_60_bpm() {
    // 60.00 BPM (centibpm 6000) → 250_000_000 / 6000 = 41666 µs
    let interval = clock::tick_interval(6_000);
    assert_eq!(interval.as_micros(), 41666);
}

#[test]
fn tick_interval_240_bpm() {
    // 240.00 BPM (centibpm 24000) → 250_000_000 / 24000 = 10416 µs
    let interval = clock::tick_interval(24_000);
    assert_eq!(interval.as_micros(), 10416);
}

#[test]
fn tick_interval_1_bpm() {
    // 1.00 BPM (centibpm 100) → 250_000_000 / 100 = 2_500_000 µs = 2.5s
    let interval = clock::tick_interval(100);
    assert_eq!(interval.as_micros(), 2_500_000);
}

#[test]
fn tick_interval_300_bpm() {
    // 300.00 BPM (centibpm 30000) → 250_000_000 / 30000 = 8333 µs
    let interval = clock::tick_interval(30_000);
    assert_eq!(interval.as_micros(), 8333);
}

#[test]
fn tick_interval_fractional_140_01_bpm_shifts_by_a_microsecond() {
    // 0.01 BPM resolution survives the 250_000_000 / centibpm integer
    // division: at 14000 → 17857 µs, at 14001 → 17855 µs (the exact
    // fractional period drops by ~0.13 µs and integer truncation lands
    // it on the next lower microsecond boundary). The key invariant is
    // that the centi-BPM input produces a distinct, smaller period.
    let coarse = clock::tick_interval(14_000).as_micros();
    let fine = clock::tick_interval(14_001).as_micros();
    assert_eq!(coarse, 17_857);
    assert_eq!(fine, 17_855);
    assert!(fine < coarse);
}

#[test]
fn tick_interval_zero_centibpm_treats_as_minimum() {
    // Defensive: a 0 centi-BPM input must not divide-by-zero. The
    // runner clamps to 1 internally.
    let interval = clock::tick_interval(0);
    assert_eq!(interval.as_micros(), 250_000_000);
}

#[test]
fn pattern_wrap_duration_85_bpm_normal_16_steps() {
    // 85.00 BPM (centibpm 8500) - same value as the legacy integer test.
    let duration = clock::pattern_wrap_duration(8_500, 16, false);
    assert_eq!(duration.as_micros(), 2_823_456);
}

#[test]
fn pattern_wrap_duration_120_bpm_triplet_16_steps() {
    // 120.00 BPM (centibpm 12000) - same value as the legacy integer test.
    let duration = clock::pattern_wrap_duration(12_000, 16, true);
    assert_eq!(duration.as_micros(), 2_666_624);
}

// =========================================================================
// AppState: status endpoint (async)
// =========================================================================

#[tokio::test]
async fn status_disconnected_by_default() {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        make_test_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let session = state.midi.session.lock().await;
    assert!(session.is_none());
    let clock = state.playback.clock.lock().await;
    assert!(clock.is_none());
}

#[tokio::test]
async fn disconnect_when_not_connected_returns_false() {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        make_test_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );

    // Call the handler directly
    let response = crate::web::handlers::disconnect(axum::extract::State(state)).await;
    assert!(!response.0.disconnected);
}

// =========================================================================
// HTTP integration: router-level tests
// =========================================================================

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn build_test_router() -> axum::Router {
    use crate::web::handlers;
    use axum::routing::{get, post};

    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        make_test_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    axum::Router::new()
        .route("/api/status", get(handlers::status))
        .route("/api/ports", get(handlers::ports))
        .route("/api/midi/disconnect", post(handlers::disconnect))
        .route("/api/pattern/load", post(handlers::pattern_load))
        .route("/api/pattern/save", post(handlers::pattern_save))
        .route("/api/pattern/import", post(handlers::pattern_import))
        .route(
            "/api/pattern/parse-bank",
            post(handlers::pattern_parse_bank),
        )
        .route(
            "/api/pattern/play-preview",
            post(handlers::pattern_play_preview),
        )
        .route("/api/pattern/export-pool", post(handlers::export_pool))
        .route("/api/pattern/export", post(handlers::pattern_export))
        .route("/api/transport/start", post(handlers::transport_start))
        .route("/api/transport/stop", post(handlers::transport_stop))
        .route("/api/transport/bpm", post(handlers::transport_bpm))
        .route(
            "/api/transport/wrap-pulse",
            post(handlers::transport_wrap_pulse),
        )
        .route(
            "/api/config/progression",
            get(handlers::get_progression_config),
        )
        .route(
            "/api/config/progression",
            post(handlers::save_progression_config),
        )
        .route("/api/config/env", get(handlers::get_env_config))
        .route(
            "/api/progression/export-package",
            post(handlers::export_progression_package),
        )
        .with_state(state)
}

/// Build a throwaway library backed by a unique temp file so web tests never
/// contend with each other or with the real `ui/config/bank-library.json`.
fn make_test_library() -> std::sync::Arc<crate::library::LibraryStore> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("td3-web-tests-library-{}-{}.json", pid, n));
    let _ = std::fs::remove_file(&path);
    std::sync::Arc::new(
        crate::library::LibraryStore::load_or_create(path).expect("test library creation"),
    )
}

/// Helper: build a valid 16-step WebPattern JSON for save requests.
fn valid_web_pattern_json() -> String {
    let step = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let steps: Vec<&str> = (0..16).map(|_| step).collect();
    format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    )
}

#[tokio::test]
async fn http_status_returns_disconnected() {
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let status: StatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(!status.connected);
    assert!(status.product_name.is_none());
    assert!(status.firmware.is_none());
    assert!(!status.playing);
    assert_eq!(status.bpm, 120);
    assert_eq!(status.centibpm, 12_000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_ports_returns_ok() {
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/ports")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let ports: PortsResponse = serde_json::from_slice(&body).unwrap();
    // Just verify it's valid JSON with the expected shape
    // Verify the response deserialized with the expected shape (port lists may be empty)
    let _ = ports.inputs.len();
    let _ = ports.outputs.len();
}

#[tokio::test]
async fn http_disconnect_when_not_connected() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/midi/disconnect")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: DisconnectResponse = serde_json::from_slice(&body).unwrap();
    assert!(!result.disconnected);
}

#[tokio::test]
async fn http_transport_start_rejects_bpm_zero() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/start")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"bpm":0}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_transport_start_rejects_bpm_over_300() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/start")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"bpm":301}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_transport_start_requires_connection() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/start")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"bpm":120}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // Should fail because no MIDI session
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_transport_wrap_pulse_rejects_invalid_active_steps() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/wrap-pulse")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"transportId":1,"anchorEpochMs":1,"wrapIndex":0,"activeSteps":0,"triplet":false}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn transport_wrap_pulse_returns_inactive_when_stopped_after_acceptance() {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        make_test_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let started_at_epoch_ms = crate::web::handlers::current_epoch_millis_for_clock();
    let mut clock_guard = state.playback.clock.lock().await;
    *clock_guard = Some(ClockState {
        centibpm: 30_000,
        started_at_epoch_ms,
        transport_id: 7,
        playing: true,
        runner: None,
    });

    let req = TransportWrapPulseRequest {
        transport_id: 7,
        anchor_epoch_ms: started_at_epoch_ms,
        wrap_index: 3,
        active_steps: 1,
        triplet: false,
    };
    let state_for_pulse = state.clone();
    let pulse_task = tokio::spawn(async move {
        crate::web::handlers::transport_wrap_pulse(
            axum::extract::State(state_for_pulse),
            axum::Json(req),
        )
        .await
    });

    tokio::task::yield_now().await;
    drop(clock_guard);
    crate::web::handlers::stop_clock(&state).await;

    let axum::Json(pulse) = pulse_task.await.unwrap().unwrap();
    assert!(!pulse.ok);
    assert_eq!(pulse.transport_id, 7);
    assert_eq!(pulse.wrap_index, 3);
}

#[tokio::test]
async fn http_transport_stop_idempotent_without_clock() {
    // Transport/stop is idempotent: the dedicated-thread clock owns
    // its own MIDI connection and sends MIDI Stop (0xFC) on thread
    // exit, so calling stop when no clock is running is a harmless
    // no-op. Previously this path coupled a `send_stop` to the main
    // session and failed with 400 when disconnected - that coupling
    // was accidental, and is now gone.
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/stop")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_transport_bpm_update_ok_even_without_clock() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/bpm")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"bpm":140}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // BPM update is a no-op when no clock is running - still returns OK
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_transport_bpm_rejects_zero() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/bpm")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"bpm":0}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_transport_bpm_accepts_centibpm_payload() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/bpm")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"centibpm":14037}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_transport_bpm_rejects_missing_fields() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/bpm")
        .header("content-type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_transport_bpm_rejects_centibpm_over_30000() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/transport/bpm")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"centibpm":30001}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// =========================================================================
// HTTP integration: pattern save validation (no MIDI session)
// =========================================================================

#[tokio::test]
async fn http_pattern_save_requires_connection() {
    let app = build_test_router();
    let body = format!(
        r#"{{"patgroup":1,"pattern":1,"side":"A","data":{}}}"#,
        valid_web_pattern_json()
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let err: ErrorBody = serde_json::from_slice(&body).unwrap();
    assert!(
        err.error.contains("not connected"),
        "error should mention connection: {}",
        err.error
    );
}

#[tokio::test]
async fn http_pattern_save_rejects_invalid_group() {
    let app = build_test_router();
    let body = format!(
        r#"{{"patgroup":5,"pattern":1,"side":"A","data":{}}}"#,
        valid_web_pattern_json()
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_save_rejects_invalid_pattern_number() {
    let app = build_test_router();
    let body = format!(
        r#"{{"patgroup":1,"pattern":9,"side":"A","data":{}}}"#,
        valid_web_pattern_json()
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_save_rejects_invalid_side() {
    let app = build_test_router();
    let body = format!(
        r#"{{"patgroup":1,"pattern":1,"side":"C","data":{}}}"#,
        valid_web_pattern_json()
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_save_rejects_wrong_step_count() {
    let app = build_test_router();
    // Only 2 steps instead of 16
    let body = r#"{"patgroup":1,"pattern":1,"side":"A","data":{
        "active_steps":16,"triplet":false,"steps":[
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
            {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}
        ]}}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_save_rejects_invalid_note_in_data() {
    let app = build_test_router();
    // First step has invalid note "Z"
    let mut steps = Vec::new();
    steps.push(
        r#"{"note":"Z","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#
            .to_string(),
    );
    for _ in 1..16 {
        steps.push(
            r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#
                .to_string(),
        );
    }
    let body = format!(
        r#"{{"patgroup":1,"pattern":1,"side":"A","data":{{"active_steps":16,"triplet":false,"steps":[{}]}}}}"#,
        steps.join(",")
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // Should fail on conversion (invalid note) - either 400 or 500
    assert_ne!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_pattern_save_rejects_malformed_json() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/save")
        .header("content-type", "application/json")
        .body(Body::from("not json"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK);
}

// =========================================================================
// HTTP integration: pattern load validation (no MIDI session)
// =========================================================================

#[tokio::test]
async fn http_pattern_load_requires_connection() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/load")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"patgroup":1,"pattern":1,"side":"A"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_load_rejects_group_zero() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/load")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"patgroup":0,"pattern":1,"side":"A"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_load_rejects_pattern_nine() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/load")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"patgroup":1,"pattern":9,"side":"A"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_load_rejects_bad_side() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/load")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"patgroup":1,"pattern":1,"side":"X"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// =========================================================================
// HTTP integration: pattern save accepts valid patgroup/pattern/side combos
// (still fails with "not connected" but validates address first)
// =========================================================================

#[tokio::test]
async fn http_pattern_save_all_groups_valid_address() {
    for patgroup in 1..=4u8 {
        let app = build_test_router();
        let body = format!(
            r#"{{"patgroup":{},"pattern":1,"side":"A","data":{}}}"#,
            patgroup,
            valid_web_pattern_json()
        );
        let req = Request::builder()
            .method("POST")
            .uri("/api/pattern/save")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should reach "not connected" (400), not "invalid patgroup"
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let err: ErrorBody = serde_json::from_slice(&body).unwrap();
        assert!(
            err.error.contains("not connected"),
            "patgroup {} should pass validation, got: {}",
            patgroup,
            err.error
        );
    }
}

#[tokio::test]
async fn http_pattern_save_both_sides_valid() {
    for side in &["A", "B"] {
        let app = build_test_router();
        let body = format!(
            r#"{{"patgroup":1,"pattern":1,"side":"{}","data":{}}}"#,
            side,
            valid_web_pattern_json()
        );
        let req = Request::builder()
            .method("POST")
            .uri("/api/pattern/save")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let err: ErrorBody = serde_json::from_slice(&body).unwrap();
        assert!(
            err.error.contains("not connected"),
            "side {} should pass validation, got: {}",
            side,
            err.error
        );
    }
}

#[tokio::test]
async fn http_pattern_save_all_pattern_numbers_valid() {
    for slot in 1..=8u8 {
        let app = build_test_router();
        let body = format!(
            r#"{{"patgroup":1,"pattern":{},"side":"A","data":{}}}"#,
            slot,
            valid_web_pattern_json()
        );
        let req = Request::builder()
            .method("POST")
            .uri("/api/pattern/save")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let err: ErrorBody = serde_json::from_slice(&body).unwrap();
        assert!(
            err.error.contains("not connected"),
            "pattern {} should pass validation, got: {}",
            slot,
            err.error
        );
    }
}

// =========================================================================
// HTTP integration: pattern import (file parse, no MIDI needed)
// =========================================================================

/// Generate a valid TOML pattern file string.
fn sample_toml_pattern() -> String {
    let mut s = String::from(
        "format = \"td3-control\"\nformat_version = 1\ndevice = \"TD-3\"\nactive_steps = 16\ntriplet_time = false\n\n",
    );
    for i in 1..=16 {
        s.push_str(&format!(
            "[[steps]]\nindex = {}\nnote = \"C\"\ntranspose = \"NORMAL\"\naccent = false\nslide = false\ntime = \"NORMAL\"\n\n",
            i
        ));
    }
    s
}

/// Generate a valid JSON pattern file string (full format, not WebPattern).
fn sample_json_pattern() -> String {
    let mut steps = Vec::new();
    for i in 1..=16 {
        steps.push(format!(
            r#"{{"index":{},"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}}"#,
            i
        ));
    }
    format!(
        r#"{{"format":"td3-control","format_version":1,"device":"TD-3","active_steps":16,"triplet_time":false,"steps":[{}]}}"#,
        steps.join(",")
    )
}

/// Generate a valid steps.txt pattern string.
fn sample_steps_pattern() -> String {
    let mut s = String::from("format=td3-stepdsl-v1\nactive_steps=16\ntriplet_time=off\n\n");
    for i in 1..=16 {
        s.push_str(&format!("{:02}  C:---:N\n", i));
    }
    s
}

#[tokio::test]
async fn http_import_toml_returns_valid_pattern() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": sample_toml_pattern(),
        "format": "toml"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: PatternImportResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.pattern.active_steps, 16);
    assert!(!result.pattern.triplet);
    assert_eq!(result.pattern.steps.len(), 16);
    assert_eq!(result.pattern.steps[0].note.as_str(), "C");
}

#[tokio::test]
async fn http_import_json_returns_valid_pattern() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": sample_json_pattern(),
        "format": "json"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: PatternImportResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.pattern.active_steps, 16);
    assert_eq!(result.pattern.steps.len(), 16);
}

#[tokio::test]
async fn http_import_steps_returns_valid_pattern() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": sample_steps_pattern(),
        "format": "steps"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: PatternImportResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.pattern.active_steps, 16);
    assert_eq!(result.pattern.steps.len(), 16);
}

#[tokio::test]
async fn http_import_toml_preserves_note_and_flags() {
    // Build a TOML with specific values on step 1
    let mut toml = String::from(
        "format = \"td3-control\"\nformat_version = 1\ndevice = \"TD-3\"\nactive_steps = 8\ntriplet_time = true\n\n",
    );
    toml.push_str(
        "[[steps]]\nindex = 1\nnote = \"A#\"\ntranspose = \"UP\"\naccent = true\nslide = true\ntime = \"REST\"\n\n",
    );
    for i in 2..=16 {
        toml.push_str(&format!(
            "[[steps]]\nindex = {}\nnote = \"C\"\ntranspose = \"NORMAL\"\naccent = false\nslide = false\ntime = \"NORMAL\"\n\n",
            i
        ));
    }
    let app = build_test_router();
    let body = serde_json::json!({ "content": toml, "format": "toml" }).to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: PatternImportResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.pattern.active_steps, 8);
    assert!(result.pattern.triplet);
    let s0 = &result.pattern.steps[0];
    assert_eq!(s0.note.as_str(), "A#");
    assert_eq!(s0.transpose.as_str(), "UP");
    assert!(s0.accent);
    assert!(s0.slide);
    assert_eq!(s0.time.as_str(), "REST");
}

#[tokio::test]
async fn http_import_steps_preserves_flags() {
    let steps_content = "\
format=td3-stepdsl-v1\n\
active_steps=4\n\
triplet_time=on\n\
\n\
01 A#:UAS:R\n\
02  C:---:N\n\
03  E:D--:T\n\
04  G:--S:TR\n\
05  C:---:N\n\
06  C:---:N\n\
07  C:---:N\n\
08  C:---:N\n\
09  C:---:N\n\
10  C:---:N\n\
11  C:---:N\n\
12  C:---:N\n\
13  C:---:N\n\
14  C:---:N\n\
15  C:---:N\n\
16  C:---:N\n";
    let app = build_test_router();
    let body = serde_json::json!({ "content": steps_content, "format": "steps" }).to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: PatternImportResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.pattern.active_steps, 4);
    assert!(result.pattern.triplet);
    let s0 = &result.pattern.steps[0];
    assert_eq!(s0.note.as_str(), "A#");
    assert_eq!(s0.transpose.as_str(), "UP");
    assert!(s0.accent);
    assert!(s0.slide);
    assert_eq!(s0.time.as_str(), "REST");
    let s2 = &result.pattern.steps[2];
    assert_eq!(s2.note.as_str(), "E");
    assert_eq!(s2.transpose.as_str(), "DOWN");
    assert_eq!(s2.time.as_str(), "TIE");
    let s3 = &result.pattern.steps[3];
    assert_eq!(s3.note.as_str(), "G");
    assert!(s3.slide);
    assert_eq!(s3.time.as_str(), "TIE_REST");
}

#[tokio::test]
async fn http_import_rejects_unsupported_format() {
    let app = build_test_router();
    // `syx` is a real codec but it isn't wired into the single-pattern
    // import endpoint (bank-level upload only), so it still triggers the
    // "unsupported format" branch now that mid/seq/pat are first-class.
    let body = serde_json::json!({
        "content": "whatever",
        "format": "syx"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let err: ErrorBody = serde_json::from_slice(&body).unwrap();
    assert!(err.error.contains("unsupported format"));
}

#[tokio::test]
async fn http_import_rejects_malformed_toml() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": "this is not valid toml [[[",
        "format": "toml"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_import_rejects_malformed_json_pattern() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": "{not valid json",
        "format": "json"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_import_rejects_malformed_steps() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": "format=td3-stepdsl-v1\n01 garbage line\n",
        "format": "steps"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_import_format_is_case_insensitive() {
    let app = build_test_router();
    let body = serde_json::json!({
        "content": sample_toml_pattern(),
        "format": "TOML"
    })
    .to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// =========================================================================
// HTTP integration: export-pool (batch pattern → text formats)
// =========================================================================

#[tokio::test]
async fn http_export_pool_single_pattern() {
    let app = build_test_router();
    let body = format!(r#"{{"patterns":[{}]}}"#, valid_web_pattern_json());
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/export-pool")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: ExportPoolResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].name, "pattern_001");
    assert!(result.files[0].toml.contains("td3-control"));
    assert!(result.files[0].json.contains("td3-control"));
    assert!(result.files[0].steps.contains("td3-stepdsl-v1"));
}

#[tokio::test]
async fn http_export_pool_multiple_patterns() {
    let app = build_test_router();
    let pat = valid_web_pattern_json();
    let body = format!(r#"{{"patterns":[{},{},{}]}}"#, pat, pat, pat);
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/export-pool")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: ExportPoolResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.files.len(), 3);
    assert_eq!(result.files[0].name, "pattern_001");
    assert_eq!(result.files[1].name, "pattern_002");
    assert_eq!(result.files[2].name, "pattern_003");
}

#[tokio::test]
async fn http_export_pool_empty_array() {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/export-pool")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"patterns":[]}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: ExportPoolResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.files.len(), 0);
}

#[tokio::test]
async fn http_export_pool_rejects_invalid_pattern() {
    let app = build_test_router();
    // Pattern with only 2 steps
    let body = r#"{"patterns":[{"active_steps":16,"triplet":false,"steps":[
        {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"},
        {"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}
    ]}]}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/export-pool")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_export_pool_output_is_re_importable() {
    // Export a pattern, then verify each format can be re-imported
    let app = build_test_router();
    let body = format!(r#"{{"patterns":[{}]}}"#, valid_web_pattern_json());
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/export-pool")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: ExportPoolResponse = serde_json::from_slice(&body).unwrap();
    let file = &result.files[0];

    // Re-import each format
    for (content, fmt) in [
        (&file.toml, "toml"),
        (&file.json, "json"),
        (&file.steps, "steps"),
    ] {
        let app = build_test_router();
        let import_body = serde_json::json!({ "content": content, "format": fmt }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/pattern/import")
            .header("content-type", "application/json")
            .body(Body::from(import_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "re-import failed for format {}",
            fmt
        );
    }
}

// =========================================================================
// CLI: control subcommand parses
// =========================================================================

#[test]
fn clap_parses_control_subcommand() {
    use crate::config::Cli;
    use clap::Parser;
    let cli =
        Cli::try_parse_from(["td3-control", "control", "--scratch-pattern", "G1P1A"]).unwrap();
    match cli.command.unwrap() {
        crate::config::Command::Control(args) => {
            assert!(args.port.is_none());
            assert!(args.bind.is_none());
            assert_eq!(args.scratch_pattern.as_deref(), Some("G1P1A"));
        }
        _ => panic!("expected Control command"),
    }
}

#[test]
fn clap_parses_control_with_custom_port() {
    use crate::config::Cli;
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "td3-control",
        "control",
        "--scratch-pattern",
        "G2P3B",
        "--port",
        "8080",
    ])
    .unwrap();
    match cli.command.unwrap() {
        crate::config::Command::Control(args) => {
            assert_eq!(args.port, Some(8080));
        }
        _ => panic!("expected Control command"),
    }
}

#[tokio::test]
async fn http_progression_config_returns_valid_json() {
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/config/progression")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Verify essential fields exist
    assert!(
        config["anchor_steps"].is_array(),
        "anchor_steps must be an array"
    );
    assert!(config["presets"].is_object(), "presets must be an object");
    assert!(config["mutation"].is_object(), "mutation must be an object");
    assert!(
        config["default_timeline"].is_array(),
        "default_timeline must be an array"
    );
    assert!(
        config["scale_profiles"].is_object(),
        "scale_profiles must be an object"
    );
    // Verify anchor_steps has 4 entries
    assert_eq!(config["anchor_steps"].as_array().unwrap().len(), 4);
    // Verify default_timeline has 16 entries
    assert_eq!(config["default_timeline"].as_array().unwrap().len(), 16);
}

#[tokio::test]
async fn http_progression_config_save_roundtrips() {
    // Read current config
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/config/progression")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let original: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Save it back
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/config/progression")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&original).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let save_resp: SaveConfigResponse = serde_json::from_slice(&body).unwrap();
    assert!(save_resp.ok);

    // Read again and verify it matches
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/config/progression")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let reloaded: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(original, reloaded);
}

#[tokio::test]
async fn http_progression_config_has_all_required_presets() {
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/config/progression")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify all 4 profiles exist in presets
    let presets = config["presets"]
        .as_object()
        .expect("presets must be object");
    for profile in &["safe", "dark", "tension", "jazz"] {
        assert!(
            presets.contains_key(*profile),
            "presets must contain '{}'",
            profile
        );
        let preset_list = presets[*profile].as_array().expect("preset must be array");
        for (i, preset) in preset_list.iter().enumerate() {
            let degrees = preset.as_array().expect("degree list must be array");
            assert_eq!(
                degrees.len(),
                4,
                "{} preset {} must have 4 degrees",
                profile,
                i
            );
        }
    }

    // Verify mutation config
    let mutation = config["mutation"]
        .as_object()
        .expect("mutation must be object");
    assert!(
        mutation.contains_key("target_changes"),
        "mutation must have target_changes"
    );
    assert!(
        mutation.contains_key("min_changes"),
        "mutation must have min_changes"
    );
    assert!(
        mutation.contains_key("max_changes"),
        "mutation must have max_changes"
    );

    // Verify anchor_steps values are valid step indices
    let anchors = config["anchor_steps"]
        .as_array()
        .expect("anchor_steps must be array");
    for anchor in anchors {
        let v = anchor.as_u64().expect("anchor must be number");
        assert!(v < 16, "anchor step {} must be < 16", v);
    }

    // Verify scale_profiles maps scale IDs to valid profile names
    let profiles = config["scale_profiles"]
        .as_object()
        .expect("scale_profiles must be object");
    let valid_profiles: Vec<&str> = vec!["safe", "dark", "tension", "jazz"];
    for (scale_id, profile_val) in profiles {
        let p = profile_val.as_str().expect("profile must be string");
        assert!(
            valid_profiles.contains(&p),
            "scale '{}' has profile '{}' which is not in {:?}",
            scale_id,
            p,
            valid_profiles
        );
    }
}

#[test]
fn clap_parses_control_with_custom_bind() {
    use crate::config::Cli;
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "td3-control",
        "control",
        "--scratch-pattern",
        "G1P1A",
        "--bind",
        "0.0.0.0",
    ])
    .unwrap();
    match cli.command.unwrap() {
        crate::config::Command::Control(args) => {
            assert_eq!(args.bind.as_deref(), Some("0.0.0.0"));
        }
        _ => panic!("expected Control command"),
    }
}

// =========================================================================
// HTTP integration: progression package export endpoint.
// =========================================================================
//
// These exercise the full handler path (not just `package_export`) so
// validation failures surface as HTTP 400 with the documented error shape,
// and the happy-path response plus on-disk ZIP filename are verified.

#[allow(clippy::too_many_arguments)]
fn build_export_body(
    package_id: &str,
    formats: &[&str],
    combined_rbs: bool,
    combined_sqs: bool,
    scale_name: &str,
    acid_len: usize,
    bass_len: usize,
    working_dir: Option<&str>,
) -> String {
    let fmt_arr = formats
        .iter()
        .map(|f| format!("\"{}\"", f))
        .collect::<Vec<_>>()
        .join(",");
    let acid: Vec<String> = (0..acid_len).map(|_| valid_web_pattern_json()).collect();
    let bass: Vec<String> = (0..bass_len).map(|_| valid_web_pattern_json()).collect();
    let combined = format!("{{\"rbs\":{},\"sqs\":{}}}", combined_rbs, combined_sqs);
    let working_dir_json = match working_dir {
        Some(dir) => format!(",\"workingDir\":{}", serde_json::to_string(dir).unwrap()),
        None => String::new(),
    };
    format!(
        r#"{{"packageId":"{pkg}","formats":[{fmts}],"combinedFormats":{comb},"scaleName":"{scl}","label":"Random Progression","acidPatterns":[{acid}],"basslines":[{bass}]{wd}}}"#,
        pkg = package_id,
        fmts = fmt_arr,
        comb = combined,
        scl = scale_name,
        acid = acid.join(","),
        bass = bass.join(","),
        wd = working_dir_json,
    )
}

/// Check that `name` satisfies:
///   `^PG_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}-[\w_]+-Random_Progression_Package\.zip$`
/// where the scale slot is one or more of [A-Za-z0-9_] (ASCII word chars +
/// underscore, matching the backend's sanitize_component output).
fn zip_filename(name: &str) -> bool {
    const SUFFIX: &str = "-Random_Progression_Package.zip";
    if !name.starts_with("PG_") || !name.ends_with(SUFFIX) {
        return false;
    }
    // Timestamp block immediately after PG_: YYYY-MM-DD_HH-MM-SS-
    // Example: "1970-01-01_00-00-00-"
    let rest = &name[3..];
    if rest.len() < 20 {
        return false;
    }
    let ts = &rest[..19];
    let ts_bytes = ts.as_bytes();
    if !ts_bytes[0..4].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if ts_bytes[4] != b'-' {
        return false;
    }
    if !ts_bytes[5..7].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if ts_bytes[7] != b'-' {
        return false;
    }
    if !ts_bytes[8..10].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if ts_bytes[10] != b'_' {
        return false;
    }
    if !ts_bytes[11..13].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if ts_bytes[13] != b'-' {
        return false;
    }
    if !ts_bytes[14..16].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if ts_bytes[16] != b'-' {
        return false;
    }
    if !ts_bytes[17..19].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if !rest[19..].starts_with('-') {
        return false;
    }
    // Scale segment = everything between the '-' after timestamp and SUFFIX.
    let scale_start = 3 + 20; // PG_ + timestamp + '-'
    let scale_end = name.len() - SUFFIX.len();
    if scale_end <= scale_start {
        return false;
    }
    let scale = &name[scale_start..scale_end];
    scale.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

async fn post_export(body: String) -> (StatusCode, serde_json::Value) {
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/progression/export-package")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_rejects_wrong_acid_count() {
    let body = build_export_body(
        "pkg_test",
        &["mid"],
        false,
        false,
        "natural_minor",
        3,
        4,
        None,
    );
    let (status, json) = post_export(body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.contains("acidPatterns"),
        "error mentions acidPatterns: {}",
        err
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_rejects_wrong_bassline_count() {
    let body = build_export_body(
        "pkg_test",
        &["mid"],
        false,
        false,
        "natural_minor",
        4,
        2,
        None,
    );
    let (status, json) = post_export(body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.contains("basslines"),
        "error mentions basslines: {}",
        err
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_rejects_empty_selection() {
    let body = build_export_body("pkg_test", &[], false, false, "natural_minor", 4, 4, None);
    let (status, json) = post_export(body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.to_lowercase().contains("at least one format"),
        "error mentions at-least-one: {}",
        err
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_rejects_unknown_format() {
    let body = build_export_body(
        "pkg_test",
        &["wav"],
        false,
        false,
        "natural_minor",
        4,
        4,
        None,
    );
    let (status, json) = post_export(body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.contains("wav"),
        "error mentions offending format: {}",
        err
    );
    assert!(
        err.to_lowercase().contains("unknown format"),
        "error phrased as unknown format: {}",
        err
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_rejects_missing_package_id() {
    let body = build_export_body("", &["mid"], false, false, "natural_minor", 4, 4, None);
    let (status, _json) = post_export(body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_happy_path_writes_zip_filename() {
    // Use a uniquely-named temp dir so parallel test runs don't collide.
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = std::env::temp_dir().join(format!("td3-pkg-export-http-{}-{}", pid, ts));
    std::fs::create_dir_all(&work).unwrap();

    let body = build_export_body(
        "pkg_test",
        &["mid", "steps_txt", "seq"],
        false,
        false,
        "natural_minor",
        4,
        4,
        Some(work.to_str().unwrap()),
    );
    let (status, json) = post_export(body).await;
    assert_eq!(status, StatusCode::OK, "response body: {}", json);
    assert_eq!(json["ok"].as_bool(), Some(true));
    assert_eq!(json["packageId"].as_str(), Some("pkg_test"));

    let zip_name = json["zipName"].as_str().unwrap();
    // Filename shape: `PG_YYYY-MM-DD_HH-MM-SS-{scale}-Random_Progression_Package.zip`
    assert!(
        zip_filename(zip_name),
        "zipName '{}' must match shape",
        zip_name
    );
    assert!(
        zip_name.contains("-natural_minor-"),
        "zipName contains scale segment: {}",
        zip_name
    );

    let saved_path = json["savedPath"].as_str().unwrap();
    assert!(
        std::path::Path::new(saved_path).exists(),
        "saved zip exists on disk: {}",
        saved_path
    );
    // 3 formats × 4 patterns × 2 (pattern + bassline) = 24 files.
    assert_eq!(json["fileCount"].as_u64(), Some(24));

    // Cleanup
    let _ = std::fs::remove_file(saved_path);
    let _ = std::fs::remove_dir_all(&work);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_export_package_sanitizes_scale_name_in_filename() {
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = std::env::temp_dir().join(format!("td3-pkg-export-san-{}-{}", pid, ts));
    std::fs::create_dir_all(&work).unwrap();

    // Scale name with unsafe characters - backend must sanitize but the output
    // must still satisfy the regex (\w and _ only in the scale slot).
    let body = build_export_body(
        "pkg_test",
        &["mid"],
        false,
        false,
        "weird/name with spaces",
        4,
        4,
        Some(work.to_str().unwrap()),
    );
    let (status, json) = post_export(body).await;
    assert_eq!(status, StatusCode::OK);
    let zip_name = json["zipName"].as_str().unwrap();
    assert!(
        zip_filename(zip_name),
        "Sanitized zipName still matches shape: {}",
        zip_name
    );
    assert!(
        !zip_name.contains('/') && !zip_name.contains(' '),
        "Unsafe chars scrubbed: {}",
        zip_name
    );

    let saved_path = json["savedPath"].as_str().unwrap();
    let _ = std::fs::remove_file(saved_path);
    let _ = std::fs::remove_dir_all(&work);
}

// =========================================================================
// Scratch pattern parsing
// =========================================================================

#[test]
fn parse_pattern_address_valid_formats() {
    use crate::config::parse_pattern_address;
    // Standard format
    let sp = parse_pattern_address("G1P1A").unwrap();
    assert_eq!((sp.patgroup, sp.slot, sp.side), (0, 0, 0));
    // Lowercase
    let sp = parse_pattern_address("g2p3b").unwrap();
    assert_eq!((sp.patgroup, sp.slot, sp.side), (1, 2, 1));
    // With dash
    let sp = parse_pattern_address("G4-P8B").unwrap();
    assert_eq!((sp.patgroup, sp.slot, sp.side), (3, 7, 1));
    // With space
    let sp = parse_pattern_address("G3 P5A").unwrap();
    assert_eq!((sp.patgroup, sp.slot, sp.side), (2, 4, 0));
}

#[test]
fn parse_pattern_address_label() {
    use crate::config::parse_pattern_address;
    let sp = parse_pattern_address("G2P4B").unwrap();
    assert_eq!(sp.label(), "G2-P4B");
}

#[test]
fn parse_pattern_address_rejects_invalid() {
    use crate::config::parse_pattern_address;
    assert!(parse_pattern_address("").is_err());
    assert!(parse_pattern_address("G0P1A").is_err()); // patgroup 0
    assert!(parse_pattern_address("G5P1A").is_err()); // patgroup 5
    assert!(parse_pattern_address("G1P0A").is_err()); // pattern 0
    assert!(parse_pattern_address("G1P9A").is_err()); // pattern 9
    assert!(parse_pattern_address("G1P1C").is_err()); // side C
    assert!(parse_pattern_address("XYZ").is_err()); // garbage
}

#[test]
fn clap_accepts_control_without_scratch_pattern() {
    // Post-TD3_CONFIG.env: `--scratch-pattern` is optional at the clap layer
    // because it falls back to `UI_SCRATCH_PATTERN` from TD3_CONFIG.env.
    // Validation of the resolved value happens in `load_config`.
    use crate::config::Cli;
    use clap::Parser;
    let cli = Cli::try_parse_from(["td3-control", "control"]).unwrap();
    match cli.command.unwrap() {
        crate::config::Command::Control(args) => {
            assert!(args.scratch_pattern.is_none());
        }
        _ => panic!("expected Control command"),
    }
}

// =========================================================================
// /api/config/env - boot-time UI defaults surface
// =========================================================================
//
// The endpoint serves the `UI_*` subset of `TD3_CONFIG.env` so the frontend
// can stamp defaults into the DOM without hard-coded numbers in HTML. The
// shape is a JSON object keyed by camelCase names matching
// `UiConfigSnapshot`. These tests use `UiConfigSnapshot::for_tests()` which
// mirrors the bundled template exactly, so assertions pin the contract.

#[tokio::test]
async fn http_config_env_returns_ui_defaults_from_snapshot() {
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/config/env")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Values must match UiConfigSnapshot::for_tests() (which matches the
    // bundled config/default_env.template).
    assert_eq!(json["uiAutoConnectToMidi"], true);
    assert_eq!(json["uiAutoSetLiveUpdate"], true);
    assert_eq!(json["uiDefaultBpm"], 120);
    assert_eq!(json["uiDefaultTriplet"], false);
    assert_eq!(json["uiMaxBankHistorySize"], 200);
    assert_eq!(json["uiRandDefaultRoot"], 0);
    assert_eq!(json["uiRandDefaultScale"], "minor");
    assert_eq!(json["uiRandNotePercent"], 50);
    assert_eq!(json["uiRandSlidePercent"], 20);
    assert_eq!(json["uiRandAccPercent"], 30);
    assert_eq!(json["uiRandUdPercent"], 30);
}

#[tokio::test]
async fn http_config_env_keys_are_stable_camel_case() {
    // The frontend stamper (ui/js/app-config.js) reads these exact keys.
    // If this test fails the frontend will silently fall back to HTML
    // defaults, so we pin the key set rather than just the values.
    let app = build_test_router();
    let req = Request::builder()
        .uri("/api/config/env")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let obj = json.as_object().expect("payload must be a JSON object");

    let expected_keys = [
        "uiAutoConnectToMidi",
        "uiAutoSetLiveUpdate",
        "uiDefaultBpm",
        "uiDefaultTriplet",
        "uiMaxBankHistorySize",
        "uiRandDefaultRoot",
        "uiRandDefaultScale",
        "uiRandNotePercent",
        "uiRandSlidePercent",
        "uiRandAccPercent",
        "uiRandUdPercent",
        "progressionNextPatternSaveStep",
    ];
    for key in expected_keys {
        assert!(obj.contains_key(key), "missing expected key: {}", key);
    }
    assert_eq!(obj.len(), expected_keys.len(), "unexpected extra keys");
}

// =========================================================================
// HTTP: /api/pattern/play-preview (transient sqs/rbs slot audition)
// =========================================================================
//
// Happy path cannot be exercised without a live MIDI session, so these tests
// cover the validation layer only: connection presence, BPM range, and the
// WebPattern shape check. The handler mirrors `bank_handlers::play_item` for
// the actual upload/transport - that path is already covered by the bank
// audition tests.

#[tokio::test]
async fn http_pattern_play_preview_requires_connection() {
    let app = build_test_router();
    let body = format!(r#"{{"pattern":{},"bpm":120}}"#, valid_web_pattern_json());
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/play-preview")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let err: ErrorBody = serde_json::from_slice(&body).unwrap();
    assert!(
        err.error.contains("not connected"),
        "error should mention connection: {}",
        err.error
    );
}

#[tokio::test]
async fn http_pattern_play_preview_rejects_bpm_zero() {
    let app = build_test_router();
    let body = format!(r#"{{"pattern":{},"bpm":0}}"#, valid_web_pattern_json());
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/play-preview")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_play_preview_rejects_bpm_over_300() {
    let app = build_test_router();
    let body = format!(r#"{{"pattern":{},"bpm":301}}"#, valid_web_pattern_json());
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/play-preview")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_play_preview_rejects_wrong_step_count() {
    // 15 steps instead of 16 - web_to_pattern should reject this at the
    // validation gate before any device interaction is attempted.
    let step = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let steps: Vec<&str> = (0..15).map(|_| step).collect();
    let pattern = format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    );
    let body = format!(r#"{{"pattern":{},"bpm":120}}"#, pattern);
    let app = build_test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/play-preview")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_pattern_play_preview_defaults_bpm_when_missing() {
    // Without `bpm` the handler defaults to 120; since there's no session
    // the validation path still reaches the "not connected" branch and
    // returns 400 - which is what we want to observe (no 422 from serde,
    // no panic from unwrap on a missing BPM).
    let app = build_test_router();
    let body = format!(r#"{{"pattern":{}}}"#, valid_web_pattern_json());
    let req = Request::builder()
        .method("POST")
        .uri("/api/pattern/play-preview")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let err: ErrorBody = serde_json::from_slice(&body).unwrap();
    assert!(
        err.error.contains("not connected"),
        "default-bpm path should reach not-connected: {}",
        err.error
    );
}
