use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};
use crate::web::clock::{
    coalescing_rejects_stale_without_losing_valid, cycle_timing_at_now,
    deadline_drain_observes_queued_update, flush_raw_sends_for_test, prepare_schedule,
    reject_closed_command_for_test, reject_closed_raw_send_for_test, remaining_start_delay,
    AuditionSchedule, AuditionTransitionTestHarness, AuditionUpdateError, ScheduledMidi,
    DEFAULT_AUDITION_CHANNEL,
};

const CYCLE_US: u64 = 100;

fn schedule(events: &[(u64, &[u8])]) -> AuditionSchedule {
    schedule_with_period(events, CYCLE_US)
}

fn schedule_with_period(events: &[(u64, &[u8])], cycle_period_us: u64) -> AuditionSchedule {
    AuditionSchedule {
        events: events
            .iter()
            .map(|(offset_us, bytes)| ScheduledMidi {
                offset_us: *offset_us,
                bytes: bytes.to_vec(),
                event_id: None,
            })
            .collect(),
        cycle_period_us,
        channel: DEFAULT_AUDITION_CHANNEL,
    }
}

fn rest_schedule() -> AuditionSchedule {
    schedule(&[])
}

fn normal_schedule(note: u8) -> AuditionSchedule {
    schedule(&[(0, &[0x90, note, 78]), (50, &[0x80, note, 64])])
}

fn boundary_release_schedule(note: u8) -> AuditionSchedule {
    schedule(&[(0, &[0x90, note, 78]), (CYCLE_US, &[0x80, note, 64])])
}

#[test]
fn normal_to_rest_rollover_does_not_replay_offset_zero() {
    let mut harness = AuditionTransitionTestHarness::new(normal_schedule(36));
    harness.dispatch_through(50).unwrap();
    harness.clear_sent();

    let acknowledgement =
        harness.queue_next_cycle(schedule_with_period(&[], CYCLE_US + 20), 12_000);
    harness.rollover().unwrap();

    assert!(harness.sent().is_empty());
    let applied = acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(applied.centibpm, 12_000);
    assert_eq!(applied.cycle_period_micros, CYCLE_US + 20);
    assert_eq!(harness.cycle_period_us(), CYCLE_US + 20);
}

#[test]
fn rest_to_normal_rollover_dispatches_offset_zero_note() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let acknowledgement = harness.queue_next_cycle(normal_schedule(38), 12_000);

    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x90, 38, 78]]);
    assert!(acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn terminal_boundary_note_off_precedes_new_offset_zero_once() {
    let mut harness = AuditionTransitionTestHarness::new(boundary_release_schedule(36));
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();
    let acknowledgement = harness.queue_next_cycle(normal_schedule(38), 12_000);

    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64], vec![0x90, 38, 78]]);
    assert_eq!(
        harness
            .sent()
            .iter()
            .filter(|bytes| bytes.as_slice() == [0x80, 36, 64])
            .count(),
        1
    );
    assert!(acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn terminal_boundary_note_off_is_not_resent_after_deadline_dispatch() {
    let mut harness = AuditionTransitionTestHarness::new(boundary_release_schedule(36));
    harness.dispatch_through(CYCLE_US).unwrap();
    let acknowledgement = harness.queue_next_cycle(normal_schedule(38), 12_000);

    harness.rollover().unwrap();

    assert_eq!(
        harness.sent(),
        &[vec![0x90, 36, 78], vec![0x80, 36, 64], vec![0x90, 38, 78]]
    );
    assert_eq!(
        harness
            .sent()
            .iter()
            .filter(|bytes| bytes.as_slice() == [0x80, 36, 64])
            .count(),
        1
    );
    assert!(acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn immediate_gate_change_waits_for_old_note_off_and_updates_subsequent_gate() {
    let old_schedule = schedule_with_period(
        &[
            (0, &[0x90, 36, 78]),
            (100, &[0x80, 36, 64]),
            (100, &[0x90, 38, 78]),
            (200, &[0x80, 38, 64]),
        ],
        250,
    );
    let updated_schedule = schedule_with_period(
        &[
            (0, &[0x90, 36, 78]),
            (25, &[0x80, 36, 64]),
            (100, &[0x90, 38, 78]),
            (125, &[0x80, 38, 64]),
        ],
        250,
    );
    let mut harness = AuditionTransitionTestHarness::new(old_schedule);
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();

    let acknowledgement = harness.queue_immediate_update(updated_schedule, 12_000);
    harness.dispatch_deadline_and_rollover(99).unwrap();

    assert!(harness.sent().is_empty());
    assert_eq!(acknowledgement.try_recv(), Err(TryRecvError::Empty));

    harness.dispatch_deadline_and_rollover(100).unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64]]);
    assert!(acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());

    harness.dispatch_deadline_and_rollover(100).unwrap();
    harness.dispatch_deadline_and_rollover(124).unwrap();
    assert_eq!(harness.sent(), &[vec![0x80, 36, 64], vec![0x90, 38, 78]]);

    harness.dispatch_deadline_and_rollover(125).unwrap();
    assert_eq!(
        harness.sent(),
        &[vec![0x80, 36, 64], vec![0x90, 38, 78], vec![0x80, 38, 64],]
    );
}

