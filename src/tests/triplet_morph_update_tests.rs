//! Identity-safe schedule replacement tests for morph updates: dispatch
//! ledger replay protection, safety-margin deferral to the next cycle,
//! queued-transition ownership, and apply-mode acknowledgements.

use crate::triplet_morph::MorphAmount;
use crate::web::clock::{
    prepare_morph_schedule, AuditionApplyMode, AuditionSchedule, AuditionTransitionTestHarness,
    AuditionUpdateError, DEFAULT_AUDITION_CHANNEL,
};

use super::fixtures::straight_sixteen;

const CENTIBPM_120: u32 = 12_000;

fn morph_schedule(amount: u32) -> AuditionSchedule {
    morph_schedule_at(amount, CENTIBPM_120)
}

fn morph_schedule_at(amount: u32, centibpm: u32) -> AuditionSchedule {
    let pattern = straight_sixteen();
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        centibpm,
        50,
        MorphAmount::new(amount).expect("amount in range"),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    schedule
}

fn count_note_ons(sent: &[Vec<u8>]) -> usize {
    sent.iter()
        .filter(|bytes| {
            matches!(bytes.first().map(|b| b & 0xF0), Some(0x90))
                && bytes.get(2).copied().unwrap_or(0) > 0
        })
        .count()
}

#[test]
fn rapid_amounts_install_only_the_latest_pending_schedule() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(0));
    // Play through the first attack and its release so the ledger knows
    // step 0 is consumed.
    harness.dispatch_through(70_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 1);

    let rx10 = harness.queue_immediate_update(morph_schedule(10), CENTIBPM_120);
    let rx40 = harness.queue_immediate_update(morph_schedule(40), CENTIBPM_120);
    let rx90 = harness.queue_immediate_update(morph_schedule(90), CENTIBPM_120);

    assert!(harness.try_apply_pending_at_phase(70_000), "safe install");

    for rx in [rx10, rx40, rx90] {
        let ack = rx.recv().expect("channel").expect("acknowledged");
        assert_eq!(ack.cycle_period_micros, 2_000_000);
        assert_eq!(ack.apply_mode, AuditionApplyMode::CurrentCycleFuture);
    }

    // At amount 90 the second attack moved to 162,500 us; the stale
    // amount-10 onset at 129,166 us must not exist in the installed
    // schedule.
    harness.dispatch_through(150_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 1, "no early second attack");
    harness.dispatch_through(165_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 2, "latest offset fires");
}

#[test]
fn a_dispatched_event_moved_into_the_future_is_not_replayed() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(10));
    // Dispatch through the second attack at its amount-10 onset.
    harness.dispatch_through(130_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 2);

    // Move the second attack into the future: 145,416 us at amount 49.
    // Amount 49 stays below the collision retirement threshold, so all
    // sixteen attacks are still emitted and this exercises replay
    // protection alone.
    let rx = harness.queue_immediate_update(morph_schedule(49), CENTIBPM_120);
    assert!(harness.try_apply_pending_at_phase(130_000), "safe install");
    rx.recv().expect("channel").expect("acknowledged");

    harness.dispatch_through(2_000_000).expect("dispatch");
    // Sixteen source attacks, each exactly once: the moved second
    // attack is consumed and never replayed.
    assert_eq!(count_note_ons(harness.sent()), 16);
}

#[test]
fn a_safe_undispatched_event_adopts_its_new_offset() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(90));
    harness.dispatch_through(60_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 1);

    // Move the undispatched second attack earlier, but still safely
    // ahead of the current phase (129,166 us at amount 10 vs 60,000).
    // The phase sits before the first release in both schedules
    // (102,916 us at amount 90, 71,964 us at amount 10), so nothing
    // already dispatched is disturbed and the install is safe.
    let rx = harness.queue_immediate_update(morph_schedule(10), CENTIBPM_120);
    assert!(harness.try_apply_pending_at_phase(60_000), "safe install");
    rx.recv().expect("channel").expect("acknowledged");

    harness.dispatch_through(130_000).expect("dispatch");
    assert_eq!(
        count_note_ons(harness.sent()),
        2,
        "new earlier offset fires"
    );
}

#[test]
fn an_event_moved_behind_the_phase_defers_the_complete_update() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(90));
    harness.dispatch_through(140_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 1);

    // At amount 10 the second attack sits at 129,166 us, behind the
    // 140,000 us phase, and was never dispatched: the whole update must
    // wait for the boundary instead of firing late or being dropped.
    let rx = harness.queue_immediate_update(morph_schedule(10), CENTIBPM_120);
    assert!(!harness.try_apply_pending_at_phase(140_000), "must defer");
    assert!(harness.pending_update_is_deferred());

    // The old schedule stays audibly authoritative for this cycle.
    harness.dispatch_through(163_000).expect("dispatch");
    assert_eq!(count_note_ons(harness.sent()), 2, "old offset still fires");

    harness.rollover().expect("rollover");
    let ack = rx.recv().expect("channel").expect("acknowledged at wrap");
    assert_eq!(ack.apply_mode, AuditionApplyMode::NextCycle);
    assert!(!harness.pending_update_is_deferred());
}

