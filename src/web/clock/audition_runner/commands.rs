use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::schedule::AuditionSchedule;

const AUDITION_SPIN_THRESHOLD: Duration = Duration::from_micros(1500);

/// How an installed schedule update took effect: mid-cycle with only
/// future events replaced, or at the next cycle boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditionApplyMode {
    CurrentCycleFuture,
    NextCycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditionUpdateAck {
    pub centibpm: u32,
    pub schedule_generation: u64,
    pub effective_at_epoch_micros: u64,
    pub cycle_epoch_micros: u64,
    pub cycle_period_micros: u64,
    pub phase_micros: u64,
    pub apply_mode: AuditionApplyMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditionUpdateError {
    GenerationConflict,
    Superseded,
    AuditionStopped,
    PlaybackFailed(String),
}

pub type AuditionUpdateResult = Result<AuditionUpdateAck, AuditionUpdateError>;
pub(super) type AuditionTerminalStatus = Arc<Mutex<Option<AuditionUpdateError>>>;

pub(super) fn new_terminal_status() -> AuditionTerminalStatus {
    Arc::new(Mutex::new(None))
}

pub(super) fn publish_terminal_error(status: &AuditionTerminalStatus, error: AuditionUpdateError) {
    match status.lock() {
        Ok(mut current) => {
            if current.is_none() {
                *current = Some(error);
            }
        }
        Err(poisoned) => {
            let mut current = poisoned.into_inner();
            if current.is_none() {
                *current = Some(error);
            }
        }
    }
}

pub(super) fn terminal_error(status: &AuditionTerminalStatus) -> Option<AuditionUpdateError> {
    match status.lock() {
        Ok(current) => current.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

struct AuditionUpdateWaiter {
    sender: Option<Sender<AuditionUpdateResult>>,
    terminal_status: AuditionTerminalStatus,
}

impl AuditionUpdateWaiter {
    fn new(sender: Sender<AuditionUpdateResult>, terminal_status: AuditionTerminalStatus) -> Self {
        Self {
            sender: Some(sender),
            terminal_status,
        }
    }

    fn resolve(mut self, result: AuditionUpdateResult) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
    }
}

impl Drop for AuditionUpdateWaiter {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let error = terminal_error(&self.terminal_status)
                .unwrap_or(AuditionUpdateError::AuditionStopped);
            let _ = sender.send(Err(error));
        }
    }
}

pub(super) struct AuditionScheduleUpdate {
    pub schedule: AuditionSchedule,
    pub centibpm: u32,
    expected_schedule_generation: Option<u64>,
    acknowledgements: Vec<AuditionUpdateWaiter>,
}

pub(super) struct AuditionUpdateAcknowledgements {
    centibpm: u32,
    senders: Vec<AuditionUpdateWaiter>,
}

impl AuditionUpdateAcknowledgements {
    pub(super) fn centibpm(&self) -> u32 {
        self.centibpm
    }

    pub(super) fn send(self, acknowledgement: AuditionUpdateAck) {
        for sender in self.senders {
            sender.resolve(Ok(acknowledgement));
        }
    }

    pub(super) fn reject(self, error: AuditionUpdateError) {
        for sender in self.senders {
            sender.resolve(Err(error.clone()));
        }
    }
}

impl AuditionScheduleUpdate {
    pub(super) fn new(
        schedule: AuditionSchedule,
        centibpm: u32,
        expected_schedule_generation: Option<u64>,
        acknowledgement: Sender<AuditionUpdateResult>,
        terminal_status: AuditionTerminalStatus,
    ) -> Self {
        Self {
            schedule,
            centibpm,
            expected_schedule_generation,
            acknowledgements: vec![AuditionUpdateWaiter::new(acknowledgement, terminal_status)],
        }
    }

    pub(super) fn replace_with(&mut self, newer: Self) {
        if self.expected_schedule_generation != newer.expected_schedule_generation {
            let superseded = std::mem::replace(self, newer);
            superseded.reject(AuditionUpdateError::Superseded);
            return;
        }
        let Self {
            schedule,
            centibpm,
            expected_schedule_generation: _,
            mut acknowledgements,
        } = newer;
        self.schedule = schedule;
        self.centibpm = centibpm;
        self.acknowledgements.append(&mut acknowledgements);
    }

    pub(super) fn supersede_with(&mut self, newer: Self) {
        let superseded = std::mem::replace(self, newer);
        superseded.reject(AuditionUpdateError::Superseded);
    }

    pub(super) fn matches_generation(&self, schedule_generation: u64) -> bool {
        self.expected_schedule_generation
            .is_none_or(|expected| expected == schedule_generation)
    }

    pub(super) fn install(self, schedule: &mut AuditionSchedule) -> AuditionUpdateAcknowledgements {
        *schedule = self.schedule;
        AuditionUpdateAcknowledgements {
            centibpm: self.centibpm,
            senders: self.acknowledgements,
        }
    }

    pub(super) fn reject(self, error: AuditionUpdateError) {
        AuditionUpdateAcknowledgements {
            centibpm: self.centibpm,
            senders: self.acknowledgements,
        }
        .reject(error);
    }
}

pub(super) enum AuditionCommand {
    Update(AuditionScheduleUpdate),
    QueueNextCycle(AuditionScheduleUpdate),
    Stop,
}

pub(super) struct AuditionCommandBatch {
    pub immediate_update: Option<AuditionScheduleUpdate>,
    pub next_cycle_update: Option<AuditionScheduleUpdate>,
}

impl AuditionCommandBatch {
    fn new() -> Self {
        Self {
            immediate_update: None,
            next_cycle_update: None,
        }
    }

