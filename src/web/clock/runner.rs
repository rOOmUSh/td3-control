use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::Td3Error;
use crate::midi_io::SysexSender;

#[cfg(windows)]
use super::timing::WaitableTimer;
use super::timing::{raise_thread_priority_time_critical, sleep_until, TimerPeriodGuard};

const MIDI_CLOCK: u8 = 0xF8;
/// MIDI Start byte.
const MIDI_START: u8 = 0xFA;
/// MIDI Stop byte.
const MIDI_STOP: u8 = 0xFC;

/// Pulses per quarter note for MIDI clock.
pub const PPQN: u32 = 24;

/// How long to wait for the clock thread to forward a queued SysEx
/// before giving up. Picked well above the worst-case per-tick drain
/// latency (a few ms) so normal operation always completes, but tight
/// enough that a stuck clock thread surfaces quickly as a timeout
/// rather than hanging the handler.
const QUEUE_SEND_TIMEOUT: Duration = Duration::from_secs(3);

/// Safety margin left between the end of a drain batch and the next
/// scheduled tick. If the margin is smaller than this the drain bails
/// out so the next 0xF8 stays on schedule. One Windows timer quantum
/// (~1 ms) plus slack for the sleep wakeup and the tick send itself.
const DRAIN_SAFETY_MARGIN: Duration = Duration::from_millis(2);

/// Extra time allowed for the clock thread to wake at a scheduled target and
/// complete the MIDI Start write. The requested delay is added separately.
const START_ACK_GRACE: Duration = Duration::from_secs(3);

/// Bound shutdown latency while a clock thread is waiting for a scheduled
/// start. The final two milliseconds still use the precision timing path.
const SCHEDULE_STOP_POLL_INTERVAL: Duration = Duration::from_millis(10);
const SCHEDULE_PRECISION_WINDOW: Duration = Duration::from_millis(2);

/// Successful pulse snapshots retained so a wrap waiter can recover the
/// exact boundary timestamp even if the HTTP task wakes several ticks late.
const PULSE_HISTORY_CAPACITY: usize = 4_096;

#[derive(Debug)]
enum ClockStartStatus {
    Pending,
    Started,
    Failed(String),
    Stopped,
}

#[derive(Debug)]
struct StartSync {
    status: Mutex<ClockStartStatus>,
    changed: Condvar,
}

impl StartSync {
    fn new() -> Self {
        Self {
            status: Mutex::new(ClockStartStatus::Pending),
            changed: Condvar::new(),
        }
    }

    fn lock_status(&self) -> MutexGuard<'_, ClockStartStatus> {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn mark_started(&self) {
        let mut status = self.lock_status();
        if matches!(*status, ClockStartStatus::Pending) {
            *status = ClockStartStatus::Started;
            drop(status);
            self.changed.notify_all();
        }
    }

    fn mark_failed(&self, message: String) {
        let mut status = self.lock_status();
        if matches!(*status, ClockStartStatus::Pending) {
            *status = ClockStartStatus::Failed(message);
            drop(status);
            self.changed.notify_all();
        }
    }

    fn mark_stopped(&self) {
        let mut status = self.lock_status();
        if matches!(*status, ClockStartStatus::Pending) {
            *status = ClockStartStatus::Stopped;
            drop(status);
            self.changed.notify_all();
        }
    }

    fn wait_for_start(&self, timeout: Duration) -> Result<(), Td3Error> {
        let deadline = Instant::now() + timeout;
        let mut status = self.lock_status();

        loop {
            match &*status {
                ClockStartStatus::Started => return Ok(()),
                ClockStartStatus::Failed(message) => {
                    return Err(Td3Error::Midi(message.clone()));
                }
                ClockStartStatus::Stopped => {
                    return Err(Td3Error::Midi(
                        "clock stopped before MIDI Start was sent".to_string(),
                    ));
                }
                ClockStartStatus::Pending => {}
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(Td3Error::Timeout {
                    operation: "MIDI Start acknowledgement".to_string(),
                });
            }

            let remaining = deadline.saturating_duration_since(now);
            status = match self.changed.wait_timeout(status, remaining) {
                Ok((next_status, _)) => next_status,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }
}

/// Cloneable acknowledgement handle for the clock thread's MIDI Start write.
/// A successful wait means `0xFA` was accepted by the MIDI output connection.
#[derive(Clone, Debug)]
pub struct ClockStartMonitor {
    shared: Arc<StartSync>,
    timeout: Duration,
}

impl ClockStartMonitor {
    pub fn wait_for_start(&self) -> Result<(), Td3Error> {
        self.shared.wait_for_start(self.timeout)
    }
}

#[cfg(test)]
pub(crate) struct ClockStartTestHarness {
    shared: Arc<StartSync>,
    timeout: Duration,
}

#[cfg(test)]
impl ClockStartTestHarness {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            shared: Arc::new(StartSync::new()),
            timeout,
        }
    }

    pub(crate) fn monitor(&self) -> ClockStartMonitor {
        ClockStartMonitor {
            shared: Arc::clone(&self.shared),
            timeout: self.timeout,
        }
    }

    pub(crate) fn mark_started(&self) {
        self.shared.mark_started();
    }

    pub(crate) fn mark_failed(&self, message: &str) {
        self.shared.mark_failed(message.to_string());
    }

    pub(crate) fn mark_stopped(&self) {
        self.shared.mark_stopped();
    }
}

