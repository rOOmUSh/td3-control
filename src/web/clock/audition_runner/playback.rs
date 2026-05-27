use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::Td3Error;

use super::super::timing::{raise_thread_priority_time_critical, TimerPeriodGuard};
use super::commands::{wait_until_or_command, AuditionCommand, WaitOutcome};
use super::midi_events::{send_due_events_until_update_boundary, silence_all, DueEventsResult};
use super::schedule::AuditionSchedule;
use super::updates::{apply_schedule_update_at_phase, apply_schedule_update_now};

pub(super) fn run_audition(
    out: &mut midir::MidiOutputConnection,
    mut schedule: AuditionSchedule,
    looping: bool,
    stop: Arc<AtomicBool>,
    command_rx: Receiver<AuditionCommand>,
    start_delay: Duration,
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
    let mut cycle_period = Duration::from_micros(schedule.cycle_period_us.max(1));
    let mut epoch = Instant::now() + start_delay;
    let mut next_event = 0usize;
    let mut pending_update: Option<AuditionSchedule> = None;

    'cycles: loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let now = Instant::now();
        let cycle_end = epoch + cycle_period;
        if now >= cycle_end {
            if !looping {
                break;
            }
            if sounding.is_empty() {
                let Some(updated) = pending_update.take() else {
                    epoch = cycle_end;
                    let late = Instant::now().saturating_duration_since(epoch);
                    if late > cycle_period {
                        epoch = Instant::now();
                    }
                    next_event = 0;
                    continue;
                };
                apply_schedule_update_at_phase(
                    &mut schedule,
                    &mut cycle_period,
                    &mut epoch,
                    &mut next_event,
                    updated,
                    now,
                    0,
                );
                continue;
            }
            epoch = cycle_end;
            let late = Instant::now().saturating_duration_since(epoch);
            if late > cycle_period {
                epoch = Instant::now();
            }
            next_event = 0;
            continue;
        }

        let deadline = schedule
            .events
            .get(next_event)
            .map(|ev| epoch + Duration::from_micros(ev.offset_us))
            .unwrap_or(cycle_end);

        match wait_until_or_command(deadline, &stop, &command_rx) {
            WaitOutcome::Stop => break 'cycles,
            WaitOutcome::Update(updated) => {
                pending_update = Some(updated);
                if sounding.is_empty() {
                    if let Some(updated) = pending_update.take() {
                        apply_schedule_update_now(
                            &mut schedule,
                            &mut cycle_period,
                            &mut epoch,
                            &mut next_event,
                            updated,
                        );
                    }
                }
            }
            WaitOutcome::Deadline => {
                if next_event >= schedule.events.len() {
                    if sounding.is_empty() {
                        if let Some(updated) = pending_update.take() {
                            apply_schedule_update_now(
                                &mut schedule,
                                &mut cycle_period,
                                &mut epoch,
                                &mut next_event,
                                updated,
                            );
                            continue;
                        }
                    }
                    if !looping {
                        break;
                    }
                    epoch = cycle_end;
                    next_event = 0;
                    let late = Instant::now().saturating_duration_since(epoch);
                    if late > cycle_period {
                        epoch = Instant::now();
                    }
                    continue;
                }
                let due_offset = schedule.events[next_event].offset_us;
                if sounding.is_empty() {
                    if let Some(updated) = pending_update.take() {
                        apply_schedule_update_at_phase(
                            &mut schedule,
                            &mut cycle_period,
                            &mut epoch,
                            &mut next_event,
                            updated,
                            Instant::now(),
                            due_offset,
                        );
                        continue;
                    }
                }
                let pending = pending_update.is_some();
                match send_due_events_until_update_boundary(
                    &schedule,
                    &mut next_event,
                    &mut sounding,
                    due_offset,
                    pending,
                    |bytes| {
                        out.send(bytes)
                            .map_err(|e| Td3Error::Midi(format!("note send: {}", e)))
                    },
                ) {
                    Ok(DueEventsResult::Complete) => {}
                    Ok(DueEventsResult::ApplyPendingUpdate) => {
                        if let Some(updated) = pending_update.take() {
                            apply_schedule_update_at_phase(
                                &mut schedule,
                                &mut cycle_period,
                                &mut epoch,
                                &mut next_event,
                                updated,
                                Instant::now(),
                                due_offset,
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!("audition: note send failed, stopping: {}", e);
                        break 'cycles;
                    }
                }
            }
        }
    }

    silence_all(out, &sounding);
}
