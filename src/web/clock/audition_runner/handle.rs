use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::error::Td3Error;

use super::commands::AuditionCommand;
use super::playback::run_audition;
use super::schedule::AuditionSchedule;

/// Handle to a running audition thread. Call [`stop`](Self::stop) (or
/// drop) to shut it down; the thread silences any sounding notes and
/// returns its `MidiOutputConnection` so the caller can put it back in
/// the session.
pub struct AuditionRunner {
    stop: Arc<AtomicBool>,
    command_tx: Sender<AuditionCommand>,
    thread: Option<JoinHandle<midir::MidiOutputConnection>>,
}

impl AuditionRunner {
    /// Spawn the audition thread and arm its first cycle after
    /// `start_delay`. A zero delay starts immediately.
    pub fn spawn_scheduled(
        out_conn: midir::MidiOutputConnection,
        schedule: AuditionSchedule,
        looping: bool,
        start_delay: Duration,
    ) -> Result<Self, Td3Error> {
        let stop = Arc::new(AtomicBool::new(false));
        let (command_tx, command_rx) = mpsc::channel::<AuditionCommand>();
        let thread = {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("td3-midi-audition".into())
                .spawn(move || {
                    let mut out = out_conn;
                    run_audition(&mut out, schedule, looping, stop, command_rx, start_delay);
                    out
                })
                .map_err(|e| {
                    Td3Error::Midi(format!("failed to spawn MIDI audition thread: {}", e))
                })?
        };

        Ok(Self {
            stop,
            command_tx,
            thread: Some(thread),
        })
    }

    /// Replace the running note schedule without restarting playback.
    /// The audition thread keeps its current cycle phase and applies the
    /// new events from the next not-yet-reached event offset.
    pub fn update_schedule(&self, schedule: AuditionSchedule) -> Result<(), Td3Error> {
        self.command_tx
            .send(AuditionCommand::Update(schedule))
            .map_err(|_| Td3Error::Midi("audition thread update queue closed".into()))
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
