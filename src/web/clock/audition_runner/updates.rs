use std::time::{Duration, Instant};

use super::midi_events::next_event_index;
use super::schedule::AuditionSchedule;

pub(super) fn apply_schedule_update_now(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    updated: AuditionSchedule,
) {
    let now = Instant::now();
    let cycle_us = updated.cycle_period_us.max(1);
    let elapsed_us = now
        .saturating_duration_since(*epoch)
        .as_micros()
        .min(u64::MAX as u128) as u64;
    let phase_us = elapsed_us % cycle_us;

    apply_schedule_update_at_phase(
        schedule,
        cycle_period,
        epoch,
        next_event,
        updated,
        now,
        phase_us,
    );
}

pub(super) fn apply_schedule_update_at_phase(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    updated: AuditionSchedule,
    now: Instant,
    phase_us: u64,
) {
    let cycle_us = updated.cycle_period_us.max(1);
    let phase_us = phase_us % cycle_us;

    *schedule = updated;
    *cycle_period = Duration::from_micros(cycle_us);
    *epoch = now
        .checked_sub(Duration::from_micros(phase_us))
        .unwrap_or(now);
    *next_event = next_event_index(schedule, phase_us);
}