/// Last successfully emitted MIDI Clock pulse and the tempo that applies to
/// the interval following it. Pulse zero is the immediate clock sent after
/// MIDI Start. The pulse index never resets during a live tempo change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockPulseSnapshot {
    pub pulse_index: u64,
    pub epoch_micros: u64,
    pub centibpm: u32,
    pub tempo_revision: u64,
}

#[derive(Debug)]
struct PulseState {
    running: bool,
    requested_centibpm: u32,
    requested_revision: u64,
    last_pulse: Option<ClockPulseSnapshot>,
    history: VecDeque<ClockPulseSnapshot>,
}

#[derive(Debug)]
struct PulseSync {
    state: Mutex<PulseState>,
    changed: Condvar,
}

impl PulseSync {
    fn new(initial_centibpm: u32) -> Self {
        Self {
            state: Mutex::new(PulseState {
                running: true,
                requested_centibpm: initial_centibpm.max(1),
                requested_revision: 0,
                last_pulse: None,
                history: VecDeque::with_capacity(PULSE_HISTORY_CAPACITY),
            }),
            changed: Condvar::new(),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, PulseState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn request_tempo(&self, centibpm: u32) -> u64 {
        let mut state = self.lock_state();
        let centibpm = centibpm.max(1);
        if state.requested_centibpm == centibpm {
            return state.requested_revision;
        }
        state.requested_revision = state.requested_revision.saturating_add(1);
        state.requested_centibpm = centibpm;
        state.requested_revision
    }

    /// Record a successful pulse and atomically adopt the newest requested
    /// tempo. The returned tempo applies to the interval after this pulse.
    fn publish_pulse(
        &self,
        pulse_index: u64,
        epoch_micros: u64,
        applied_centibpm: u32,
        applied_revision: u64,
    ) -> ClockPulseSnapshot {
        let mut state = self.lock_state();
        let (centibpm, tempo_revision) = if state.requested_revision > applied_revision {
            (state.requested_centibpm, state.requested_revision)
        } else {
            (applied_centibpm, applied_revision)
        };
        let snapshot = ClockPulseSnapshot {
            pulse_index,
            epoch_micros,
            centibpm,
            tempo_revision,
        };
        if state.history.len() == PULSE_HISTORY_CAPACITY {
            state.history.pop_front();
        }
        state.history.push_back(snapshot);
        state.last_pulse = Some(snapshot);
        drop(state);
        self.changed.notify_all();
        snapshot
    }

    fn mark_stopped(&self) {
        let mut state = self.lock_state();
        state.running = false;
        drop(state);
        self.changed.notify_all();
    }

    fn wait_for_tempo_revision(&self, revision: u64) -> Option<ClockPulseSnapshot> {
        let mut state = self.lock_state();
        loop {
            if let Some(snapshot) = state.last_pulse {
                if snapshot.tempo_revision >= revision {
                    return Some(snapshot);
                }
            }
            if !state.running {
                return None;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn wait_for_pulse(&self, target_pulse: u64) -> Option<ClockPulseSnapshot> {
        let mut state = self.lock_state();
        loop {
            if let Some(last) = state.last_pulse {
                if last.pulse_index >= target_pulse {
                    return state
                        .history
                        .iter()
                        .find(|snapshot| snapshot.pulse_index == target_pulse)
                        .copied();
                }
            }
            if !state.running {
                return None;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn latest_running_pulse(&self) -> Option<ClockPulseSnapshot> {
        let state = self.lock_state();
        if state.running {
            state.last_pulse
        } else {
            None
        }
    }
}

/// Cloneable wait handle used by async handlers through `spawn_blocking`.
#[derive(Clone, Debug)]
pub struct ClockPulseMonitor {
    shared: Arc<PulseSync>,
}

impl ClockPulseMonitor {
    pub fn wait_for_tempo_revision(&self, revision: u64) -> Option<ClockPulseSnapshot> {
        self.shared.wait_for_tempo_revision(revision)
    }

    pub fn wait_for_pulse(&self, target_pulse: u64) -> Option<ClockPulseSnapshot> {
        self.shared.wait_for_pulse(target_pulse)
    }

    pub fn latest_running_pulse(&self) -> Option<ClockPulseSnapshot> {
        self.shared.latest_running_pulse()
    }
}

#[cfg(test)]
pub(crate) struct ClockPulseTestHarness {
    shared: Arc<PulseSync>,
    next_pulse_index: u64,
    applied_centibpm: u32,
    applied_revision: u64,
}

#[cfg(test)]
impl ClockPulseTestHarness {
    pub(crate) fn new(initial_centibpm: u32) -> Self {
        Self {
            shared: Arc::new(PulseSync::new(initial_centibpm)),
            next_pulse_index: 0,
            applied_centibpm: initial_centibpm.max(1),
            applied_revision: 0,
        }
    }

    pub(crate) fn monitor(&self) -> ClockPulseMonitor {
        ClockPulseMonitor {
            shared: Arc::clone(&self.shared),
        }
    }

    pub(crate) fn set_centibpm(&self, centibpm: u32) -> u64 {
        self.shared.request_tempo(centibpm)
    }

    pub(crate) fn publish_pulse(&mut self, epoch_micros: u64) -> ClockPulseSnapshot {
        let snapshot = self.shared.publish_pulse(
            self.next_pulse_index,
            epoch_micros,
            self.applied_centibpm,
            self.applied_revision,
        );
        self.next_pulse_index = self.next_pulse_index.saturating_add(1);
        self.applied_centibpm = snapshot.centibpm;
        self.applied_revision = snapshot.tempo_revision;
        snapshot
    }

    pub(crate) fn stop(&self) {
        self.shared.mark_stopped();
    }
}

/// Calculate the interval between clock ticks for a given tempo,
/// where tempo is expressed in centi-BPM (BPM x 100). Exposed for unit
/// tests; the clock thread uses `tick_period_micros` directly so
/// integer math never round-trips through `Duration`.
#[allow(dead_code)] // used by tests::web_tests
pub fn tick_interval(centibpm: u32) -> Duration {
    Duration::from_micros(tick_period_micros(centibpm))
}

/// Calculate one full pattern cycle from MIDI clock pulses. Normal TD-3
/// timing uses 6 pulses per step, while triplet timing uses 8. Tempo
/// is expressed in centi-BPM (BPM x 100).
#[allow(dead_code)] // used by tests::web_tests
pub fn pattern_wrap_duration(centibpm: u32, active_steps: u8, triplet: bool) -> Duration {
    let pulse_count = pattern_wrap_pulses(active_steps, triplet);
    Duration::from_micros(tick_period_micros(centibpm).saturating_mul(pulse_count))
}

/// MIDI Clock pulses in one pattern cycle. Normal steps consume six pulses;
/// triplet steps consume eight.
pub fn pattern_wrap_pulses(active_steps: u8, triplet: bool) -> u64 {
    let steps_per_beat = if triplet { 3 } else { 4 };
    let pulses_per_step = PPQN / steps_per_beat;
    active_steps.max(1) as u64 * pulses_per_step as u64
}

/// Integer tick period in microseconds for a centi-BPM tempo. Centi-BPM
/// is clamped to >= 1 so we never divide by zero.
///
/// Derivation: at BPM `b` and 24 PPQN, period = 60_000_000 / (b * 24)
/// microseconds. Substituting `b = centibpm / 100` and rearranging to
/// keep all arithmetic in integers:
///     period = (60_000_000 * 100) / (centibpm * 24)
///            = 250_000_000 / centibpm.
/// Numerically equivalent to the legacy `60_000_000 / (bpm * 24)` for
/// integer BPM (centibpm = bpm * 100); fractional BPM resolves to its
/// own integer microsecond value at sub-microsecond granularity.
fn tick_period_micros(centibpm: u32) -> u64 {
    let centibpm = centibpm.max(1) as u64;
    250_000_000u64 / centibpm
}

/// A byte sequence to be sent on the clock thread's output port,
/// paired with a completion channel so the enqueuer can observe
/// success/failure. The thread replies exactly once per request,
/// then drops `done` - the caller's `recv` returns immediately.
struct SendRequest {
    bytes: Vec<u8>,
    done: Sender<Result<(), Td3Error>>,
}

struct ClockSignals {
    start_sync: Arc<StartSync>,
    pulse_sync: Arc<PulseSync>,
    stop: Arc<AtomicBool>,
    abandon: Arc<AtomicBool>,
}

/// Handle to a clock thread. Call `stop()` (or drop) to shut it down cleanly.
/// Once MIDI Start has been sent, the thread emits MIDI Stop (0xFC) and joins.
///
/// Tempo state uses integer centi-BPM (BPM x 100), giving 0.01 BPM
/// resolution without floats. Successful pulses publish synchronization
/// snapshots for tempo acknowledgements and wrap waiters.
pub struct ClockRunner {
    start_sync: Arc<StartSync>,
    start_wait_timeout: Duration,
    pulse_sync: Arc<PulseSync>,
    stop: Arc<AtomicBool>,
    abandon: Arc<AtomicBool>,
    /// Sender for the SysEx send queue. Handlers clone nothing -
    /// they hold `&ClockRunner` and submit through `send_blocking`.
    send_tx: Sender<SendRequest>,
    thread: Option<JoinHandle<midir::MidiOutputConnection>>,
}

impl ClockRunner {
    /// Spawn the clock thread and wait for `start_delay` before sending
    /// MIDI Start. A zero delay starts immediately.
    pub fn spawn_scheduled(
        out_conn: midir::MidiOutputConnection,
        initial_centibpm: u32,
        start_delay: Duration,
        lane_inbox: super::cutoff_lane::LaneInbox,
    ) -> Result<Self, Td3Error> {
        let start_sync = Arc::new(StartSync::new());
        let start_wait_timeout = start_delay.saturating_add(START_ACK_GRACE);
        let pulse_sync = Arc::new(PulseSync::new(initial_centibpm));
        let stop = Arc::new(AtomicBool::new(false));
        let abandon = Arc::new(AtomicBool::new(false));
        let (send_tx, send_rx) = mpsc::channel::<SendRequest>();

        let thread = {
            let start_sync = Arc::clone(&start_sync);
            let pulse_sync = Arc::clone(&pulse_sync);
            let stop = Arc::clone(&stop);
            let abandon = Arc::clone(&abandon);
            thread::Builder::new()
                .name("td3-midi-clock".into())
                .spawn(move || {
                    let mut out = out_conn;
                    run_clock(
                        &mut out,
                        initial_centibpm.max(1),
                        ClockSignals {
                            start_sync,
                            pulse_sync,
                            stop,
                            abandon,
                        },
                        send_rx,
                        start_delay,
                        lane_inbox,
                    );
                    out
                })
                .map_err(|e| Td3Error::Midi(format!("failed to spawn MIDI clock thread: {}", e)))?
        };

        Ok(Self {
            start_sync,
            start_wait_timeout,
            pulse_sync,
            stop,
            abandon,
            send_tx,
            thread: Some(thread),
        })
    }

    /// Return a monitor that resolves only after the clock thread has
    /// successfully written MIDI Start, or reports why it could not start.
    pub fn start_monitor(&self) -> ClockStartMonitor {
        ClockStartMonitor {
            shared: Arc::clone(&self.start_sync),
            timeout: self.start_wait_timeout,
        }
    }

    /// Queue a tempo in centi-BPM (BPM x 100) and return its revision.
    /// The clock adopts the newest pending revision after a successful
    /// pulse, then schedules the following pulse one full new period later.
    pub fn set_centibpm(&self, new_centibpm: u32) -> u64 {
        self.pulse_sync.request_tempo(new_centibpm)
    }

    pub fn pulse_monitor(&self) -> ClockPulseMonitor {
        ClockPulseMonitor {
            shared: Arc::clone(&self.pulse_sync),
        }
    }

    /// Signal the clock thread and release synchronization waiters without
    /// waiting for a potentially blocked MIDI driver call to return.
    pub(crate) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.start_sync.mark_stopped();
        self.pulse_sync.mark_stopped();
    }

    /// Enqueue a byte sequence to be sent on the clock thread's
    /// output connection and block until the thread reports the send
    /// result. Used during playback so SysEx handlers (pattern save,
    /// pattern load, etc.) can talk to the device without tearing
    /// down the clock.
    ///
    /// The thread drains between 0xF8 ticks, so latency is at most
    /// one tick period plus the actual USB write time (<1 ms for a
    /// 112-byte pattern on USB-MIDI).
    pub fn send_blocking(&self, bytes: Vec<u8>) -> Result<(), Td3Error> {
        let (done_tx, done_rx) = mpsc::channel();
        let req = SendRequest {
            bytes,
            done: done_tx,
        };
        self.send_tx
            .send(req)
            .map_err(|_| Td3Error::Midi("clock thread send queue closed".into()))?;
        match done_rx.recv_timeout(QUEUE_SEND_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(Td3Error::Timeout {
                operation: "clock queue send".to_owned(),
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Td3Error::Midi(
                "clock thread dropped send completion before reply".into(),
            )),
        }
    }

    /// Signal the thread to stop and wait for it to exit. Returns the
    /// `MidiOutputConnection` that the thread was using so the caller
    /// can put it back into the session. Returns `None` only if the
    /// thread panicked (very unusual - `run_clock` cannot panic in
    /// normal operation).
    pub fn stop(mut self) -> Option<midir::MidiOutputConnection> {
        self.request_stop();
        self.thread.take().and_then(|t| t.join().ok())
    }

    /// Signal shutdown and detach the OS thread when a MIDI driver call has
    /// already exceeded its API timeout. The connection is discarded when
    /// the thread eventually exits.
    pub(crate) fn stop_detached(mut self, cleanup_pending: Arc<AtomicUsize>) {
        self.abandon.store(true, Ordering::Release);
        self.request_stop();
        let Some(clock_thread) = self.thread.take() else {
            return;
        };
        cleanup_pending.fetch_add(1, Ordering::AcqRel);
        let holder = Arc::new(Mutex::new(Some(clock_thread)));
        let worker_holder = Arc::clone(&holder);
        let cleanup_flag = Arc::clone(&cleanup_pending);
        let spawned = thread::Builder::new()
            .name("td3-midi-clock-cleanup".to_string())
            .spawn(move || {
                let clock_thread = worker_holder
                    .lock()
                    .ok()
                    .and_then(|mut holder| holder.take());
                let Some(clock_thread) = clock_thread else {
                    log::error!(
                        "clock cleanup lost its join handle; MIDI reconnect remains blocked"
                    );
                    return;
                };
                let _ = clock_thread.join();
                cleanup_flag.fetch_sub(1, Ordering::AcqRel);
            });
        if spawned.is_err() {
            // Retain the join handle so neither it nor a completed thread's
            // output connection can be dropped on the request thread.
            std::mem::forget(holder);
            log::error!(
                "clock cleanup thread could not start; MIDI reconnect remains blocked for safety"
            );
        }
    }
}

impl Drop for ClockRunner {
    fn drop(&mut self) {
        // Defensive: if the runner is dropped without `stop()` being
        // called (e.g. a panic unwinds past the handler), still signal
        // the thread and join so we never leak the OS thread or hold
        // the MIDI port open indefinitely. The connection drops with
        // the join result - reconnect will re-open the port.
        self.request_stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl SysexSender for ClockRunner {
    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), Td3Error> {
        self.send_blocking(bytes.to_vec())
    }
}

fn run_clock(
    out: &mut midir::MidiOutputConnection,
    initial_centibpm: u32,
    signals: ClockSignals,
    send_rx: Receiver<SendRequest>,
    start_delay: Duration,
    lane_inbox: super::cutoff_lane::LaneInbox,
) {
    let mut lane_tracker = super::cutoff_lane::LaneTracker::new();
    let ClockSignals {
        start_sync,
        pulse_sync,
        stop,
        abandon,
    } = signals;
    // Raise Windows timer resolution to 1 ms for the whole playback
    // session. Kept alive in a local so `Drop` runs when this function
    // returns - after the final MIDI Stop - restoring the process-wide
    // default. No-op on non-Windows targets.
    let _timer_guard = TimerPeriodGuard::acquire();

    // Bump this thread to TIME_CRITICAL. No-op on non-Windows.
    raise_thread_priority_time_critical();

    // Try to create a high-resolution waitable timer. On modern
    // Windows this gives us tens-of-microseconds wake precision;
    // elsewhere (and on failure) `sleep_until` is used instead.
    #[cfg(windows)]
    let hr_timer: Option<WaitableTimer> = WaitableTimer::try_new();

    if !start_delay.is_zero() {
        let start_at = Instant::now() + start_delay;
        if !wait_for_scheduled_start(start_at, &stop) {
            start_sync.mark_stopped();
            pulse_sync.mark_stopped();
            return;
        }
    }

    // Fire MIDI Start first so the device resets its clock division before
    // the first 0xF8 arrives. A failed Start is fatal because neither the
    // device nor the HTTP caller can treat the transport as running.
    if let Err(err) = out.send(&[MIDI_START]) {
        let message = format!("MIDI Start send failed: {}", err);
        log::warn!("clock: {}", message);
        start_sync.mark_failed(message);
        pulse_sync.mark_stopped();
        return;
    }
    start_sync.mark_started();

    // Tick zero fires immediately after Start. Stable-tempo deadlines remain
    // phase-locked to the prior deadline. A tempo update is adopted only
    // after a successful pulse and schedules the next pulse one complete new
    // period later, preserving musical pulse position without a compressed
    // double tick.
    let mut next_deadline = Instant::now();
    let mut pulse_index: u64 = 0;
    let mut current_centibpm = initial_centibpm.max(1);
    let mut current_tempo_revision = 0u64;
    let mut period_us = tick_period_micros(current_centibpm);

    while !stop.load(Ordering::Acquire) {
        let mut deadline = next_deadline;
        let now = Instant::now();

        if deadline > now {
            // Prefer the high-resolution waitable timer (Windows).
            // Fall back to hybrid sleep+spin on other OSes and on
            // timer-creation failure. Both paths park the thread
            // until `deadline` is reached.
            #[cfg(windows)]
            {
                match &hr_timer {
                    Some(t) => t.wait_until(deadline, &stop),
                    None => sleep_until(deadline, &stop),
                }
            }
            #[cfg(not(windows))]
            {
                sleep_until(deadline, &stop);
            }
        } else if now.saturating_duration_since(deadline).as_micros() as u64 > period_us {
            // Fell more than one full period behind - re-anchor instead
            // of burst-firing the backlog. Burst-firing was exactly
            // what compressed the clock in the scope trace.
            deadline = now;
        }
        // else: we're late by <1 period - fire immediately, the phase
        // lock tightens over the next few ticks.

        // Re-check stop between the sleep and the send so we never
        // emit a ghost tick after shutdown was requested.
        if stop.load(Ordering::Acquire) {
            break;
        }

        // A step-start pulse carries its lane Control Change first, so the
        // filter is in position when the device sounds the step on the
        // clock byte that follows. A failed write is logged and playback
        // carries on; the clock byte below decides whether the port is gone.
        lane_tracker.poll_inbox(&lane_inbox);
        if let Some(cc) = lane_tracker.on_pulse(pulse_index) {
            if let Err(e) = out.send(&cc) {
                log::warn!("clock: cutoff lane send failed: {}", e);
            }
        }

        if let Err(e) = out.send(&[MIDI_CLOCK]) {
            // Port vanished (device unplugged mid-play), or the driver
            // is in a bad state. Exit cleanly - a reconnect spawns a
            // fresh runner with a new connection.
            log::warn!("clock: tick send failed, stopping: {}", e);
            break;
        }

        let sent_at = Instant::now();
        let snapshot = pulse_sync.publish_pulse(
            pulse_index,
            current_epoch_micros(),
            current_centibpm,
            current_tempo_revision,
        );
        pulse_index = pulse_index.saturating_add(1);

        let tempo_changed = snapshot.tempo_revision != current_tempo_revision;
        current_centibpm = snapshot.centibpm;
        current_tempo_revision = snapshot.tempo_revision;
        period_us = tick_period_micros(current_centibpm);
        next_deadline = next_tick_deadline(deadline, sent_at, period_us, tempo_changed);

        // After the tick, drain any queued SysEx sends until close to
        // the next deadline. Handlers (e.g. pattern save during the
        // progression hot-swap) wait on the reply channel.
        drain_send_queue(out, &send_rx, next_deadline, &stop, &abandon);
    }

    pulse_sync.mark_stopped();

    let abandoned = abandon.load(Ordering::Acquire);
    let shutdown_message = if abandoned {
        "clock transport was abandoned after a MIDI timeout"
    } else {
        "clock transport stopped before queued send"
    };
    while let Ok(req) = send_rx.try_recv() {
        let _ = req
            .done
            .send(Err(Td3Error::Midi(shutdown_message.to_string())));
    }

    if !abandoned {
        if let Err(e) = out.send(&[MIDI_STOP]) {
            log::warn!("clock: MIDI Stop send failed: {}", e);
        }
    }
    // `out` drops here - the MIDI connection closes.
}

/// Drain pending SysEx sends after a tick, stopping before we'd push
/// the next tick past its deadline. Each queued send reports its
/// result back on the request's completion channel.
fn drain_send_queue(
    out: &mut midir::MidiOutputConnection,
    send_rx: &Receiver<SendRequest>,
    next_deadline: Instant,
    stop: &AtomicBool,
    abandon: &AtomicBool,
) {
    loop {
        if queued_send_rejection(stop, abandon).is_some() {
            return;
        }

        // Bail out if we're already close to the next deadline. The
        // check is at the top of each iteration so a send that ran
        // long doesn't cause us to start another one.
        let now = Instant::now();
        if next_deadline.saturating_duration_since(now) < DRAIN_SAFETY_MARGIN {
            return;
        }

        match send_rx.try_recv() {
            Ok(req) => {
                if let Some(message) = queued_send_rejection(stop, abandon) {
                    let _ = req.done.send(Err(Td3Error::Midi(message.to_string())));
                    return;
                }
                let result = out
                    .send(&req.bytes)
                    .map_err(|e| Td3Error::Midi(format!("queued send failed: {}", e)));
                // Best-effort notify: if the caller hung up (dropped
                // the completion rx on timeout) we don't care.
                let _ = req.done.send(result);
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return,
        }
    }
}

pub(crate) fn queued_send_rejection(
    stop: &AtomicBool,
    abandon: &AtomicBool,
) -> Option<&'static str> {
    if abandon.load(Ordering::Acquire) {
        Some("clock transport was abandoned after a MIDI timeout")
    } else if stop.load(Ordering::Acquire) {
        Some("clock transport stopped before queued send")
    } else {
        None
    }
}

fn current_epoch_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

pub(crate) fn wait_for_scheduled_start(deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Acquire) {
            return false;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }

        if remaining <= SCHEDULE_PRECISION_WINDOW {
            sleep_until(deadline, stop);
            return !stop.load(Ordering::Acquire);
        }

        let coarse_wait = remaining
            .saturating_sub(SCHEDULE_PRECISION_WINDOW)
            .min(SCHEDULE_STOP_POLL_INTERVAL);
        thread::sleep(coarse_wait);
    }
}

pub(crate) fn next_tick_deadline(
    scheduled_deadline: Instant,
    sent_at: Instant,
    period_us: u64,
    tempo_changed: bool,
) -> Instant {
    let period = Duration::from_micros(period_us);
    if tempo_changed {
        sent_at + period
    } else {
        scheduled_deadline + period
    }
}
