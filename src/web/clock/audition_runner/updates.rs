use std::time::{Duration, Instant};

use super::commands::{
    AuditionApplyMode, AuditionScheduleUpdate, AuditionUpdateAck, AuditionUpdateAcknowledgements,
};
use super::midi_events::{next_event_index, DispatchLedger};
use super::schedule::AuditionSchedule;

/// Minimum lead time, in microseconds, an undispatched replacement event
/// must keep ahead of the current cycle phase before an in-place morph
/// update is installed. Covers the scheduler spin threshold plus send
/// latency so a replacement event is never fired late.
pub(super) const MORPH_UPDATE_SAFETY_MARGIN_US: u64 = 2_000;

/// Whether a schedule update may replace the current cycle's remainder
/// in place, or must wait for the next cycle boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MorphUpdateDisposition {
    InstallNow,
    DeferToBoundary,
}

/// Identity-safe replacement check for schedules carrying stable event
/// identities. Every event already dispatched in the current cycle is
/// treated as consumed even if its new offset moved into the future.
/// Every undispatched event must have a deadline strictly after the
/// current phase plus the safety margin; otherwise the complete update
/// defers to the cycle boundary. Schedules without identities keep the
/// legacy in-place installation behavior.
pub(super) fn morph_update_disposition(
    old_cycle_us: u64,
    phase_us: u64,
    replacement: &AuditionSchedule,
    dispatched: &DispatchLedger,
) -> MorphUpdateDisposition {
    let has_identities = replacement
        .events
        .iter()
        .any(|event| event.event_id.is_some());
    if !has_identities {
        return MorphUpdateDisposition::InstallNow;
    }
    let new_cycle_us = replacement.cycle_period_us.max(1);
    let scaled_phase = scale_cycle_phase(phase_us, old_cycle_us.max(1), new_cycle_us);
    let horizon = scaled_phase.saturating_add(MORPH_UPDATE_SAFETY_MARGIN_US);
    for event in &replacement.events {
        if event.offset_us > horizon {
            break;
        }
        let consumed = event.event_id.is_some_and(|id| dispatched.contains(&id));
        if !consumed {
            return MorphUpdateDisposition::DeferToBoundary;
        }
    }
    MorphUpdateDisposition::InstallNow
}

pub(super) fn apply_schedule_update_now(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    updated: AuditionScheduleUpdate,
    schedule_generation: u64,
) {
    let now = Instant::now();
    let old_cycle_us = cycle_period.as_micros().max(1).min(u64::MAX as u128) as u64;
    let elapsed_us = now
        .saturating_duration_since(*epoch)
        .as_micros()
        .min(u64::MAX as u128) as u64;
    let phase_us = elapsed_us % old_cycle_us;

    apply_schedule_update_at_phase(
        schedule,
        cycle_period,
        epoch,
        next_event,
        updated,
        schedule_generation,
        now,
        phase_us,
    );
}