#[test]
fn a_deferred_update_cannot_overwrite_a_queued_timeline_pattern() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(90));
    harness.dispatch_through(140_000).expect("dispatch");

    let queued_rx = harness.queue_next_cycle(morph_schedule(90), CENTIBPM_120);
    let deferred_rx = harness.queue_immediate_update(morph_schedule(10), CENTIBPM_120);
    assert!(!harness.try_apply_pending_at_phase(140_000), "must defer");

    harness.rollover().expect("rollover");

    // The queued timeline transition owns the boundary; the deferred
    // current-pattern amount is superseded, not silently installed.
    let deferred = deferred_rx.recv().expect("channel");
    assert_eq!(deferred, Err(AuditionUpdateError::Superseded));
    let queued = queued_rx.recv().expect("channel").expect("installed");
    assert_eq!(queued.apply_mode, AuditionApplyMode::NextCycle);
    assert_eq!(queued.schedule_generation, 1);
    assert_eq!(harness.schedule_generation(), 1);
}

#[test]
fn a_morph_only_update_preserves_phase_and_cycle_period() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(0));
    harness.dispatch_through(70_000).expect("dispatch");
    harness.set_phase(70_000);
    let epoch_before = harness.cycle_epoch();

    let rx = harness.queue_immediate_update(morph_schedule(55), CENTIBPM_120);
    assert!(harness.try_apply_pending_at_phase(70_000), "safe install");
    assert_eq!(
        harness.cycle_epoch(),
        epoch_before,
        "morph-only install must not move the cycle epoch"
    );
    let ack = rx.recv().expect("channel").expect("acknowledged");
    assert_eq!(ack.cycle_period_micros, 2_000_000, "cycle must not change");
    assert!(
        ack.phase_micros >= 70_000 && ack.phase_micros < 120_000,
        "phase preserved, got {}",
        ack.phase_micros
    );
}

#[test]
fn hammered_morph_updates_keep_the_cycle_epoch_bit_identical() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(0));
    harness.dispatch_through(70_000).expect("dispatch");

    // Sweep-like hammer: many equal-cycle installs, up and down, at
    // varied phases. Every install must leave the cycle epoch exactly
    // where it was; re-deriving it from the wall clock would fold
    // dispatch latency into the grid and permanently step the bar.
    let amounts = [
        5u32, 15, 25, 35, 45, 55, 65, 75, 85, 95, 90, 80, 70, 60, 50, 40, 30, 20, 10, 0,
    ];
    for pass in 0..5u64 {
        for (i, &amount) in amounts.iter().enumerate() {
            let phase = 70_000 + pass * 900 + i as u64 * 37;
            harness.set_phase(phase);
            let epoch_before = harness.cycle_epoch();
            let rx = harness.queue_immediate_update(morph_schedule(amount), CENTIBPM_120);
            assert!(
                harness.try_apply_pending_at_phase(phase),
                "install pass {} amount {}",
                pass,
                amount
            );
            assert_eq!(
                harness.cycle_epoch(),
                epoch_before,
                "cycle epoch moved: pass {} amount {}",
                pass,
                amount
            );
            let ack = rx.recv().expect("channel").expect("acknowledged");
            assert_eq!(ack.cycle_period_micros, 2_000_000);
            assert_eq!(ack.apply_mode, AuditionApplyMode::CurrentCycleFuture);
        }
    }
}

#[test]
fn a_real_tempo_change_still_reanchors_the_epoch() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(0));
    harness.dispatch_through(70_000).expect("dispatch");
    harness.set_phase(70_000);
    let epoch_before = harness.cycle_epoch();

    // 240 BPM halves the cycle; the epoch must re-anchor so the
    // normalized phase carries into the new cycle length.
    let rx = harness.queue_immediate_update(morph_schedule_at(0, 24_000), 24_000);
    assert!(
        harness.try_apply_pending_at_phase(70_000),
        "tempo change installs"
    );
    assert_ne!(
        harness.cycle_epoch(),
        epoch_before,
        "a tempo change is the one case that re-anchors"
    );
    let ack = rx.recv().expect("channel").expect("acknowledged");
    assert_eq!(ack.cycle_period_micros, 1_000_000);
    assert_eq!(harness.cycle_period_us(), 1_000_000);
}

#[test]
fn a_stale_generation_morph_update_is_rejected() {
    let mut harness = AuditionTransitionTestHarness::new(morph_schedule(0));
    let rx = harness.queue_immediate_update_expected(morph_schedule(30), CENTIBPM_120, Some(7));
    let result = rx.recv().expect("channel");
    assert_eq!(result, Err(AuditionUpdateError::GenerationConflict));
}
