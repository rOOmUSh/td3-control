use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io::SysexSender;

use super::commands::{
    new_terminal_status, terminal_error, AuditionCommand, AuditionScheduleUpdate,
    AuditionTerminalStatus, AuditionUpdateError, AuditionUpdateResult, RawSend,
};

/// Upper bound on waiting for the audition thread to write a raw message.
/// The thread services commands between scheduled events, so a healthy
/// thread answers within a few milliseconds; the bound only guards a
/// thread that is wedged inside a MIDI driver call.
const RAW_SEND_TIMEOUT: Duration = Duration::from_secs(3);
use super::playback::run_audition;
use super::schedule::AuditionSchedule;

/// Handle to a running audition thread. Call [`stop`](Self::stop) (or
/// drop) to shut it down; the thread silences any sounding notes and
/// returns its `MidiOutputConnection` so the caller can put it back in
/// the session.
pub struct AuditionRunner {
    stop: Arc<AtomicBool>,
    command_tx: Sender<AuditionCommand>,
    terminal_status: AuditionTerminalStatus,
    thread: Option<JoinHandle<midir::MidiOutputConnection>>,
}

impl AuditionRunner {
    /// Spawn the audition thread and arm its first cycle for the absolute
    /// target epoch. The thread resolves the remaining delay after it starts.
    pub fn spawn_scheduled(
        out_conn: midir::MidiOutputConnection,
        schedule: AuditionSchedule,
        looping: bool,
        target_epoch_micros: u64,
        centibpm: u32,
    ) -> Result<(Self, Receiver<AuditionUpdateResult>), Td3Error> {
        let stop = Arc::new(AtomicBool::new(false));
        let (command_tx, command_rx) = mpsc::channel::<AuditionCommand>();
        let (start_tx, start_rx) = mpsc::channel();
        let terminal_status = new_terminal_status();
        let thread = {
            let stop = Arc::clone(&stop);
            let terminal_status = Arc::clone(&terminal_status);
            thread::Builder::new()
                .name("td3-midi-audition".into())
                .spawn(move || {
                    let mut out = out_conn;
                    run_audition(
                        &mut out,
                        schedule,
                        looping,
                        stop,
                        command_rx,
                        target_epoch_micros,
                        centibpm,
                        start_tx,
                        terminal_status,
                    );
                    out
                })
                .map_err(|e| {
                    Td3Error::Midi(format!("failed to spawn MIDI audition thread: {}", e))
                })?
        };

        Ok((
            Self {
                stop,
                command_tx,
                terminal_status,
                thread: Some(thread),
            },
            start_rx,
        ))
    }

    /// Replace the running note schedule without restarting playback.
    /// The audition thread keeps its current cycle phase and applies the
    /// new events from the next not-yet-reached event offset. The returned
    /// receiver resolves after the schedule is installed at a note-safe
    /// boundary and reports its effective cycle timing.
    pub fn update_schedule(
        &self,
        schedule: AuditionSchedule,
        centibpm: u32,
        expected_schedule_generation: Option<u64>,
    ) -> Result<Receiver<AuditionUpdateResult>, Td3Error> {
        let (acknowledgement_tx, acknowledgement_rx) = mpsc::channel();
        let update = AuditionScheduleUpdate::new(
            schedule,
            centibpm,
            expected_schedule_generation,
            acknowledgement_tx,
            Arc::clone(&self.terminal_status),
        );
        if let Err(unsent) = self.command_tx.send(AuditionCommand::Update(update)) {
            reject_unsent_command(unsent.0, &self.terminal_status);
        }
        Ok(acknowledgement_rx)
    }

    /// Queue a schedule replacement for the next cycle rollover. The old
    /// schedule remains active through its boundary events. The returned
    /// receiver resolves after the new schedule is installed and its
    /// offset-zero events are sent.
    pub fn queue_next_cycle(
        &self,
        schedule: AuditionSchedule,
        centibpm: u32,
        expected_schedule_generation: Option<u64>,
    ) -> Result<Receiver<AuditionUpdateResult>, Td3Error> {
        let (acknowledgement_tx, acknowledgement_rx) = mpsc::channel();
        let update = AuditionScheduleUpdate::new(
            schedule,
            centibpm,
            expected_schedule_generation,
            acknowledgement_tx,
            Arc::clone(&self.terminal_status),
        );
        if let Err(unsent) = self
            .command_tx
            .send(AuditionCommand::QueueNextCycle(update))
        {
            reject_unsent_command(unsent.0, &self.terminal_status);
        }
        Ok(acknowledgement_rx)
    }