    fn push(&mut self, command: AuditionCommand, schedule_generation: u64) -> bool {
        match command {
            AuditionCommand::Update(update) => {
                if !update.matches_generation(schedule_generation) {
                    update.reject(AuditionUpdateError::GenerationConflict);
                    return true;
                }
                if let Some(current) = self.immediate_update.as_mut() {
                    current.replace_with(update);
                } else {
                    self.immediate_update = Some(update);
                }
                true
            }
            AuditionCommand::QueueNextCycle(update) => {
                if !update.matches_generation(schedule_generation) {
                    update.reject(AuditionUpdateError::GenerationConflict);
                    return true;
                }
                if let Some(current) = self.next_cycle_update.as_mut() {
                    current.supersede_with(update);
                } else {
                    self.next_cycle_update = Some(update);
                }
                true
            }
            AuditionCommand::Stop => false,
        }
    }
}

pub(super) enum WaitOutcome {
    Deadline,
    Commands(AuditionCommandBatch),
    Stop,
}

pub(super) fn wait_until_or_command(
    deadline: Instant,
    stop: &AtomicBool,
    command_rx: &Receiver<AuditionCommand>,
    schedule_generation: u64,
) -> WaitOutcome {
    loop {
        if stop.load(Ordering::Acquire) {
            return WaitOutcome::Stop;
        }
        match command_rx.try_recv() {
            Ok(command) => return coalesce_command(command, command_rx, schedule_generation),
            Err(TryRecvError::Disconnected) => return WaitOutcome::Stop,
            Err(TryRecvError::Empty) => {}
        }

        let now = Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            return drain_commands_now(stop, command_rx, schedule_generation);
        }

        if remaining > AUDITION_SPIN_THRESHOLD {
            let wait_deadline = deadline - AUDITION_SPIN_THRESHOLD;
            match command_rx.recv_timeout(wait_deadline.saturating_duration_since(now)) {
                Ok(command) => {
                    return coalesce_command(command, command_rx, schedule_generation);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return WaitOutcome::Stop,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
            }
        }

        while Instant::now() < deadline {
            if stop.load(Ordering::Acquire) {
                return WaitOutcome::Stop;
            }
            std::hint::spin_loop();
        }
        return drain_commands_now(stop, command_rx, schedule_generation);
    }
}

pub(super) fn drain_commands_now(
    stop: &AtomicBool,
    command_rx: &Receiver<AuditionCommand>,
    schedule_generation: u64,
) -> WaitOutcome {
    if stop.load(Ordering::Acquire) {
        return WaitOutcome::Stop;
    }
    match command_rx.try_recv() {
        Ok(command) => coalesce_command(command, command_rx, schedule_generation),
        Err(TryRecvError::Disconnected) => WaitOutcome::Stop,
        Err(TryRecvError::Empty) => WaitOutcome::Deadline,
    }
}

fn coalesce_command(
    first: AuditionCommand,
    command_rx: &Receiver<AuditionCommand>,
    schedule_generation: u64,
) -> WaitOutcome {
    let mut batch = AuditionCommandBatch::new();
    if !batch.push(first, schedule_generation) {
        return WaitOutcome::Stop;
    }
    loop {
        match command_rx.try_recv() {
            Ok(command) => {
                if !batch.push(command, schedule_generation) {
                    return WaitOutcome::Stop;
                }
            }
            Err(TryRecvError::Disconnected) => return WaitOutcome::Stop,
            Err(TryRecvError::Empty) => return WaitOutcome::Commands(batch),
        }
    }
}

#[cfg(test)]
pub(crate) fn deadline_drain_observes_queued_update(schedule: AuditionSchedule) -> bool {
    let stop = AtomicBool::new(false);
    let terminal_status = new_terminal_status();
    let (command_tx, command_rx) = mpsc::channel();
    let (acknowledgement_tx, _acknowledgement_rx) = mpsc::channel();
    if command_tx
        .send(AuditionCommand::QueueNextCycle(
            AuditionScheduleUpdate::new(
                schedule,
                12_000,
                Some(0),
                acknowledgement_tx,
                terminal_status,
            ),
        ))
        .is_err()
    {
        return false;
    }
    matches!(
        drain_commands_now(&stop, &command_rx, 0),
        WaitOutcome::Commands(AuditionCommandBatch {
            next_cycle_update: Some(_),
            ..
        })
    )
}

#[cfg(test)]
pub(crate) fn coalescing_rejects_stale_without_losing_valid(schedule: AuditionSchedule) -> bool {
    let stop = AtomicBool::new(false);
    let terminal_status = new_terminal_status();
    let (command_tx, command_rx) = mpsc::channel();
    let (valid_tx, _valid_rx) = mpsc::channel();
    let (stale_tx, stale_rx) = mpsc::channel();
    if command_tx
        .send(AuditionCommand::QueueNextCycle(
            AuditionScheduleUpdate::new(
                schedule.clone(),
                12_000,
                Some(0),
                valid_tx,
                Arc::clone(&terminal_status),
            ),
        ))
        .is_err()
        || command_tx
            .send(AuditionCommand::QueueNextCycle(
                AuditionScheduleUpdate::new(schedule, 14_000, Some(1), stale_tx, terminal_status),
            ))
            .is_err()
    {
        return false;
    }

    let retained_valid = matches!(
        drain_commands_now(&stop, &command_rx, 0),
        WaitOutcome::Commands(AuditionCommandBatch {
            next_cycle_update: Some(_),
            ..
        })
    );
    let rejected_stale = matches!(
        stale_rx.try_recv(),
        Ok(Err(AuditionUpdateError::GenerationConflict))
    );
    retained_valid && rejected_stale
}
