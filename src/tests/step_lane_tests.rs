//! Per-step lanes: the LIVE cutoff lane tracker on the clock thread, the
//! audition timeline with per-step gate and cutoff, and the HTTP
//! validation of the lane fields.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::formats::mid::{build_timeline_with_gate, build_timeline_with_lanes, StepLanes};
use crate::library::LibraryStore;
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};
use crate::web::api_types::ErrorBody;
use crate::web::clock::cutoff_lane::{
    new_lane_inbox, CutoffLane, LaneRequest, LaneTracker, FILTER_CUTOFF_CC,
};
use crate::web::clock::prepare_schedule_with_lanes;
use crate::web::handlers;
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

// ---------------------------------------------------------------------------
// LaneTracker
// ---------------------------------------------------------------------------

fn lane(values: Option<[u8; 16]>, active_steps: u8, triplet: bool) -> CutoffLane {
    CutoffLane {
        values,
        active_steps,
        triplet,
        channel: 1,
    }
}

fn ramp() -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = (i * 8) as u8;
    }
    out
}

#[test]
fn tracker_without_a_lane_sends_nothing() {
    let mut tracker = LaneTracker::new();
    for pulse in 0..100 {
        assert!(tracker.on_pulse(pulse).is_none());
    }
}

#[test]
fn straight_lane_sends_cc74_every_six_pulses_in_step_order() {
    let mut tracker = LaneTracker::new();
    tracker.accept(LaneRequest {
        lane: Some(lane(Some(ramp()), 16, false)),
        at_cycle_boundary: false,
    });
    let mut sent = Vec::new();
    for pulse in 0..(6 * 16) {
        if let Some(cc) = tracker.on_pulse(pulse) {
            sent.push((pulse, cc));
        }
    }
    assert_eq!(sent.len(), 16, "one CC per step");
    for (i, (pulse, cc)) in sent.iter().enumerate() {
        assert_eq!(
            *pulse,
            (i * 6) as u64,
            "step {} starts on pulse {}",
            i,
            i * 6
        );
        assert_eq!(cc[0], 0xB0);
        assert_eq!(cc[1], FILTER_CUTOFF_CC);
        assert_eq!(cc[2], (i * 8) as u8);
    }
}

#[test]
fn triplet_lane_uses_eight_pulses_per_step_and_wraps_on_active_steps() {
    let mut tracker = LaneTracker::new();
    let mut values = [0u8; 16];
    values[0] = 10;
    values[1] = 20;
    values[2] = 30;
    tracker.accept(LaneRequest {
        lane: Some(lane(Some(values), 3, true)),
        at_cycle_boundary: false,
    });
    let sent: Vec<(u64, u8)> = (0..48)
        .filter_map(|pulse| tracker.on_pulse(pulse).map(|cc| (pulse, cc[2])))
        .collect();
    assert_eq!(
        sent,
        vec![(0, 10), (8, 20), (16, 30), (24, 10), (32, 20), (40, 30)],
        "three triplet steps repeat every 24 pulses"
    );
}

#[test]
fn boundary_request_waits_for_the_cycle_to_complete() {
    let mut tracker = LaneTracker::new();
    tracker.accept(LaneRequest {
        lane: Some(lane(Some([1u8; 16]), 4, false)),
        at_cycle_boundary: false,
    });
    assert_eq!(tracker.on_pulse(0).map(|cc| cc[2]), Some(1));
    tracker.accept(LaneRequest {
        lane: Some(lane(Some([2u8; 16]), 4, false)),
        at_cycle_boundary: true,
    });
    assert_eq!(
        tracker.on_pulse(6).map(|cc| cc[2]),
        Some(1),
        "old lane until the wrap"
    );
    assert_eq!(tracker.on_pulse(12).map(|cc| cc[2]), Some(1));
    assert_eq!(tracker.on_pulse(18).map(|cc| cc[2]), Some(1));
    assert_eq!(
        tracker.on_pulse(24).map(|cc| cc[2]),
        Some(2),
        "new lane at the wrap"
    );
    assert_eq!(tracker.current().map(|l| l.values), Some(Some([2u8; 16])));
}

#[test]
fn boundary_request_without_a_current_lane_applies_immediately() {
    let mut tracker = LaneTracker::new();
    tracker.accept(LaneRequest {
        lane: Some(lane(Some([5u8; 16]), 16, false)),
        at_cycle_boundary: true,
    });
    assert_eq!(tracker.on_pulse(0).map(|cc| cc[2]), Some(5));
}

