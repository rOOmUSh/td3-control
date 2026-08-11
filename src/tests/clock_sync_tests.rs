use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use crate::web::api_types::{
    BpmRequest, TransportResponse, TransportWrapPulseRequest, TransportWrapPulseResponse,
};
use crate::web::clock::{
    next_tick_deadline, pattern_wrap_pulses, queued_send_rejection, wait_for_scheduled_start,
    ClockPulseTestHarness, ClockStartTestHarness,
};

#[test]
fn start_acknowledgement_waits_for_success() {
    let harness = ClockStartTestHarness::new(Duration::from_secs(1));
    let monitor = harness.monitor();
    let (result_tx, result_rx) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let _ = result_tx.send(monitor.wait_for_start());
    });

    assert!(result_rx.recv_timeout(Duration::from_millis(20)).is_err());
    harness.mark_started();
    assert!(result_rx
        .recv_timeout(Duration::from_millis(200))
        .expect("start waiter should receive acknowledgement")
        .is_ok());
    waiter.join().expect("start waiter thread should finish");

    harness.mark_stopped();
    assert!(harness.monitor().wait_for_start().is_ok());
}

#[test]
fn start_acknowledgement_propagates_send_failure() {
    let harness = ClockStartTestHarness::new(Duration::from_secs(1));
    harness.mark_failed("MIDI Start send failed: test driver error");
    harness.mark_stopped();

    let error = harness
        .monitor()
        .wait_for_start()
        .expect_err("failed Start send must not acknowledge success");
    assert!(error.to_string().contains("test driver error"));
}

#[test]
fn start_acknowledgement_reports_stop_before_start() {
    let harness = ClockStartTestHarness::new(Duration::from_secs(1));
    harness.mark_stopped();

    let error = harness
        .monitor()
        .wait_for_start()
        .expect_err("stopped runner must not acknowledge success");
    assert!(error
        .to_string()
        .contains("stopped before MIDI Start was sent"));

    harness.mark_started();
    assert!(harness.monitor().wait_for_start().is_err());
}

