//! MIDI clock: dedicated OS thread that sends 0xF8 at 24 PPQN with
//! monotonic, phase-locked scheduling.
//!
//! ## Timing Model
//!
//! A real OS thread (`std::thread::spawn`) borrows the session's
//! `MidiOutputConnection` for the lifetime of playback. (Windows's
//! winmm driver refuses two handles to the same output port with
//! `MMSYSERR_ALLOCATED`, so a separate clock handle isn't an option.)
//! The thread never acquires the async session mutex. Scheduling is
//! phase-locked against a start `Instant`: each tick is at
//! `epoch + tick_idx * period`, so transient lateness catches up
//! within one period without producing a burst. If the scheduler
//! misses us by more than one period we re-anchor to the present
//! rather than firing the backlog.
//!
//! The thread returns the connection through its `JoinHandle` on
//! exit, and `stop()` hands it back so the caller can put it back
//! into the session.
//!
//! ## SysEx during playback
//!
//! The progression feature (`ui/js/progression-transport.js`) calls
//! `api.savePattern` mid-play to hot-swap patterns on the device
//! (the device pre-loads the next pattern 8 steps before its
//! internal wrap-around). Since the clock thread owns the output
//! port exclusively while playing, handlers can't send directly -
//! they enqueue a `SendRequest` on the runner and block until the
//! clock thread forwards the bytes on the shared connection.
//!
//! The thread drains the queue *after* each 0xF8 tick and stops
//! draining within a safety margin of the next deadline so the
//! tick itself is never delayed by a queued send. Individual SysEx
//! sends on USB-MIDI are sub-millisecond - the 20 ms tick period at
//! 120 BPM has ample headroom.
//!
//! Shared state (BPM, stop signal) travels as atomics - lock-free
//! reads on every tick.
//!
//! ## Windows timing hardening
//!
//! Four things happen on Windows at the start of every playback:
//!
//! 1. **Timer resolution → 1 ms** via `timeBeginPeriod(1)` (RAII
//!    guard pairs it with `timeEndPeriod(1)`). Belt-and-suspenders:
//!    we still use a waitable timer below, but the 1 ms quantum
//!    lowers jitter in `WaitForSingleObject` wake paths for
//!    Windows 10 builds where the high-resolution timer falls back
//!    to the standard timer.
//! 2. **Thread priority → TIME_CRITICAL** via `SetThreadPriority`,
//!    so the OS scheduler isn't tempted to preempt the clock for
//!    normal-priority work (tokio workers, the HTTP handler, the
//!    browser IPC, etc). The priority is local to this thread and
//!    is released when the thread exits.
//! 3. **High-resolution waitable timer** (`CreateWaitableTimerExW`
//!    with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`, Windows 10
//!    1803+). `SetWaitableTimer` schedules the wake at 100-ns
//!    granularity and `WaitForSingleObject` returns within tens of
//!    microseconds of the requested deadline - immune to the
//!    15.625 ms quantum that plagues `thread::sleep`. On older
//!    Windows or if the high-res flag is unsupported, falls back
//!    transparently to hybrid sleep+spin.
//! 4. **Spin tail (fallback only)** - if the waitable timer can't
//!    be created, `sleep_until` sleeps for the bulk of the wait
//!    then spins the last sub-millisecond on `Instant::now()`. On
//!    modern Windows 10/11 this path is never taken.

mod runner;
mod timing;

#[allow(unused_imports)]
pub use runner::tick_interval;
pub use runner::{pattern_wrap_duration, ClockRunner, PPQN};
