use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use super::schedule::AuditionSchedule;

const AUDITION_SPIN_THRESHOLD: Duration = Duration::from_micros(1500);

pub(super) enum AuditionCommand {
    Update(AuditionSchedule),
    Stop,
}

pub(super) enum WaitOutcome {
    Deadline,
    Update(AuditionSchedule),
    Stop,
}

pub(super) fn wait_until_or_command(
    deadline: Instant,
    stop: &AtomicBool,
    command_rx: &Receiver<AuditionCommand>,
) -> WaitOutcome {
    loop {
        if stop.load(Ordering::Acquire) {
            return WaitOutcome::Stop;
        }
        match command_rx.try_recv() {
            Ok(command) => return coalesce_command(command, command_rx),
            Err(TryRecvError::Disconnected) => return WaitOutcome::Stop,
            Err(TryRecvError::Empty) => {}
        }

        let now = Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            return WaitOutcome::Deadline;
        }

        if remaining > AUDITION_SPIN_THRESHOLD {
            let wait_deadline = deadline - AUDITION_SPIN_THRESHOLD;
            match command_rx.recv_timeout(wait_deadline.saturating_duration_since(now)) {
                Ok(command) => return coalesce_command(command, command_rx),
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
        return WaitOutcome::Deadline;
    }
}

fn coalesce_command(first: AuditionCommand, command_rx: &Receiver<AuditionCommand>) -> WaitOutcome {
    let mut latest = match first {
        AuditionCommand::Stop => return WaitOutcome::Stop,
        AuditionCommand::Update(schedule) => schedule,
    };
    loop {
        match command_rx.try_recv() {
            Ok(AuditionCommand::Stop) => return WaitOutcome::Stop,
            Ok(AuditionCommand::Update(schedule)) => latest = schedule,
            Err(TryRecvError::Disconnected) => return WaitOutcome::Stop,
            Err(TryRecvError::Empty) => return WaitOutcome::Update(latest),
        }
    }
}