#[test]
fn lane_without_values_keeps_timing_but_sends_nothing() {
    let mut tracker = LaneTracker::new();
    tracker.accept(LaneRequest {
        lane: Some(lane(None, 4, false)),
        at_cycle_boundary: false,
    });
    assert!(tracker.on_pulse(0).is_none());
    tracker.accept(LaneRequest {
        lane: Some(lane(Some([9u8; 16]), 4, false)),
        at_cycle_boundary: true,
    });
    assert!(
        tracker.on_pulse(6).is_none(),
        "still silent before the wrap"
    );
    assert_eq!(
        tracker.on_pulse(24).map(|cc| cc[2]),
        Some(9),
        "timing carried the wrap"
    );
}

#[test]
fn inbox_is_polled_without_blocking_and_newest_request_wins() {
    let inbox = new_lane_inbox();
    let mut tracker = LaneTracker::new();
    {
        let mut slot = inbox.lock().unwrap();
        *slot = Some(LaneRequest {
            lane: Some(lane(Some([3u8; 16]), 16, false)),
            at_cycle_boundary: false,
        });
        *slot = Some(LaneRequest {
            lane: Some(lane(Some([4u8; 16]), 16, false)),
            at_cycle_boundary: false,
        });
    }
    tracker.poll_inbox(&inbox);
    assert_eq!(tracker.on_pulse(0).map(|cc| cc[2]), Some(4));
    assert!(inbox.lock().unwrap().is_none(), "request consumed");

    // A held lock never stalls the clock: the poll is a no-op.
    let held = inbox.lock().unwrap();
    tracker.poll_inbox(&inbox);
    drop(held);
    let _ = Arc::clone(&inbox);
}

// ---------------------------------------------------------------------------
// Audition timeline with lanes
// ---------------------------------------------------------------------------

fn four_note_pattern() -> Pattern {
    let mut pattern = Pattern {
        active_steps: 4,
        ..Default::default()
    };
    for i in 0..4 {
        pattern.step[i] = Step {
            note: i as u8,
            transpose: Transpose::Normal,
            accent: Accent::Off,
            slide: Slide::Off,
            time: Time::Normal,
        };
    }
    pattern
}

fn options() -> crate::formats::mid::MidiExportOptions {
    crate::formats::mid::MidiExportOptions {
        bpm: 120,
        ppqn: 480,
        channel: 1,
        octave_offset: 0,
        accent_velocity: 110,
        normal_velocity: 78,
        slide_mode: crate::formats::mid::MidiSlideMode::Td3,
        loop_count: 1,
    }
}

#[test]
fn empty_lanes_reproduce_the_gate_only_timeline() {
    let pattern = four_note_pattern();
    let plain = build_timeline_with_gate(&pattern, "t", &options(), 70).unwrap();
    let laned =
        build_timeline_with_lanes(&pattern, "t", &options(), 70, StepLanes::default()).unwrap();
    assert_eq!(plain, laned);
}

#[test]
fn cutoff_lane_emits_cc74_at_every_step_start_before_the_note_on() {
    let pattern = four_note_pattern();
    let mut cutoffs = [64u8; 16];
    cutoffs[0] = 0;
    cutoffs[1] = 40;
    cutoffs[2] = 90;
    cutoffs[3] = 127;
    let lanes = StepLanes {
        gates: None,
        cutoffs: Some(cutoffs),
    };
    let timeline = build_timeline_with_lanes(&pattern, "t", &options(), 50, lanes).unwrap();
    let ccs: Vec<(u32, u8)> = timeline
        .iter()
        .filter(|ev| ev.data.first().map(|b| b & 0xF0) == Some(0xB0))
        .map(|ev| (ev.tick, ev.data[2]))
        .collect();
    assert_eq!(ccs, vec![(0, 0), (120, 40), (240, 90), (360, 127)]);
    for ev in &timeline {
        if ev.data.first().map(|b| b & 0xF0) == Some(0xB0) {
            assert_eq!(ev.data[1], 74);
        }
    }
    // The schedule keeps them and orders each before the Note On of its tick.
    let schedule = prepare_schedule_with_lanes(&pattern, 12_000, 50, lanes, 1).unwrap();
    let kinds: Vec<u8> = schedule
        .events
        .iter()
        .filter(|ev| ev.offset_us == 0)
        .map(|ev| ev.bytes[0] & 0xF0)
        .collect();
    assert_eq!(kinds, vec![0xB0, 0x90], "CC then Note On at offset zero");
}