#[test]
fn rollover_flushes_all_overdue_note_offs_but_skips_overdue_note_ons() {
    let old = schedule(&[
        (0, &[0x90, 36, 78]),
        (20, &[0x80, 36, 64]),
        (30, &[0x90, 38, 78]),
        (80, &[0x80, 38, 64]),
        (90, &[0x90, 40, 0]),
    ]);
    let mut harness = AuditionTransitionTestHarness::new(old);
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();
    let acknowledgement = harness.queue_next_cycle(normal_schedule(42), 12_000);

    harness.rollover().unwrap();

    assert_eq!(
        harness.sent(),
        &[
            vec![0x80, 36, 64],
            vec![0x80, 38, 64],
            vec![0x90, 40, 0],
            vec![0x90, 42, 78],
        ]
    );
    assert_eq!(
        harness
            .sent()
            .iter()
            .filter(|bytes| bytes.as_slice() == [0x90, 38, 78])
            .count(),
        0
    );
    assert!(acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn final_active_slide_release_from_real_schedule_precedes_queued_note_on() {
    let rest = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);
    let mut steps = [rest; 16];
    steps[0] = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    steps[1] = Step::new(2, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    let pattern = Pattern::new(false, 2, steps).unwrap();
    let old_schedule = prepare_schedule(&pattern, 12_000, DEFAULT_AUDITION_CHANNEL).unwrap();
    let terminal_release = old_schedule
        .events
        .iter()
        .find(|event| {
            event.offset_us == old_schedule.cycle_period_us
                && matches!(event.bytes.first().map(|status| status & 0xF0), Some(0x80))
        })
        .expect("final slide must release at the cycle boundary")
        .bytes
        .clone();
    let old_cycle_us = old_schedule.cycle_period_us;
    let mut harness = AuditionTransitionTestHarness::new(old_schedule);
    harness.dispatch_through(old_cycle_us - 1).unwrap();
    harness.clear_sent();
    let acknowledgement = harness.queue_next_cycle(normal_schedule(42), 12_000);

    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[terminal_release, vec![0x90, 42, 78]]);
    assert!(acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn final_deadline_drain_observes_an_already_queued_transition() {
    assert!(deadline_drain_observes_queued_update(rest_schedule()));
}

#[test]
fn stale_coalesced_command_does_not_replace_valid_work() {
    assert!(coalescing_rejects_stale_without_losing_valid(
        rest_schedule()
    ));
}

#[test]
fn latest_queued_transition_supersedes_older_waiter() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let superseded = harness.queue_next_cycle(normal_schedule(36), 12_000);
    let latest = harness.queue_next_cycle(normal_schedule(40), 14_000);

    assert_eq!(
        superseded.try_recv(),
        Ok(Err(AuditionUpdateError::Superseded))
    );
    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x90, 40, 78]]);
    let applied = latest
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(applied.centibpm, 14_000);
}

#[test]
fn stop_releases_queued_transition_waiter() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let acknowledgement = harness.queue_next_cycle(normal_schedule(36), 12_000);

    harness.stop();

    assert_eq!(
        acknowledgement.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::AuditionStopped))
    );
}

#[test]
fn stop_after_boundary_off_prevents_new_note_and_releases_waiter() {
    let mut harness = AuditionTransitionTestHarness::new(boundary_release_schedule(36));
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();
    let acknowledgement = harness.queue_next_cycle(normal_schedule(38), 12_000);

    harness.rollover_stopping_after_boundary_event().unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64]]);
    assert_eq!(
        acknowledgement.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::AuditionStopped))
    );
}

#[test]
fn stop_during_offset_zero_send_rejects_success_acknowledgement() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let acknowledgement = harness.queue_next_cycle(normal_schedule(38), 12_000);

    harness.rollover_stopping_after_offset_zero().unwrap();

    assert_eq!(harness.sent(), &[vec![0x90, 38, 78]]);
    assert_eq!(
        acknowledgement.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::AuditionStopped))
    );
}

