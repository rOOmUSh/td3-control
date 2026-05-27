use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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
    let steps_per_beat = if triplet { 3 } else { 4 };
    let pulses_per_step = PPQN / steps_per_beat;
    let pulse_count = active_steps.max(1) as u64 * pulses_per_step as u64;
    Duration::from_micros(tick_period_micros(centibpm).saturating_mul(pulse_count))
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

/// Handle to a running clock thread. Call `stop()` (or drop) to shut
/// it down cleanly - the thread emits MIDI Stop (0xFC) and joins.
///
/// Tempo state is stored as centi-BPM (BPM x 100) in an `AtomicU32`,
/// giving 0.01 BPM resolution without floats.
pub struct ClockRunner {
    centibpm: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
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
    ) -> Result<Self, Td3Error> {
        let centibpm = Arc::new(AtomicU32::new(initial_centibpm.max(1)));
        let stop = Arc::new(AtomicBool::new(false));
        let (send_tx, send_rx) = mpsc::channel::<SendRequest>();

        let thread = {
            let centibpm = Arc::clone(&centibpm);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("td3-midi-clock".into())
                .spawn(move || {
                    let mut out = out_conn;
                    run_clock(&mut out, centibpm, stop, send_rx, start_delay);
                    out
                })
                .map_err(|e| Td3Error::Midi(format!("failed to spawn MIDI clock thread: {}", e)))?
        };

        Ok(Self {
            centibpm,
            stop,
            send_tx,
            thread: Some(thread),
        })
    }

    /// Update the tempo in centi-BPM (BPM x 100). Takes effect on the
    /// next tick. The thread re-anchors its phase reference so the new
    /// period applies from that moment (no accumulated drift catch-up).
    pub fn set_centibpm(&self, new_centibpm: u32) {
        self.centibpm.store(new_centibpm.max(1), Ordering::Release);
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
        self.stop.store(true, Ordering::Release);
        self.thread.take().and_then(|t| t.join().ok())
    }
}

impl Drop for ClockRunner {
    fn drop(&mut self) {
        // Defensive: if the runner is dropped without `stop()` being
        // called (e.g. a panic unwinds past the handler), still signal
        // the thread and join so we never leak the OS thread or hold
        // the MIDI port open indefinitely. The connection drops with
        // the join result - reconnect will re-open the port.
        self.stop.store(true, Ordering::Release);
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
    centibpm: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    send_rx: Receiver<SendRequest>,
    start_delay: Duration,
) {
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
        sleep_until(start_at, &stop);
        if stop.load(Ordering::Acquire) {
            return;
        }
    }

    // Fire MIDI Start first so the device resets its clock division
    // before the first 0xF8 arrives. Failure is logged but not fatal:
    // some USB-MIDI stacks return transient errors that clear on the
    // very next send, and the tick loop below will surface a real
    // disconnect quickly anyway.
    if let Err(e) = out.send(&[MIDI_START]) {
        log::warn!("clock: MIDI Start send failed: {}", e);
    }

    // Phase-locked schedule. `epoch` is the reference moment; tick N
    // is scheduled at `epoch + N * period`. Tick 0 fires immediately
    // after Start to match the prior `tokio::time::interval` behavior
    // (which also fires the first tick on the same instant).
    let mut epoch = Instant::now();
    let mut tick_idx: u64 = 0;
    let mut current_centibpm = centibpm.load(Ordering::Acquire).max(1);
    let mut period_us = tick_period_micros(current_centibpm);

    while !stop.load(Ordering::Acquire) {
        // Tempo change? Re-anchor to "now" so the new tempo applies from
        // this moment - no catch-up burst from the old schedule.
        let latest_centibpm = centibpm.load(Ordering::Acquire).max(1);
        if latest_centibpm != current_centibpm {
            current_centibpm = latest_centibpm;
            period_us = tick_period_micros(current_centibpm);
            epoch = Instant::now();
            tick_idx = 0;
        }

        // Compute the deadline for this tick. `saturating_mul` guards
        // against theoretical overflow on multi-year uninterrupted runs.
        let elapsed = Duration::from_micros(period_us.saturating_mul(tick_idx));
        let deadline = epoch + elapsed;
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
            epoch = now;
            tick_idx = 0;
        }
        // else: we're late by <1 period - fire immediately, the phase
        // lock tightens over the next few ticks.

        // Re-check stop between the sleep and the send so we never
        // emit a ghost tick after shutdown was requested.
        if stop.load(Ordering::Acquire) {
            break;
        }

        if let Err(e) = out.send(&[MIDI_CLOCK]) {
            // Port vanished (device unplugged mid-play), or the driver
            // is in a bad state. Exit cleanly - a reconnect spawns a
            // fresh runner with a new connection.
            log::warn!("clock: tick send failed, stopping: {}", e);
            break;
        }

        tick_idx = tick_idx.saturating_add(1);

        // After the tick, drain any queued SysEx sends until close to
        // the next deadline. Handlers (e.g. pattern save during the
        // progression hot-swap) wait on the reply channel.
        drain_send_queue(out, &send_rx, epoch, period_us, tick_idx);
    }

    // Drain any remaining queued sends before the port closes so
    // handlers don't hang on their completion receiver. They'll see
    // an error because we're already past the tick loop, but a real
    // reply (or failure) is better than a timeout.
    while let Ok(req) = send_rx.try_recv() {
        let result = out
            .send(&req.bytes)
            .map_err(|e| Td3Error::Midi(format!("queued send during shutdown: {}", e)));
        let _ = req.done.send(result);
    }

    // Always attempt MIDI Stop on the way out, even after a send
    // failure above - the driver may have recovered, and emitting a
    // stray 0xFC is cheap.
    if let Err(e) = out.send(&[MIDI_STOP]) {
        log::warn!("clock: MIDI Stop send failed: {}", e);
    }
    // `out` drops here - the MIDI connection closes.
}

/// Drain pending SysEx sends after a tick, stopping before we'd push
/// the next tick past its deadline. Each queued send reports its
/// result back on the request's completion channel.
fn drain_send_queue(
    out: &mut midir::MidiOutputConnection,
    send_rx: &Receiver<SendRequest>,
    epoch: Instant,
    period_us: u64,
    next_tick_idx: u64,
) {
    let next_deadline = epoch + Duration::from_micros(period_us.saturating_mul(next_tick_idx));

    loop {
        // Bail out if we're already close to the next deadline. The
        // check is at the top of each iteration so a send that ran
        // long doesn't cause us to start another one.
        let now = Instant::now();
        if next_deadline.saturating_duration_since(now) < DRAIN_SAFETY_MARGIN {
            return;
        }

        match send_rx.try_recv() {
            Ok(req) => {
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