#[test]
fn gate_lane_lengthens_and_shortens_individual_notes() {
    let pattern = four_note_pattern();
    let mut gates = [50u32; 16];
    gates[0] = 100;
    gates[1] = 10;
    let lanes = StepLanes {
        gates: Some(gates),
        cutoffs: None,
    };
    let timeline = build_timeline_with_lanes(&pattern, "t", &options(), 50, lanes).unwrap();
    let offs: Vec<u32> = timeline
        .iter()
        .filter(|ev| ev.data.first().map(|b| b & 0xF0) == Some(0x80))
        .map(|ev| ev.tick)
        .collect();
    // step ticks = 120: step 0 holds a full step, step 1 holds 12 ticks,
    // steps 2 and 3 keep the pattern gate of 60 ticks.
    assert_eq!(offs, vec![120, 132, 300, 420]);
}

#[test]
fn cutoff_lane_is_emitted_on_rest_steps_too() {
    let mut pattern = four_note_pattern();
    pattern.step[1].time = Time::Rest;
    let lanes = StepLanes {
        gates: None,
        cutoffs: Some([7u8; 16]),
    };
    let timeline = build_timeline_with_lanes(&pattern, "t", &options(), 50, lanes).unwrap();
    let cc_ticks: Vec<u32> = timeline
        .iter()
        .filter(|ev| ev.data.first().map(|b| b & 0xF0) == Some(0xB0))
        .map(|ev| ev.tick)
        .collect();
    assert_eq!(cc_ticks, vec![0, 120, 240, 360]);
}

// ---------------------------------------------------------------------------
// HTTP validation
// ---------------------------------------------------------------------------