    /// Write one channel-voice message through the audition thread and
    /// wait for the write result. Returns an error when the thread has
    /// stopped, when the driver rejects the write, or when no reply
    /// arrives within `RAW_SEND_TIMEOUT`.
    pub fn send_raw(&self, bytes: &[u8]) -> Result<(), Td3Error> {
        let (done_tx, done_rx) = mpsc::channel();
        let raw = RawSend {
            bytes: bytes.to_vec(),
            done: done_tx,
        };
        if let Err(unsent) = self.command_tx.send(AuditionCommand::SendBytes(raw)) {
            reject_unsent_command(unsent.0, &self.terminal_status);
        }
        match done_rx.recv_timeout(RAW_SEND_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(Td3Error::Timeout {
                operation: "audition raw send".to_owned(),
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Td3Error::Midi(
                "audition thread dropped the send before replying".into(),
            )),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    /// Signal the thread to stop and wait for it to exit. The thread
    /// silences sounding notes before returning the
    /// `MidiOutputConnection`. Returns `None` only if the thread
    /// panicked (it cannot in normal operation).
    pub fn stop(mut self) -> Option<midir::MidiOutputConnection> {
        self.stop.store(true, Ordering::Release);
        let _ = self.command_tx.send(AuditionCommand::Stop);
        self.thread.take().and_then(|t| t.join().ok())
    }

    pub(crate) fn stop_detached(mut self, cleanup_pending: Arc<AtomicUsize>) {
        self.stop.store(true, Ordering::Release);
        let _ = self.command_tx.send(AuditionCommand::Stop);
        let Some(audition_thread) = self.thread.take() else {
            return;
        };
        cleanup_pending.fetch_add(1, Ordering::AcqRel);
        let holder = Arc::new(Mutex::new(Some(audition_thread)));
        let worker_holder = Arc::clone(&holder);
        let cleanup_flag = Arc::clone(&cleanup_pending);
        let spawned = thread::Builder::new()
            .name("td3-midi-audition-cleanup".to_string())
            .spawn(move || {
                let audition_thread = worker_holder
                    .lock()
                    .ok()
                    .and_then(|mut holder| holder.take());
                let Some(audition_thread) = audition_thread else {
                    log::error!(
                        "audition cleanup lost its join handle; MIDI reconnect remains blocked"
                    );
                    return;
                };
                let _ = audition_thread.join();
                cleanup_flag.fetch_sub(1, Ordering::AcqRel);
            });
        if spawned.is_err() {
            std::mem::forget(holder);
            log::error!(
                "audition cleanup thread could not start; MIDI reconnect remains blocked for safety"
            );
        }
    }
}

fn reject_unsent_command(command: AuditionCommand, status: &AuditionTerminalStatus) {
    let error = terminal_error(status).unwrap_or(AuditionUpdateError::AuditionStopped);
    match command {
        AuditionCommand::Update(update) | AuditionCommand::QueueNextCycle(update) => {
            update.reject(error);
        }
        AuditionCommand::SendBytes(raw) => raw.reject("audition has stopped"),
        AuditionCommand::Stop => {}
    }
}

impl SysexSender for AuditionRunner {
    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), Td3Error> {
        self.send_raw(bytes)
    }
}

/// Exercises the rejection path a raw send takes when the audition
/// thread has already gone: the requester must receive an error, never
/// hang on the reply.
#[cfg(test)]
pub(crate) fn reject_closed_raw_send_for_test() -> Result<(), Td3Error> {
    let status = new_terminal_status();
    let (done_tx, done_rx) = mpsc::channel();
    let raw = RawSend {
        bytes: vec![0xB0, 0x4A, 0x40],
        done: done_tx,
    };
    reject_unsent_command(AuditionCommand::SendBytes(raw), &status);
    done_rx
        .recv_timeout(Duration::from_millis(100))
        .unwrap_or(Ok(()))
}

#[cfg(test)]
pub(crate) fn reject_closed_command_for_test(
    schedule: AuditionSchedule,
    error: AuditionUpdateError,
) -> AuditionUpdateResult {
    let status = new_terminal_status();
    super::commands::publish_terminal_error(&status, error);
    let (acknowledgement_tx, acknowledgement_rx) = mpsc::channel();
    let update = AuditionScheduleUpdate::new(
        schedule,
        12_000,
        Some(0),
        acknowledgement_tx,
        Arc::clone(&status),
    );
    reject_unsent_command(AuditionCommand::Update(update), &status);
    acknowledgement_rx
        .recv()
        .unwrap_or(Err(AuditionUpdateError::AuditionStopped))
}

impl Drop for AuditionRunner {
    fn drop(&mut self) {
        // Defensive: if dropped without `stop()` (e.g. a panic unwinds
        // past the handler), still signal and join so the OS thread and
        // MIDI port are released.
        self.stop.store(true, Ordering::Release);
        let _ = self.command_tx.send(AuditionCommand::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
