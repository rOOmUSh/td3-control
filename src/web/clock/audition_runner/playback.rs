use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::Td3Error;

use super::super::timing::{raise_thread_priority_time_critical, TimerPeriodGuard};
#[cfg(test)]
use super::commands::new_terminal_status;
use super::commands::{
    drain_commands_now, publish_terminal_error, wait_until_or_command, AuditionCommand,
    AuditionCommandBatch, AuditionScheduleUpdate, AuditionTerminalStatus, AuditionUpdateError,
    AuditionUpdateResult, WaitOutcome,
};
use super::midi_events::{
    flush_note_offs_through_offset, send_due_events_until_update_boundary,
    send_events_at_exact_offset, silence_all, DispatchLedger, DueEventsResult,
};
use super::schedule::AuditionSchedule;
use super::updates::{
    acknowledge_cycle_boundary_update, apply_schedule_update_at_phase, apply_schedule_update_now,
    current_cycle_phase_us, cycle_timing_at_now, install_schedule_at_cycle_boundary,
    morph_update_disposition, MorphUpdateDisposition,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn run_audition(
    out: &mut midir::MidiOutputConnection,
    mut schedule: AuditionSchedule,
    looping: bool,
    stop: Arc<AtomicBool>,
    command_rx: Receiver<AuditionCommand>,
    target_epoch_micros: u64,
    centibpm: u32,
    start_tx: Sender<AuditionUpdateResult>,
    terminal_status: AuditionTerminalStatus,
) {
    // Same priority hardening as the clock thread. Blocking waits use the
    // command channel so schedule updates wake the thread before the next
    // event deadline.
    let _timer_guard = TimerPeriodGuard::acquire();
    raise_thread_priority_time_critical();

    // Pitches currently sounding (note number). Tracked so shutdown can
    // emit an explicit Note Off for each rather than relying on a
    // sequencer Stop the device never receives.
    let mut sounding: BTreeSet<u8> = BTreeSet::new();
    // Stable event identities dispatched in the current cycle. Prevents
    // a morph schedule replacement from replaying an event whose offset
    // moved into the future. Cleared at every cycle rollover after
    // boundary Note Off handling.
    let mut dispatched = DispatchLedger::new();
    let mut cycle_period = Duration::from_micros(schedule.cycle_period_us.max(1));
    let start_delay = remaining_start_delay(
        target_epoch_micros,
        crate::web::start_schedule::current_epoch_micros(),
    );
    let mut epoch = Instant::now() + start_delay;
    let mut next_event = 0usize;
    let mut pending_update: Option<AuditionScheduleUpdate> = None;
    let mut queued_next_cycle: Option<AuditionScheduleUpdate> = None;
    let mut schedule_generation = 0u64;
    let mut playback_failure: Option<String> = None;
    let mut start_tx = Some(start_tx);

    'cycles: loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let now = Instant::now();
        if start_tx.is_some() && now >= epoch {
            match send_events_at_exact_offset(
                &schedule,
                &mut next_event,
                &mut sounding,
                &mut dispatched,
                0,
                |bytes| {
                    out.send(bytes)
                        .map_err(|err| Td3Error::Midi(format!("initial note send: {}", err)))
                },
            ) {
                Ok(()) => {
                    epoch = Instant::now();
                    if let Some(start_tx) = start_tx.take() {
                        if stop.load(Ordering::Acquire) {
                            let _ = start_tx.send(Err(AuditionUpdateError::AuditionStopped));
                            break;
                        }
                        let acknowledgement = cycle_timing_at_now(
                            centibpm,
                            0,
                            epoch,
                            cycle_period.as_micros().min(u64::MAX as u128) as u64,
                        );
                        let _ = start_tx.send(Ok(acknowledgement));
                    }
                }
                Err(err) => {
                    playback_failure = Some(err.to_string());
                    break;
                }
            }
            continue;
        }
        let cycle_end = epoch + cycle_period;
        if now >= cycle_end {
            if !looping {
                if let Err(err) = send_cycle_boundary_events(
                    &schedule,
                    cycle_period,
                    &mut next_event,
                    &mut sounding,
                    &mut dispatched,
                    &mut |bytes| {
                        out.send(bytes).map_err(|send_err| {
                            Td3Error::Midi(format!("boundary note send: {}", send_err))
                        })
                    },
                ) {
                    log::warn!("audition: boundary note send failed, stopping: {}", err);
                    playback_failure = Some(err.to_string());
                }
                break;
            }
            let rollover = rollover_cycle_with_sender(
                &mut schedule,
                &mut cycle_period,
                &mut epoch,
                &mut next_event,
                &mut sounding,
                &mut dispatched,
                &mut pending_update,
                &mut queued_next_cycle,
                &mut schedule_generation,
                cycle_end,
                &stop,
                Some(&command_rx),
                &mut |bytes| {
                    out.send(bytes).map_err(|send_err| {
                        Td3Error::Midi(format!("rollover note send: {}", send_err))
                    })
                },
            );
            match rollover {
                Ok(true) => {}
                Ok(false) => break,
                Err(err) => {
                    log::warn!("audition: cycle rollover failed, stopping: {}", err);
                    playback_failure = Some(err.to_string());
                    break;
                }
            }
            continue;
        }

        let deadline = if start_tx.is_some() {
            epoch
        } else {
            schedule
                .events
                .get(next_event)
                .map(|ev| epoch + Duration::from_micros(ev.offset_us))
                .unwrap_or(cycle_end)
        };

        match wait_until_or_command(deadline, &stop, &command_rx, schedule_generation) {
            WaitOutcome::Stop => break 'cycles,
            WaitOutcome::Commands(commands) => {
                absorb_command_batch(
                    commands,
                    schedule_generation,
                    &mut pending_update,
                    &mut queued_next_cycle,
                );
                let rollover_due = Instant::now() >= epoch + cycle_period;
                apply_pending_update_if_safe(
                    &mut schedule,
                    &mut cycle_period,
                    &mut epoch,
                    &mut next_event,
                    sounding.is_empty(),
                    &dispatched,
                    &mut pending_update,
                    schedule_generation,
                    rollover_due,
                    start_tx.is_some(),
                );
            }
            WaitOutcome::Deadline => {
                if start_tx.is_some() {
                    continue 'cycles;
                }
                if next_event >= schedule.events.len() {
                    // The deadline was the cycle end. Rollover is centralized
                    // at the top of the loop so queued transitions cannot be
                    // bypassed by an empty or already-drained schedule.
                    continue 'cycles;
                }
                let due_offset = schedule.events[next_event].offset_us;
                match process_due_events_with_sender(
                    &mut schedule,
                    &mut cycle_period,
                    &mut epoch,
                    &mut next_event,
                    &mut sounding,
                    &mut dispatched,
                    &mut pending_update,
                    &queued_next_cycle,
                    schedule_generation,
                    due_offset,
                    &mut |bytes| {
                        out.send(bytes)
                            .map_err(|e| Td3Error::Midi(format!("note send: {}", e)))
                    },
                ) {
                    Ok(true) => continue 'cycles,
                    Ok(false) => {}
                    Err(e) => {
                        log::warn!("audition: note send failed, stopping: {}", e);
                        playback_failure = Some(e.to_string());
                        break 'cycles;
                    }
                }
            }
        }
    }

    if let Some(message) = playback_failure {
        let error = AuditionUpdateError::PlaybackFailed(message);
        publish_terminal_error(&terminal_status, error.clone());
        if let Some(start_tx) = start_tx.take() {
            let _ = start_tx.send(Err(error.clone()));
        }
        reject_update(&mut pending_update, error.clone());
        reject_update(&mut queued_next_cycle, error.clone());
        while let Ok(command) = command_rx.try_recv() {
            reject_command(command, error.clone());
        }
    } else {
        let error = AuditionUpdateError::AuditionStopped;
        publish_terminal_error(&terminal_status, error.clone());
        if let Some(start_tx) = start_tx.take() {
            let _ = start_tx.send(Err(error));
        }
    }

    silence_all(out, &sounding, schedule.channel);
}