fn build_router() -> Router {
    let path = std::env::temp_dir().join(format!(
        "td3-steplane-test-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    let library = Arc::new(LibraryStore::load_or_create(path).expect("test library"));
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
    Router::new()
        .route("/api/pattern/audition", post(handlers::pattern_audition))
        .route(
            "/api/transport/step-lane",
            post(handlers::transport_step_lane),
        )
        .with_state(state)
}

async fn post_json(path: &str, body: String) -> (StatusCode, String) {
    let app = build_router();
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    let status = resp.status();
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    let text = match serde_json::from_slice::<ErrorBody>(&bytes) {
        Ok(err) => err.error,
        Err(_) => String::from_utf8_lossy(&bytes).to_string(),
    };
    (status, text)
}

fn pattern_json() -> String {
    let step = r#"{"note":"C","transpose":"NORMAL","accent":false,"slide":false,"time":"NORMAL"}"#;
    let steps: Vec<&str> = (0..16).map(|_| step).collect();
    format!(
        r#"{{"active_steps":16,"triplet":false,"steps":[{}]}}"#,
        steps.join(",")
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audition_rejects_short_step_gate_lane() {
    let body = format!(r#"{{"pattern":{},"stepGates":[50,50]}}"#, pattern_json());
    let (status, text) = post_json("/api/pattern/audition", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        text.contains("stepGates must have exactly 16"),
        "got: {}",
        text
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audition_rejects_out_of_range_step_cutoff() {
    let mut values = vec![64u32; 16];
    values[3] = 128;
    let body = format!(
        r#"{{"pattern":{},"stepCutoffs":{}}}"#,
        pattern_json(),
        serde_json::to_string(&values).unwrap()
    );
    let (status, text) = post_json("/api/pattern/audition", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        text.contains("stepCutoffs[3] must be 0-127"),
        "got: {}",
        text
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audition_accepts_lanes_combined_with_morph() {
    let body = format!(
        r#"{{"pattern":{},"stepCutoffs":{},"tripletMorphPercent":30}}"#,
        pattern_json(),
        serde_json::to_string(&vec![64u32; 16]).unwrap()
    );
    let (status, text) = post_json("/api/pattern/audition", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        text, "not connected",
        "lanes accepted while morphing, then no session"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audition_accepts_lanes_at_morph_amount_zero() {
    let body = format!(
        r#"{{"pattern":{},"stepCutoffs":{},"tripletMorphPercent":0}}"#,
        pattern_json(),
        serde_json::to_string(&vec![64u32; 16]).unwrap()
    );
    let (status, text) = post_json("/api/pattern/audition", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        text, "not connected",
        "lanes accepted at amount 0, then no session"
    );
}

#[test]
fn morph_amount_zero_schedule_carries_lane_cc_with_step_identities() {
    use crate::triplet_morph::{MorphAmount, MorphEventRole};
    use crate::web::clock::prepare_morph_schedule_with_lanes;

    let pattern = four_note_pattern();
    let lanes = StepLanes {
        gates: None,
        cutoffs: Some([3u8; 16]),
    };
    let (schedule, _plan) = prepare_morph_schedule_with_lanes(
        &pattern,
        12_000,
        50,
        lanes,
        MorphAmount::new(0).unwrap(),
        1,
    )
    .unwrap();
    let ccs: Vec<(u8, u8)> = schedule
        .events
        .iter()
        .filter(|ev| ev.bytes[0] & 0xF0 == 0xB0)
        .map(|ev| {
            let id = ev.event_id.expect("lane CC carries an identity");
            assert_eq!(id.role, MorphEventRole::ControlChange);
            (id.source_step, ev.bytes[2])
        })
        .collect();
    assert_eq!(ccs, vec![(0, 3), (1, 3), (2, 3), (3, 3)]);
}

/// Sixteen straight attacks with a rising cutoff ramp and one long gate.
fn sixteen_note_pattern() -> Pattern {
    let mut pattern = Pattern {
        active_steps: 16,
        ..Default::default()
    };
    for i in 0..16 {
        pattern.step[i] = Step {
            note: (i % 12) as u8,
            transpose: Transpose::Normal,
            accent: Accent::Off,
            slide: Slide::Off,
            time: Time::Normal,
        };
    }
    pattern
}

#[test]
fn intermediate_morph_moves_lane_cc_with_its_cell_and_drops_retired_cells() {
    use crate::triplet_morph::{MorphAmount, MorphEventRole};
    use crate::web::clock::prepare_morph_schedule_with_lanes;

    let pattern = sixteen_note_pattern();
    let mut cutoffs = [0u8; 16];
    for (i, slot) in cutoffs.iter_mut().enumerate() {
        *slot = (i * 8) as u8;
    }
    let lanes = StepLanes {
        gates: None,
        cutoffs: Some(cutoffs),
    };
    let (straight, _) = prepare_morph_schedule_with_lanes(
        &pattern,
        12_000,
        50,
        lanes,
        MorphAmount::new(0).unwrap(),
        1,
    )
    .unwrap();
    let (warped, _) = prepare_morph_schedule_with_lanes(
        &pattern,
        12_000,
        50,
        lanes,
        MorphAmount::new(40).unwrap(),
        1,
    )
    .unwrap();
    let ccs = |schedule: &crate::web::clock::AuditionSchedule| -> Vec<(u8, u64, u8)> {
        schedule
            .events
            .iter()
            .filter(|ev| ev.bytes[0] & 0xF0 == 0xB0)
            .map(|ev| {
                let id = ev.event_id.expect("lane CC identity");
                assert_eq!(id.role, MorphEventRole::ControlChange);
                (id.source_step, ev.offset_us, ev.bytes[2])
            })
            .collect()
    };
    let straight_ccs = ccs(&straight);
    let warped_ccs = ccs(&warped);
    assert_eq!(straight_ccs.len(), 16, "every cell sends at amount 0");
    assert_eq!(
        warped_ccs.len(),
        16,
        "below retirement every cell still sends"
    );
    for (step, offset, value) in &warped_ccs {
        assert_eq!(
            *value,
            cutoffs[usize::from(*step)],
            "value follows its source step"
        );
        // Each CC sits exactly on its own cell's Note On.
        let note_on = warped
            .events
            .iter()
            .find(|ev| {
                ev.bytes[0] & 0xF0 == 0x90 && ev.event_id.map(|id| id.source_step) == Some(*step)
            })
            .expect("attack for step");
        assert_eq!(
            *offset, note_on.offset_us,
            "CC of step {} rides with its note",
            step
        );
    }
    let moved = warped_ccs
        .iter()
        .zip(straight_ccs.iter())
        .filter(|(w, s)| w.1 != s.1)
        .count();
    assert!(
        moved > 0,
        "warped cells have moved from their straight offsets"
    );

    let (retired, _) = prepare_morph_schedule_with_lanes(
        &pattern,
        12_000,
        50,
        lanes,
        MorphAmount::new(90).unwrap(),
        1,
    )
    .unwrap();
    let retired_ccs = ccs(&retired);
    let note_ons = retired
        .events
        .iter()
        .filter(|ev| ev.bytes[0] & 0xF0 == 0x90)
        .count();
    assert!(retired_ccs.len() < 16, "retired cells send no CC");
    assert_eq!(retired_ccs.len(), note_ons, "one CC per surviving attack");
}

#[test]
fn intermediate_morph_applies_per_step_gate_before_compensation() {
    use crate::triplet_morph::MorphAmount;
    use crate::web::clock::prepare_morph_schedule_with_lanes;

    let pattern = sixteen_note_pattern();
    let mut gates = [50u32; 16];
    gates[0] = 10;
    gates[1] = 90;
    let lanes = StepLanes {
        gates: Some(gates),
        cutoffs: None,
    };
    let (schedule, _) = prepare_morph_schedule_with_lanes(
        &pattern,
        12_000,
        50,
        lanes,
        MorphAmount::new(30).unwrap(),
        1,
    )
    .unwrap();
    let length_of = |step: u8| -> u64 {
        let on = schedule
            .events
            .iter()
            .find(|ev| {
                ev.bytes[0] & 0xF0 == 0x90 && ev.event_id.map(|id| id.source_step) == Some(step)
            })
            .unwrap()
            .offset_us;
        let off = schedule
            .events
            .iter()
            .find(|ev| {
                ev.bytes[0] & 0xF0 == 0x80 && ev.event_id.map(|id| id.source_step) == Some(step)
            })
            .unwrap()
            .offset_us;
        off - on
    };
    let short = length_of(0);
    let long = length_of(1);
    let plain = length_of(2);
    assert!(
        short < plain && plain < long,
        "short {} < plain {} < long {}",
        short,
        plain,
        long
    );
}

#[test]
fn endpoint_morph_remaps_lanes_onto_surviving_cells() {
    use crate::triplet_morph::{MorphAmount, MorphEventRole};
    use crate::web::clock::prepare_morph_schedule_with_lanes;

    let pattern = sixteen_note_pattern();
    let mut cutoffs = [0u8; 16];
    for (i, slot) in cutoffs.iter_mut().enumerate() {
        *slot = (i * 8) as u8;
    }
    let lanes = StepLanes {
        gates: None,
        cutoffs: Some(cutoffs),
    };
    let (schedule, _) = prepare_morph_schedule_with_lanes(
        &pattern,
        12_000,
        50,
        lanes,
        MorphAmount::new(100).unwrap(),
        1,
    )
    .unwrap();
    let ccs: Vec<(u8, u8)> = schedule
        .events
        .iter()
        .filter(|ev| ev.bytes[0] & 0xF0 == 0xB0)
        .map(|ev| {
            let id = ev.event_id.unwrap();
            assert_eq!(id.role, MorphEventRole::ControlChange);
            (id.source_step, ev.bytes[2])
        })
        .collect();
    assert_eq!(ccs.len(), 12, "twelve triplet cells, one CC each");
    for (source_step, value) in ccs {
        assert_eq!(
            value,
            cutoffs[usize::from(source_step)],
            "cell carries its source value"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audition_with_valid_lanes_reaches_the_session_check() {
    let body = format!(
        r#"{{"pattern":{},"stepCutoffs":{},"stepGates":{}}}"#,
        pattern_json(),
        serde_json::to_string(&vec![64u32; 16]).unwrap(),
        serde_json::to_string(&vec![50u32; 16]).unwrap()
    );
    let (status, text) = post_json("/api/pattern/audition", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, "not connected", "lanes validated, then no session");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn step_lane_rejects_bad_active_steps_and_values() {
    let (status, text) = post_json(
        "/api/transport/step-lane",
        r#"{"cutoffs":null,"activeSteps":17}"#.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("activeSteps must be 1-16"), "got: {}", text);

    let (status, text) = post_json(
        "/api/transport/step-lane",
        r#"{"cutoffs":[1,2,3],"activeSteps":16}"#.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("exactly 16 values"), "got: {}", text);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn step_lane_accepts_a_lane_without_a_session() {
    let body = format!(
        r#"{{"cutoffs":{},"activeSteps":16,"triplet":false,"atCycleBoundary":true}}"#,
        serde_json::to_string(&vec![64u32; 16]).unwrap()
    );
    let (status, text) = post_json("/api/transport/step-lane", body).await;
    assert_eq!(status, StatusCode::OK, "got: {}", text);
}