#[test]
fn queued_pattern_wins_when_immediate_update_is_pending_at_boundary() {
    let mut harness = AuditionTransitionTestHarness::new(boundary_release_schedule(36));
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();

    let immediate = harness.queue_immediate_update(normal_schedule(38), 24_000);
    let queued = harness.queue_next_cycle(normal_schedule(40), 24_000);
    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64], vec![0x90, 40, 78]]);
    assert_eq!(
        immediate.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::Superseded))
    );
    let applied = queued
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(applied.schedule_generation, 1);
    assert_eq!(harness.schedule_generation(), 1);
}

#[test]
fn due_boundary_commands_do_not_apply_immediate_before_queued_winner() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let immediate = harness.queue_immediate_update(normal_schedule(38), 24_000);
    let queued = harness.queue_next_cycle(normal_schedule(40), 24_000);

    harness.process_commands_at_due_cycle_boundary();
    assert_eq!(immediate.try_recv(), Err(TryRecvError::Empty));
    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x90, 40, 78]]);
    assert_eq!(
        immediate.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::Superseded))
    );
    assert!(queued
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn due_boundary_immediate_update_waits_for_normal_rollover_offset_zero() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let immediate = harness.queue_immediate_update(normal_schedule(38), 24_000);

    harness.process_commands_at_due_cycle_boundary();
    assert_eq!(immediate.try_recv(), Err(TryRecvError::Empty));
    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x90, 38, 78]]);
    let applied = immediate
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(applied.schedule_generation, 0);
    assert!(applied.phase_micros < applied.cycle_period_micros);
}

#[test]
fn immediate_boundary_update_rejects_failed_offset_zero_send() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let immediate = harness.queue_immediate_update(normal_schedule(38), 24_000);

    assert!(harness.rollover_failing_offset_zero().is_err());

    assert!(matches!(
        immediate.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::PlaybackFailed(_)))
    ));
}

#[test]
fn update_before_scheduled_start_does_not_reanchor_early() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let immediate = harness.queue_immediate_update(normal_schedule(38), 24_000);

    harness.process_commands_before_scheduled_start();

    assert_eq!(immediate.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(harness.schedule_generation(), 0);
}

#[test]
fn absolute_start_target_recomputes_only_remaining_delay() {
    assert_eq!(
        remaining_start_delay(10_000_000, 9_750_000),
        Duration::from_micros(250_000)
    );
    assert_eq!(
        remaining_start_delay(10_000_000, 10_050_000),
        Duration::ZERO
    );
}

#[test]
fn cycle_timing_acknowledgement_reports_elapsed_monotonic_phase() {
    let cycle_epoch = Instant::now()
        .checked_sub(Duration::from_millis(5))
        .unwrap();
    let acknowledgement = cycle_timing_at_now(12_000, 0, cycle_epoch, 1_000_000);

    assert!(acknowledgement.phase_micros >= 5_000);
    assert!(acknowledgement.phase_micros < acknowledgement.cycle_period_micros);
    assert_eq!(
        acknowledgement
            .effective_at_epoch_micros
            .saturating_sub(acknowledgement.cycle_epoch_micros),
        acknowledgement.phase_micros
    );
}

#[test]
fn closed_runner_command_reports_terminal_playback_failure() {
    let error = AuditionUpdateError::PlaybackFailed("initial send failed".to_string());
    assert_eq!(
        reject_closed_command_for_test(rest_schedule(), error.clone()),
        Err(error)
    );
}

#[test]
fn terminal_deadline_defers_pending_immediate_update_to_queued_rollover() {
    let mut harness = AuditionTransitionTestHarness::new(boundary_release_schedule(36));
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();

    let immediate = harness.queue_immediate_update(normal_schedule(38), 24_000);
    let queued = harness.queue_next_cycle(normal_schedule(40), 24_000);
    harness.dispatch_deadline_and_rollover(CYCLE_US).unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64], vec![0x90, 40, 78]]);
    assert_eq!(
        immediate.recv_timeout(Duration::from_millis(10)),
        Ok(Err(AuditionUpdateError::Superseded))
    );
    assert!(queued
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn queued_deadline_flushes_boundary_off_but_skips_boundary_on() {
    let old = schedule(&[
        (0, &[0x90, 36, 78]),
        (CYCLE_US, &[0x80, 36, 64]),
        (CYCLE_US, &[0x90, 38, 78]),
    ]);
    let mut harness = AuditionTransitionTestHarness::new(old);
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();
    let queued = harness.queue_next_cycle(normal_schedule(40), 12_000);

    harness.dispatch_deadline_and_rollover(CYCLE_US).unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64], vec![0x90, 40, 78]]);
    assert!(queued
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
}