/// Current cycle phase, in microseconds, derived from the runner epoch.
pub(super) fn current_cycle_phase_us(epoch: Instant, cycle_period: Duration) -> u64 {
    let old_cycle_us = cycle_period.as_micros().max(1).min(u64::MAX as u128) as u64;
    let elapsed_us = Instant::now()
        .saturating_duration_since(epoch)
        .as_micros()
        .min(u64::MAX as u128) as u64;
    elapsed_us % old_cycle_us
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_schedule_update_at_phase(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    updated: AuditionScheduleUpdate,
    schedule_generation: u64,
    now: Instant,
    phase_us: u64,
) {
    let old_cycle_us = cycle_period.as_micros().max(1).min(u64::MAX as u128) as u64;
    let cycle_us = updated.schedule.cycle_period_us.max(1);
    if cycle_us == old_cycle_us {
        // Equal cycle length preserves phase: the cycle epoch MUST NOT
        // move. Re-deriving it from the wall clock at install time
        // would fold scheduler and dispatch latency into the grid and
        // permanently step every later cycle late. Only the event
        // cursor is recomputed inside the unchanged cycle.
        *next_event = next_event_index(&updated.schedule, phase_us % cycle_us);
    } else {
        // Real tempo change: re-anchor the epoch so the normalized
        // phase carries over into the new cycle length.
        let scaled_phase_us = scale_cycle_phase(phase_us, old_cycle_us, cycle_us);
        *cycle_period = Duration::from_micros(cycle_us);
        *epoch = now
            .checked_sub(Duration::from_micros(scaled_phase_us))
            .unwrap_or(now);
        *next_event = next_event_index(&updated.schedule, scaled_phase_us);
    }
    let acknowledgements = updated.install(schedule);

    let applied_phase_micros = Instant::now()
        .saturating_duration_since(*epoch)
        .as_micros()
        .min(u64::MAX as u128) as u64
        % cycle_us;
    let acknowledgement = schedule_update_timing(
        acknowledgements.centibpm(),
        schedule_generation,
        crate::web::start_schedule::current_epoch_micros(),
        applied_phase_micros,
        cycle_us,
        AuditionApplyMode::CurrentCycleFuture,
    );
    acknowledgements.send(acknowledgement);
}

pub(super) fn install_schedule_at_cycle_boundary(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    updated: AuditionScheduleUpdate,
    cycle_end: Instant,
) -> (AuditionUpdateAcknowledgements, u64) {
    let cycle_us = updated.schedule.cycle_period_us.max(1);
    *cycle_period = Duration::from_micros(cycle_us);
    *epoch = cycle_end;
    *next_event = 0;
    (updated.install(schedule), cycle_us)
}

pub(super) fn acknowledge_cycle_boundary_update(
    acknowledgements: AuditionUpdateAcknowledgements,
    cycle_epoch: Instant,
    cycle_period_micros: u64,
    schedule_generation: u64,
) {
    let acknowledgement = cycle_timing_at_now(
        acknowledgements.centibpm(),
        schedule_generation,
        cycle_epoch,
        cycle_period_micros,
    );
    acknowledgements.send(acknowledgement);
}

pub(crate) fn cycle_timing_at_now(
    centibpm: u32,
    schedule_generation: u64,
    cycle_epoch: Instant,
    cycle_period_micros: u64,
) -> AuditionUpdateAck {
    let before_wall_sample = Instant::now();
    let effective_at_epoch_micros = crate::web::start_schedule::current_epoch_micros();
    let after_wall_sample = Instant::now();
    let effective_instant =
        before_wall_sample + after_wall_sample.saturating_duration_since(before_wall_sample) / 2;
    let elapsed_micros = effective_instant
        .saturating_duration_since(cycle_epoch)
        .as_micros()
        .min(u64::MAX as u128) as u64;
    schedule_boundary_update_timing(
        centibpm,
        schedule_generation,
        effective_at_epoch_micros,
        elapsed_micros,
        cycle_period_micros,
    )
}

pub(crate) fn scale_cycle_phase(phase_us: u64, old_cycle_us: u64, new_cycle_us: u64) -> u64 {
    let old_cycle = old_cycle_us.max(1);
    let new_cycle = new_cycle_us.max(1);
    let old_phase = phase_us % old_cycle;
    ((old_phase as u128 * new_cycle as u128) / old_cycle as u128) as u64
}

pub(crate) fn schedule_update_timing(
    centibpm: u32,
    schedule_generation: u64,
    effective_at_epoch_micros: u64,
    phase_micros: u64,
    cycle_period_micros: u64,
    apply_mode: AuditionApplyMode,
) -> AuditionUpdateAck {
    AuditionUpdateAck {
        centibpm,
        schedule_generation,
        effective_at_epoch_micros,
        cycle_epoch_micros: effective_at_epoch_micros.saturating_sub(phase_micros),
        cycle_period_micros,
        phase_micros,
        apply_mode,
    }
}

pub(crate) fn schedule_boundary_update_timing(
    centibpm: u32,
    schedule_generation: u64,
    effective_at_epoch_micros: u64,
    elapsed_since_cycle_end_micros: u64,
    cycle_period_micros: u64,
) -> AuditionUpdateAck {
    let cycle_period_micros = cycle_period_micros.max(1);
    AuditionUpdateAck {
        centibpm,
        schedule_generation,
        effective_at_epoch_micros,
        cycle_epoch_micros: effective_at_epoch_micros
            .saturating_sub(elapsed_since_cycle_end_micros),
        cycle_period_micros,
        phase_micros: elapsed_since_cycle_end_micros % cycle_period_micros,
        apply_mode: AuditionApplyMode::NextCycle,
    }
}