pub(crate) fn remaining_start_delay(target_epoch_micros: u64, now_epoch_micros: u64) -> Duration {
    Duration::from_micros(target_epoch_micros.saturating_sub(now_epoch_micros))
}

fn reject_command(command: AuditionCommand, error: AuditionUpdateError) {
    match command {
        AuditionCommand::Update(update) | AuditionCommand::QueueNextCycle(update) => {
            update.reject(error);
        }
        AuditionCommand::Stop => {}
    }
}

fn reject_update(update: &mut Option<AuditionScheduleUpdate>, error: AuditionUpdateError) {
    if let Some(update) = update.take() {
        update.reject(error);
    }
}

fn absorb_command_batch(
    mut commands: AuditionCommandBatch,
    schedule_generation: u64,
    pending_update: &mut Option<AuditionScheduleUpdate>,
    queued_next_cycle: &mut Option<AuditionScheduleUpdate>,
) {
    if let Some(queued) = commands.next_cycle_update.take() {
        if queued.matches_generation(schedule_generation) {
            if let Some(current) = queued_next_cycle.as_mut() {
                current.supersede_with(queued);
            } else {
                *queued_next_cycle = Some(queued);
            }
        } else {
            queued.reject(AuditionUpdateError::GenerationConflict);
        }
    }
    if let Some(updated) = commands.immediate_update.take() {
        if updated.matches_generation(schedule_generation) {
            if let Some(pending) = pending_update.as_mut() {
                pending.replace_with(updated);
            } else {
                *pending_update = Some(updated);
            }
        } else {
            updated.reject(AuditionUpdateError::GenerationConflict);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_pending_update_if_safe(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    sounding_is_empty: bool,
    dispatched: &DispatchLedger,
    pending_update: &mut Option<AuditionScheduleUpdate>,
    schedule_generation: u64,
    rollover_due: bool,
    start_pending: bool,
) {
    reject_generation_mismatch(pending_update, schedule_generation);
    if start_pending || !sounding_is_empty || rollover_due {
        return;
    }
    if let Some(updated) = pending_update.take() {
        let old_cycle_us = cycle_period.as_micros().max(1).min(u64::MAX as u128) as u64;
        let phase_us = current_cycle_phase_us(*epoch, *cycle_period);
        match morph_update_disposition(old_cycle_us, phase_us, &updated.schedule, dispatched) {
            MorphUpdateDisposition::InstallNow => apply_schedule_update_now(
                schedule,
                cycle_period,
                epoch,
                next_event,
                updated,
                schedule_generation,
            ),
            // An undispatched replacement event sits behind the current
            // phase or inside the safety margin: never fire it late and
            // never partially install. The complete latest update waits
            // for the next cycle boundary.
            MorphUpdateDisposition::DeferToBoundary => *pending_update = Some(updated),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_due_events_with_sender<F>(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
    dispatched: &mut DispatchLedger,
    pending_update: &mut Option<AuditionScheduleUpdate>,
    queued_next_cycle: &Option<AuditionScheduleUpdate>,
    schedule_generation: u64,
    due_offset: u64,
    send: &mut F,
) -> Result<bool, Td3Error>
where
    F: FnMut(&[u8]) -> Result<(), Td3Error>,
{
    reject_generation_mismatch(pending_update, schedule_generation);
    let cycle_period_us = cycle_period.as_micros().min(u64::MAX as u128) as u64;
    let queued_boundary = queued_next_cycle
        .as_ref()
        .is_some_and(|queued| queued.matches_generation(schedule_generation))
        && due_offset >= cycle_period_us;

    if queued_boundary {
        // Rollover owns every event at the old cycle boundary. It flushes
        // Note Off messages, skips stale Note On messages, installs the new
        // schedule, and only then dispatches the new offset-zero events.
        return Ok(true);
    }

    if sounding.is_empty()
        && try_apply_update_at_phase(
            schedule,
            cycle_period,
            epoch,
            next_event,
            dispatched,
            pending_update,
            schedule_generation,
            due_offset,
        )
    {
        return Ok(false);
    }

    let result = send_due_events_until_update_boundary(
        schedule,
        next_event,
        sounding,
        dispatched,
        due_offset,
        pending_update.is_some(),
        send,
    )?;
    if result == DueEventsResult::ApplyPendingUpdate {
        try_apply_update_at_phase(
            schedule,
            cycle_period,
            epoch,
            next_event,
            dispatched,
            pending_update,
            schedule_generation,
            due_offset,
        );
    }
    Ok(false)
}

/// Install the pending update at the given dispatch phase when the
/// identity-safe disposition allows it. The phase is the old schedule's
/// consumption marker: cursor placement in the replacement resumes after
/// it, exactly like the legacy path. Returns true when an update was
/// installed. A deferred update stays pending and installs at the cycle
/// boundary.
#[allow(clippy::too_many_arguments)]
fn try_apply_update_at_phase(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    dispatched: &DispatchLedger,
    pending_update: &mut Option<AuditionScheduleUpdate>,
    schedule_generation: u64,
    phase_us: u64,
) -> bool {
    let Some(updated) = pending_update.take() else {
        return false;
    };
    let old_cycle_us = cycle_period.as_micros().max(1).min(u64::MAX as u128) as u64;
    match morph_update_disposition(old_cycle_us, phase_us, &updated.schedule, dispatched) {
        MorphUpdateDisposition::InstallNow => {
            apply_schedule_update_at_phase(
                schedule,
                cycle_period,
                epoch,
                next_event,
                updated,
                schedule_generation,
                Instant::now(),
                phase_us,
            );
            true
        }
        MorphUpdateDisposition::DeferToBoundary => {
            *pending_update = Some(updated);
            false
        }
    }
}

fn reject_generation_mismatch(
    update: &mut Option<AuditionScheduleUpdate>,
    schedule_generation: u64,
) {
    if update
        .as_ref()
        .is_some_and(|update| !update.matches_generation(schedule_generation))
    {
        reject_update(update, AuditionUpdateError::GenerationConflict);
    }
}

fn send_cycle_boundary_events<F>(
    schedule: &AuditionSchedule,
    cycle_period: Duration,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
    dispatched: &mut DispatchLedger,
    send: &mut F,
) -> Result<(), Td3Error>
where
    F: FnMut(&[u8]) -> Result<(), Td3Error>,
{
    let boundary_offset = cycle_period.as_micros().min(u64::MAX as u128) as u64;
    flush_note_offs_through_offset(
        schedule,
        next_event,
        sounding,
        dispatched,
        boundary_offset,
        send,
    )
}

#[allow(clippy::too_many_arguments)]
fn rollover_cycle_with_sender<F>(
    schedule: &mut AuditionSchedule,
    cycle_period: &mut Duration,
    epoch: &mut Instant,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
    dispatched: &mut DispatchLedger,
    pending_update: &mut Option<AuditionScheduleUpdate>,
    queued_next_cycle: &mut Option<AuditionScheduleUpdate>,
    schedule_generation: &mut u64,
    cycle_end: Instant,
    stop: &AtomicBool,
    command_rx: Option<&Receiver<AuditionCommand>>,
    send: &mut F,
) -> Result<bool, Td3Error>
where
    F: FnMut(&[u8]) -> Result<(), Td3Error>,
{
    send_cycle_boundary_events(
        schedule,
        *cycle_period,
        next_event,
        sounding,
        dispatched,
        send,
    )?;
    // The old cycle's boundary Note Off handling is complete: the new
    // cycle starts with a fresh dispatch ledger.
    dispatched.clear();
    if let Some(command_rx) = command_rx {
        match drain_commands_now(stop, command_rx, *schedule_generation) {
            WaitOutcome::Stop => return Ok(false),
            WaitOutcome::Commands(commands) => absorb_command_batch(
                commands,
                *schedule_generation,
                pending_update,
                queued_next_cycle,
            ),
            WaitOutcome::Deadline => {}
        }
    }
    if stop.load(Ordering::Acquire) {
        return Ok(false);
    }

    reject_generation_mismatch(queued_next_cycle, *schedule_generation);
    if queued_next_cycle.is_some() {
        // A queued transition owns this boundary. An immediate update that
        // could not reach a note-safe point in the old cycle must fail rather
        // than report success for a schedule that is immediately replaced.
        reject_update(pending_update, AuditionUpdateError::Superseded);
        if !sounding.is_empty() {
            return Err(Td3Error::Midi(
                "audition cycle ended with a sounding note".to_string(),
            ));
        }

        if let Some(queued) = queued_next_cycle.take() {
            let (acknowledgements, queued_cycle_us) = install_schedule_at_cycle_boundary(
                schedule,
                cycle_period,
                epoch,
                next_event,
                queued,
                cycle_end,
            );
            *schedule_generation = schedule_generation.saturating_add(1);
            let install_now = Instant::now();
            if install_now.saturating_duration_since(*epoch) >= *cycle_period {
                *epoch = install_now;
            }
            if stop.load(Ordering::Acquire) {
                acknowledgements.reject(AuditionUpdateError::AuditionStopped);
                return Ok(false);
            }
            if let Err(err) = send_events_at_exact_offset(
                schedule, next_event, sounding, dispatched, 0, &mut *send,
            ) {
                acknowledgements.reject(AuditionUpdateError::PlaybackFailed(err.to_string()));
                return Err(err);
            }
            if stop.load(Ordering::Acquire) {
                acknowledgements.reject(AuditionUpdateError::AuditionStopped);
                return Ok(false);
            }
            acknowledge_cycle_boundary_update(
                acknowledgements,
                *epoch,
                queued_cycle_us,
                *schedule_generation,
            );
        }
        return Ok(true);
    }

    reject_generation_mismatch(pending_update, *schedule_generation);
    let mut boundary_acknowledgements = None;
    if sounding.is_empty() {
        if let Some(updated) = pending_update.take() {
            boundary_acknowledgements = Some(install_schedule_at_cycle_boundary(
                schedule,
                cycle_period,
                epoch,
                next_event,
                updated,
                cycle_end,
            ));
        }
    }

    if boundary_acknowledgements.is_none() {
        *epoch = cycle_end;
        *next_event = 0;
    }
    let late = Instant::now().saturating_duration_since(*epoch);
    if late >= *cycle_period {
        *epoch = Instant::now();
    }
    if stop.load(Ordering::Acquire) {
        if let Some((acknowledgements, _)) = boundary_acknowledgements {
            acknowledgements.reject(AuditionUpdateError::AuditionStopped);
        }
        return Ok(false);
    }
    if let Err(err) =
        send_events_at_exact_offset(schedule, next_event, sounding, dispatched, 0, send)
    {
        if let Some((acknowledgements, _)) = boundary_acknowledgements {
            acknowledgements.reject(AuditionUpdateError::PlaybackFailed(err.to_string()));
        }
        return Err(err);
    }
    if stop.load(Ordering::Acquire) {
        if let Some((acknowledgements, _)) = boundary_acknowledgements {
            acknowledgements.reject(AuditionUpdateError::AuditionStopped);
        }
        return Ok(false);
    }
    if let Some((acknowledgements, cycle_period_micros)) = boundary_acknowledgements {
        acknowledge_cycle_boundary_update(
            acknowledgements,
            *epoch,
            cycle_period_micros,
            *schedule_generation,
        );
    }
    Ok(true)
}

#[cfg(test)]
pub(crate) struct AuditionTransitionTestHarness {
    schedule: AuditionSchedule,
    cycle_period: Duration,
    epoch: Instant,
    next_event: usize,
    sounding: BTreeSet<u8>,
    dispatched: DispatchLedger,
    pending_update: Option<AuditionScheduleUpdate>,
    queued_next_cycle: Option<AuditionScheduleUpdate>,
    schedule_generation: u64,
    terminal_status: AuditionTerminalStatus,
    sent: Vec<Vec<u8>>,
    stop: AtomicBool,
}

#[cfg(test)]
impl AuditionTransitionTestHarness {
    pub(crate) fn new(schedule: AuditionSchedule) -> Self {
        let cycle_period = Duration::from_micros(schedule.cycle_period_us.max(1));
        let now = Instant::now();
        let epoch = now.checked_sub(cycle_period).unwrap_or(now);
        Self {
            schedule,
            cycle_period,
            epoch,
            next_event: 0,
            sounding: BTreeSet::new(),
            dispatched: DispatchLedger::new(),
            pending_update: None,
            queued_next_cycle: None,
            schedule_generation: 0,
            terminal_status: new_terminal_status(),
            sent: Vec::new(),
            stop: AtomicBool::new(false),
        }
    }

    pub(crate) fn dispatch_through(&mut self, offset_us: u64) -> Result<(), Td3Error> {
        let sent = &mut self.sent;
        send_due_events_until_update_boundary(
            &self.schedule,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            offset_us,
            false,
            |bytes| {
                sent.push(bytes.to_vec());
                Ok(())
            },
        )?;
        Ok(())
    }

    pub(crate) fn dispatch_deadline_and_rollover(
        &mut self,
        due_offset: u64,
    ) -> Result<(), Td3Error> {
        let sent = &mut self.sent;
        let defer_to_rollover = process_due_events_with_sender(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            &mut self.pending_update,
            &self.queued_next_cycle,
            self.schedule_generation,
            due_offset,
            &mut |bytes| {
                sent.push(bytes.to_vec());
                Ok(())
            },
        )?;
        if defer_to_rollover {
            self.rollover()?;
        }
        Ok(())
    }

    pub(crate) fn process_commands_at_due_cycle_boundary(&mut self) {
        apply_pending_update_if_safe(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            self.sounding.is_empty(),
            &self.dispatched,
            &mut self.pending_update,
            self.schedule_generation,
            true,
            false,
        );
    }

    pub(crate) fn process_commands_before_scheduled_start(&mut self) {
        apply_pending_update_if_safe(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            self.sounding.is_empty(),
            &self.dispatched,
            &mut self.pending_update,
            self.schedule_generation,
            false,
            true,
        );
    }

    /// Re-anchor the harness epoch so the true current phase equals
    /// `phase_us`.
    pub(crate) fn set_phase(&mut self, phase_us: u64) {
        let now = Instant::now();
        self.epoch = now
            .checked_sub(Duration::from_micros(phase_us))
            .unwrap_or(now);
    }

    /// Apply the pending update at an explicit phase, mirroring the
    /// runner's mid-cycle installation path. Returns true when the
    /// update installed; false when it stayed pending (deferred).
    pub(crate) fn try_apply_pending_at_phase(&mut self, phase_us: u64) -> bool {
        try_apply_update_at_phase(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &self.dispatched,
            &mut self.pending_update,
            self.schedule_generation,
            phase_us,
        )
    }

    pub(crate) fn cycle_epoch(&self) -> Instant {
        self.epoch
    }

    pub(crate) fn pending_update_is_deferred(&self) -> bool {
        self.pending_update.is_some()
    }

    pub(crate) fn queue_next_cycle(
        &mut self,
        schedule: AuditionSchedule,
        centibpm: u32,
    ) -> Receiver<super::commands::AuditionUpdateResult> {
        self.queue_next_cycle_expected(schedule, centibpm, Some(self.schedule_generation))
    }

    pub(crate) fn queue_next_cycle_expected(
        &mut self,
        schedule: AuditionSchedule,
        centibpm: u32,
        expected_schedule_generation: Option<u64>,
    ) -> Receiver<super::commands::AuditionUpdateResult> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let update = AuditionScheduleUpdate::new(
            schedule,
            centibpm,
            expected_schedule_generation,
            sender,
            Arc::clone(&self.terminal_status),
        );
        if update.matches_generation(self.schedule_generation) {
            if let Some(current) = self.queued_next_cycle.as_mut() {
                current.supersede_with(update);
            } else {
                self.queued_next_cycle = Some(update);
            }
        } else {
            update.reject(AuditionUpdateError::GenerationConflict);
        }
        receiver
    }

    pub(crate) fn queue_immediate_update(
        &mut self,
        schedule: AuditionSchedule,
        centibpm: u32,
    ) -> Receiver<super::commands::AuditionUpdateResult> {
        self.queue_immediate_update_expected(schedule, centibpm, Some(self.schedule_generation))
    }

    pub(crate) fn queue_immediate_update_expected(
        &mut self,
        schedule: AuditionSchedule,
        centibpm: u32,
        expected_schedule_generation: Option<u64>,
    ) -> Receiver<super::commands::AuditionUpdateResult> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let update = AuditionScheduleUpdate::new(
            schedule,
            centibpm,
            expected_schedule_generation,
            sender,
            Arc::clone(&self.terminal_status),
        );
        if update.matches_generation(self.schedule_generation) {
            if let Some(current) = self.pending_update.as_mut() {
                current.replace_with(update);
            } else {
                self.pending_update = Some(update);
            }
        } else {
            update.reject(AuditionUpdateError::GenerationConflict);
        }
        receiver
    }

    pub(crate) fn rollover(&mut self) -> Result<(), Td3Error> {
        let cycle_end = self.epoch + self.cycle_period;
        let sent = &mut self.sent;
        let continued = rollover_cycle_with_sender(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            &mut self.pending_update,
            &mut self.queued_next_cycle,
            &mut self.schedule_generation,
            cycle_end,
            &self.stop,
            None,
            &mut |bytes| {
                sent.push(bytes.to_vec());
                Ok(())
            },
        )?;
        if !continued {
            self.pending_update = None;
            self.queued_next_cycle = None;
        }
        Ok(())
    }

    pub(crate) fn rollover_queuing_during_boundary_event(
        &mut self,
        schedule: AuditionSchedule,
        centibpm: u32,
    ) -> Result<Receiver<super::commands::AuditionUpdateResult>, Td3Error> {
        let cycle_end = self.epoch + self.cycle_period;
        let sent = &mut self.sent;
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (acknowledgement_tx, acknowledgement_rx) = std::sync::mpsc::channel();
        let update = AuditionScheduleUpdate::new(
            schedule,
            centibpm,
            Some(self.schedule_generation),
            acknowledgement_tx,
            Arc::clone(&self.terminal_status),
        );
        let mut boundary_command = Some(AuditionCommand::QueueNextCycle(update));
        let continued = rollover_cycle_with_sender(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            &mut self.pending_update,
            &mut self.queued_next_cycle,
            &mut self.schedule_generation,
            cycle_end,
            &self.stop,
            Some(&command_rx),
            &mut |bytes| {
                sent.push(bytes.to_vec());
                if let Some(command) = boundary_command.take() {
                    let _ = command_tx.send(command);
                }
                Ok(())
            },
        )?;
        if !continued {
            self.pending_update = None;
            self.queued_next_cycle = None;
        }
        Ok(acknowledgement_rx)
    }

    pub(crate) fn rollover_failing_offset_zero(&mut self) -> Result<(), Td3Error> {
        let cycle_end = self.epoch + self.cycle_period;
        let sent = &mut self.sent;
        let continued = rollover_cycle_with_sender(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            &mut self.pending_update,
            &mut self.queued_next_cycle,
            &mut self.schedule_generation,
            cycle_end,
            &self.stop,
            None,
            &mut |bytes| {
                sent.push(bytes.to_vec());
                if matches!(bytes.first().map(|status| status & 0xF0), Some(0x90))
                    && bytes.get(2).copied().unwrap_or(0) > 0
                {
                    return Err(Td3Error::Midi("test offset-zero send failure".to_string()));
                }
                Ok(())
            },
        )?;
        if !continued {
            self.pending_update = None;
            self.queued_next_cycle = None;
        }
        Ok(())
    }

    pub(crate) fn rollover_stopping_after_boundary_event(&mut self) -> Result<(), Td3Error> {
        let cycle_end = self.epoch + self.cycle_period;
        let sent = &mut self.sent;
        let stop = &self.stop;
        let continued = rollover_cycle_with_sender(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            &mut self.pending_update,
            &mut self.queued_next_cycle,
            &mut self.schedule_generation,
            cycle_end,
            stop,
            None,
            &mut |bytes| {
                sent.push(bytes.to_vec());
                stop.store(true, Ordering::Release);
                Ok(())
            },
        )?;
        if !continued {
            self.pending_update = None;
            self.queued_next_cycle = None;
        }
        Ok(())
    }

    pub(crate) fn rollover_stopping_after_offset_zero(&mut self) -> Result<(), Td3Error> {
        let cycle_end = self.epoch + self.cycle_period;
        let sent = &mut self.sent;
        let stop = &self.stop;
        let continued = rollover_cycle_with_sender(
            &mut self.schedule,
            &mut self.cycle_period,
            &mut self.epoch,
            &mut self.next_event,
            &mut self.sounding,
            &mut self.dispatched,
            &mut self.pending_update,
            &mut self.queued_next_cycle,
            &mut self.schedule_generation,
            cycle_end,
            stop,
            None,
            &mut |bytes| {
                sent.push(bytes.to_vec());
                if matches!(bytes.first().map(|status| status & 0xF0), Some(0x90))
                    && bytes.get(2).copied().unwrap_or(0) > 0
                {
                    stop.store(true, Ordering::Release);
                }
                Ok(())
            },
        )?;
        if !continued {
            self.pending_update = None;
            self.queued_next_cycle = None;
        }
        Ok(())
    }

    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.pending_update = None;
        self.queued_next_cycle = None;
    }

    pub(crate) fn sent(&self) -> &[Vec<u8>] {
        &self.sent
    }

    pub(crate) fn clear_sent(&mut self) {
        self.sent.clear();
    }

    pub(crate) fn cycle_period_us(&self) -> u64 {
        self.cycle_period.as_micros().min(u64::MAX as u128) as u64
    }

    pub(crate) fn schedule_generation(&self) -> u64 {
        self.schedule_generation
    }

    pub(crate) fn make_rollover_late_by(&mut self, lateness: Duration) {
        let elapsed = self.cycle_period.saturating_add(lateness);
        let now = Instant::now();
        self.epoch = now.checked_sub(elapsed).unwrap_or(now);
    }

    pub(crate) fn cycle_is_due(&self) -> bool {
        Instant::now() >= self.epoch + self.cycle_period
    }
}