#[test]
fn queue_arriving_during_boundary_flush_installs_before_offset_zero() {
    let mut harness = AuditionTransitionTestHarness::new(boundary_release_schedule(36));
    harness.dispatch_through(0).unwrap();
    harness.clear_sent();

    let queued = harness
        .rollover_queuing_during_boundary_event(normal_schedule(40), 12_000)
        .unwrap();

    assert_eq!(harness.sent(), &[vec![0x80, 36, 64], vec![0x90, 40, 78]]);
    let acknowledgement = queued
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(acknowledgement.schedule_generation, 1);
}

#[test]
fn queue_install_reanchors_when_at_least_one_new_cycle_late() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    harness.make_rollover_late_by(Duration::from_micros(1_100_000));
    let queued_schedule = schedule_with_period(
        &[(0, &[0x90, 40, 78]), (500_000, &[0x80, 40, 64])],
        1_000_000,
    );
    let acknowledgement = harness.queue_next_cycle(queued_schedule, 12_000);

    harness.rollover().unwrap();

    assert_eq!(harness.sent(), &[vec![0x90, 40, 78]]);
    assert!(!harness.cycle_is_due());
    let applied = acknowledgement
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(applied.schedule_generation, 1);
    assert!(applied.phase_micros < applied.cycle_period_micros);
}

#[test]
fn stale_commands_after_rollover_cannot_mutate_the_new_schedule() {
    let mut harness = AuditionTransitionTestHarness::new(normal_schedule(36));
    harness.dispatch_through(50).unwrap();
    let installed = harness.queue_next_cycle(normal_schedule(40), 12_000);
    harness.rollover().unwrap();
    assert_eq!(
        installed
            .recv_timeout(Duration::from_millis(10))
            .unwrap()
            .unwrap()
            .schedule_generation,
        1
    );
    harness.clear_sent();

    let stale_immediate =
        harness.queue_immediate_update_expected(normal_schedule(38), 24_000, Some(0));
    let stale_queue = harness.queue_next_cycle_expected(normal_schedule(42), 24_000, Some(0));
    assert_eq!(
        stale_immediate.try_recv(),
        Ok(Err(AuditionUpdateError::GenerationConflict))
    );
    assert_eq!(
        stale_queue.try_recv(),
        Ok(Err(AuditionUpdateError::GenerationConflict))
    );

    harness.dispatch_through(50).unwrap();
    harness.clear_sent();
    harness.rollover().unwrap();
    assert_eq!(harness.sent(), &[vec![0x90, 40, 78]]);
    assert_eq!(harness.schedule_generation(), 1);
}

#[test]
fn unguarded_queue_remains_backward_compatible() {
    let mut harness = AuditionTransitionTestHarness::new(rest_schedule());
    let first = harness.queue_next_cycle(normal_schedule(40), 12_000);
    harness.rollover().unwrap();
    assert!(first
        .recv_timeout(Duration::from_millis(10))
        .unwrap()
        .is_ok());
    harness.dispatch_through(50).unwrap();

    let unguarded = harness.queue_next_cycle_expected(normal_schedule(42), 12_000, None);
    harness.rollover().unwrap();

    assert_eq!(
        unguarded
            .recv_timeout(Duration::from_millis(10))
            .unwrap()
            .unwrap()
            .schedule_generation,
        2
    );
}

// ---------------------------------------------------------------------------
// Raw channel-voice sends through the audition thread
// ---------------------------------------------------------------------------

#[test]
fn raw_sends_are_written_in_arrival_order_and_each_requester_is_answered() {
    let cc_a: &[u8] = &[0xB0, 0x4A, 0x10];
    let cc_b: &[u8] = &[0xB0, 0x4A, 0x7F];
    let (written, results) = flush_raw_sends_for_test(&[cc_a, cc_b], rest_schedule(), |_| Ok(()));

    assert_eq!(written, vec![cc_a.to_vec(), cc_b.to_vec()]);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(Result::is_ok), "every requester gets Ok");
}

#[test]
fn raw_send_write_failure_is_reported_to_that_requester_only() {
    let good: &[u8] = &[0xB0, 0x4A, 0x01];
    let bad: &[u8] = &[0xB0, 0x4A, 0x02];
    let (written, results) = flush_raw_sends_for_test(&[good, bad], rest_schedule(), |bytes| {
        if bytes[2] == 0x02 {
            Err(crate::error::Td3Error::Midi("port gone".to_string()))
        } else {
            Ok(())
        }
    });

    assert_eq!(written.len(), 2, "a failed write does not stop later ones");
    assert!(results[0].is_ok());
    assert!(
        matches!(&results[1], Err(err) if err.to_string().contains("port gone")),
        "failure reaches its requester"
    );
}

#[test]
fn raw_send_to_a_stopped_audition_is_rejected_not_hung() {
    let result = reject_closed_raw_send_for_test();
    assert!(
        matches!(&result, Err(err) if err.to_string().contains("stopped")),
        "got {:?}",
        result
    );
}