#[test]
fn start_acknowledgement_timeout_is_bounded() {
    let harness = ClockStartTestHarness::new(Duration::from_millis(25));
    let started = Instant::now();

    let error = harness
        .monitor()
        .wait_for_start()
        .expect_err("missing acknowledgement must time out");
    let elapsed = started.elapsed();

    assert!(matches!(error, crate::error::Td3Error::Timeout { .. }));
    assert!(elapsed >= Duration::from_millis(20));
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn scheduled_start_wait_stops_promptly() {
    let stop = Arc::new(AtomicBool::new(false));
    let waiter_stop = Arc::clone(&stop);
    let started = Instant::now();
    let waiter = thread::spawn(move || {
        wait_for_scheduled_start(Instant::now() + Duration::from_secs(1), &waiter_stop)
    });

    thread::sleep(Duration::from_millis(20));
    stop.store(true, Ordering::Release);

    assert!(!waiter
        .join()
        .expect("scheduled start waiter thread should finish"));
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn queued_sends_are_rejected_after_stop_or_abandon() {
    let stop = AtomicBool::new(false);
    let abandon = AtomicBool::new(false);
    assert_eq!(queued_send_rejection(&stop, &abandon), None);

    stop.store(true, Ordering::Release);
    assert_eq!(
        queued_send_rejection(&stop, &abandon),
        Some("clock transport stopped before queued send")
    );

    abandon.store(true, Ordering::Release);
    assert_eq!(
        queued_send_rejection(&stop, &abandon),
        Some("clock transport was abandoned after a MIDI timeout")
    );
}

#[test]
fn wrap_pulse_count_matches_normal_and_triplet_steps() {
    assert_eq!(pattern_wrap_pulses(16, false), 96);
    assert_eq!(pattern_wrap_pulses(16, true), 128);
    assert_eq!(pattern_wrap_pulses(1, false), 6);
}

#[test]
fn tempo_revision_applies_after_a_successful_monotonic_pulse() {
    let mut harness = ClockPulseTestHarness::new(12_000);
    let initial = harness.publish_pulse(1_000_000);
    assert_eq!(initial.pulse_index, 0);
    assert_eq!(initial.centibpm, 12_000);
    assert_eq!(initial.tempo_revision, 0);

    let revision = harness.set_centibpm(14_000);
    let applied = harness.publish_pulse(1_020_000);
    assert_eq!(applied.pulse_index, 1);
    assert_eq!(applied.centibpm, 14_000);
    assert_eq!(applied.tempo_revision, revision);

    let following = harness.publish_pulse(1_038_000);
    assert_eq!(following.pulse_index, 2);
    assert_eq!(following.tempo_revision, revision);
}

#[test]
fn rapid_tempo_requests_report_the_latest_applied_revision() {
    let mut harness = ClockPulseTestHarness::new(12_000);
    harness.publish_pulse(2_000_000);
    let first_revision = harness.set_centibpm(14_000);
    let latest_revision = harness.set_centibpm(16_000);
    assert!(latest_revision > first_revision);

    let monitor = harness.monitor();
    let applied = harness.publish_pulse(2_020_000);
    let observed = monitor
        .wait_for_tempo_revision(first_revision)
        .expect("latest tempo should satisfy an older revision waiter");
    assert_eq!(applied.centibpm, 16_000);
    assert_eq!(observed.tempo_revision, latest_revision);
    assert_eq!(observed.centibpm, 16_000);
}

#[test]
fn pulse_wait_returns_the_exact_boundary_snapshot() {
    let mut harness = ClockPulseTestHarness::new(12_000);
    let monitor = harness.monitor();
    let waiter = thread::spawn(move || monitor.wait_for_pulse(6));

    for pulse in 0..=6 {
        let snapshot = harness.publish_pulse(3_000_000 + pulse * 20_000);
        assert_eq!(snapshot.pulse_index, pulse);
    }

    let reached = waiter
        .join()
        .expect("pulse waiter thread should finish")
        .expect("pulse target should be reached");
    assert_eq!(reached.pulse_index, 6);
    assert_eq!(reached.epoch_micros, 3_120_000);
}

#[test]
fn expired_pulse_target_is_not_reported_as_the_latest_pulse() {
    let mut harness = ClockPulseTestHarness::new(12_000);
    let monitor = harness.monitor();

    for pulse in 0..=4_096 {
        harness.publish_pulse(4_000_000 + pulse * 20_000);
    }

    assert!(monitor.wait_for_pulse(0).is_none());
    assert_eq!(
        monitor
            .latest_running_pulse()
            .expect("running monitor should retain its latest pulse")
            .pulse_index,
        4_096
    );
}

#[test]
fn stopping_releases_pending_pulse_and_tempo_waiters() {
    let harness = ClockPulseTestHarness::new(12_000);
    let pulse_monitor = harness.monitor();
    let tempo_monitor = harness.monitor();
    let revision = harness.set_centibpm(14_000);
    let pulse_waiter = thread::spawn(move || pulse_monitor.wait_for_pulse(96));
    let tempo_waiter = thread::spawn(move || tempo_monitor.wait_for_tempo_revision(revision));

    harness.stop();

    assert!(pulse_waiter
        .join()
        .expect("pulse waiter thread should finish")
        .is_none());
    assert!(tempo_waiter
        .join()
        .expect("tempo waiter thread should finish")
        .is_none());
}

#[test]
fn changed_tempo_schedules_a_full_period_after_the_previous_tick() {
    let base = Instant::now();
    let scheduled = base + Duration::from_millis(20);
    let sent = base + Duration::from_millis(21);

    let changed = next_tick_deadline(scheduled, sent, 10_000, true);
    let stable = next_tick_deadline(scheduled, sent, 20_000, false);

    assert_eq!(changed, sent + Duration::from_millis(10));
    assert!(changed > sent);
    assert_eq!(stable, scheduled + Duration::from_millis(20));
}

#[test]
fn transport_response_serializes_applied_tempo_fields_as_camel_case() {
    let response = TransportResponse {
        ok: true,
        started_at_epoch_ms: 100,
        transport_id: 7,
        ppqn: 24,
        centibpm: Some(14_037),
        tempo_revision: Some(3),
        effective_pulse_index: Some(42),
        effective_at_epoch_micros: Some(123_456_789),
        server_epoch_micros: Some(123_456_999),
    };
    let json = serde_json::to_value(response).expect("transport response should serialize");

    assert_eq!(json["centibpm"], 14_037);
    assert_eq!(json["tempoRevision"], 3);
    assert_eq!(json["effectivePulseIndex"], 42);
    assert_eq!(json["effectiveAtEpochMicros"], 123_456_789u64);
    assert_eq!(json["serverEpochMicros"], 123_456_999u64);
}

#[test]
fn transport_start_accepts_browser_target_epoch_field() {
    let camel: BpmRequest =
        serde_json::from_str(r#"{"centibpm":12000,"targetEpochMicros":123456789}"#)
            .expect("browser transport request should deserialize");
    assert_eq!(camel.target_epoch_micros, Some(123_456_789));

    let legacy: BpmRequest =
        serde_json::from_str(r#"{"centibpm":12000,"target_epoch_micros":987654321}"#)
            .expect("legacy transport request should deserialize");
    assert_eq!(legacy.target_epoch_micros, Some(987_654_321));
}

#[test]
fn wrap_pulse_wire_fields_are_optional_and_camel_case() {
    let legacy: TransportWrapPulseRequest = serde_json::from_str(
        r#"{"transportId":7,"anchorEpochMs":100,"wrapIndex":2,"activeSteps":16,"triplet":false}"#,
    )
    .expect("legacy wrap request should deserialize");
    assert_eq!(legacy.anchor_pulse_index, None);

    let pulse: TransportWrapPulseRequest = serde_json::from_str(
        r#"{"transportId":7,"anchorEpochMs":100,"anchorPulseIndex":96,"wrapIndex":2,"activeSteps":16,"triplet":false}"#,
    )
    .expect("pulse wrap request should deserialize");
    assert_eq!(pulse.anchor_pulse_index, Some(96));

    let response = TransportWrapPulseResponse {
        ok: true,
        exact_boundary: Some(true),
        transport_id: 7,
        wrap_index: 3,
        wrap_epoch_ms: 200,
        wrap_epoch_micros: Some(200_123),
        server_epoch_ms: 201,
        ppqn: 24,
        pulse_index: Some(192),
    };
    let json = serde_json::to_value(response).expect("wrap response should serialize");
    assert_eq!(json["pulseIndex"], 192);
    assert_eq!(json["wrapEpochMicros"], 200_123);
    assert_eq!(json["exactBoundary"], true);
}
