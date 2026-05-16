use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// RAII guard that raises the Windows multimedia timer resolution to
/// 1 ms for its lifetime. Construction calls `timeBeginPeriod(1)`;
/// `Drop` calls `timeEndPeriod(1)` so the process-wide resolution is
/// restored once the last guard goes away. On non-Windows targets the
/// guard is a no-op zero-sized placeholder so the clock thread
/// compiles unchanged.
///
/// Why it matters: `std::thread::sleep` on Windows aligns to the
/// system timer (default 15.625 ms). Without this the MIDI clock
/// sleeps oversleep by up to a full timer quantum per tick, which at
/// 120–160 BPM is most of the tick period - audibly jittery 0xF8.
#[cfg(windows)]
pub(super) struct TimerPeriodGuard {
    active: bool,
}

#[cfg(windows)]
impl TimerPeriodGuard {
    pub(super) fn acquire() -> Self {
        // TIMERR_NOERROR == 0. Any non-zero return means the driver
        // refused the request; we log and carry on - the clock still
        // runs with the coarser default granularity, which the
        // spin-tail in `sleep_until` will still paper over, just at
        // higher CPU cost.
        // SAFETY: `timeBeginPeriod` is an FFI call with no pointer
        // arguments and no shared-state preconditions. The only
        // contract is that every `Begin` is paired with an `End`
        // (handled by the matching `Drop` impl).
        let rc = unsafe { windows_sys::Win32::Media::timeBeginPeriod(1) };
        let active = rc == 0;
        if active {
            log::info!(
                "clock: raised Windows timer resolution to 1 ms \
                 (timeBeginPeriod(1) ok)"
            );
        } else {
            log::warn!(
                "clock: timeBeginPeriod(1) returned {} - falling back to \
                 default (~15.6 ms) granularity; spin-tail will compensate",
                rc
            );
        }
        TimerPeriodGuard { active }
    }
}

#[cfg(windows)]
impl Drop for TimerPeriodGuard {
    fn drop(&mut self) {
        // SAFETY: paired with the `timeBeginPeriod(1)` call in
        // `acquire`. Skip the `End` if `Begin` failed so we don't
        // leave the process one step into the negative.
        if self.active {
            unsafe {
                let _ = windows_sys::Win32::Media::timeEndPeriod(1);
            }
        }
    }
}

/// Bump the calling thread to `THREAD_PRIORITY_TIME_CRITICAL` so the
/// scheduler rarely preempts it. Affects only this thread; no Drop
/// restoration is needed because the thread exits at end of playback
/// (the whole thread goes away, priority and all).
#[cfg(windows)]
pub(super) fn raise_thread_priority_time_critical() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
    };
    // SAFETY: `GetCurrentThread` returns a pseudo-handle that is
    // always valid; `SetThreadPriority` takes that handle and a
    // priority constant from Win32::System::Threading. No aliasing
    // or lifetime concerns.
    let ok = unsafe {
        let h = GetCurrentThread();
        SetThreadPriority(h, THREAD_PRIORITY_TIME_CRITICAL) != 0
    };
    if ok {
        log::info!("clock: raised thread priority to TIME_CRITICAL");
    } else {
        log::warn!(
            "clock: SetThreadPriority(TIME_CRITICAL) failed; \
             running at default priority"
        );
    }
}

#[cfg(not(windows))]
pub(super) struct TimerPeriodGuard;

#[cfg(not(windows))]
impl TimerPeriodGuard {
    pub(super) fn acquire() -> Self {
        TimerPeriodGuard
    }
}

/// Apply `THREAD_TIME_CONSTRAINT_POLICY` to the calling thread so the
/// Mach scheduler treats it as soft real-time and runs it ahead of
/// normal-priority work. The policy expresses a per-period CPU budget
/// (`computation`) that must complete within `constraint` of every
/// `period` start, with no preemption by other real-time threads.
/// Affects only this thread; no restoration is needed because the
/// thread exits at end of playback. On failure the thread keeps its
/// default scheduling priority and the spin-tail fallback in
/// `sleep_until` absorbs the resulting wake jitter.
#[cfg(target_os = "macos")]
pub(super) fn raise_thread_priority_time_critical() {
    use std::os::raw::{c_int, c_uint};

    const THREAD_TIME_CONSTRAINT_POLICY: c_uint = 2;
    // sizeof(thread_time_constraint_policy_data_t) / sizeof(integer_t)
    // = 16 / 4 = 4 - see <mach/thread_policy.h>.
    const THREAD_TIME_CONSTRAINT_POLICY_COUNT: c_uint = 4;

    #[repr(C)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }

    #[repr(C)]
    struct ThreadTimeConstraintPolicy {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: u32,
    }

    extern "C" {
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> c_int;
        fn mach_thread_self() -> c_uint;
        fn mach_task_self() -> c_uint;
        fn mach_port_deallocate(task: c_uint, name: c_uint) -> c_int;
        fn thread_policy_set(
            thread: c_uint,
            flavor: c_uint,
            policy_info: *const c_int,
            count: c_uint,
        ) -> c_int;
    }

    // SAFETY: `mach_timebase_info` writes two u32 fields into the
    // out-parameter; the struct is fully initialised before the call
    // and its layout matches `<mach/mach_time.h>`.
    let mut tb = MachTimebaseInfo { numer: 0, denom: 0 };
    let rc = unsafe { mach_timebase_info(&mut tb) };
    if rc != 0 || tb.numer == 0 || tb.denom == 0 {
        log::warn!(
            "clock: mach_timebase_info failed (rc={}, numer={}, denom={}); \
             skipping macOS RT policy",
            rc,
            tb.numer,
            tb.denom
        );
        return;
    }

    // ticks = ns * denom / numer. u128 intermediate prevents overflow
    // on architectures where numer/denom invert nanos to many ticks;
    // clamp to u32::MAX so the policy struct's fields accept the value.
    let ns_to_abs = |ns: u64| -> u32 {
        let v = (ns as u128).saturating_mul(tb.denom as u128) / tb.numer as u128;
        v.min(u32::MAX as u128) as u32
    };

    // period: tick interval hint (20 ms ≈ 24 PPQN at 120 BPM).
    // computation: per-tick CPU budget - sending one 0xF8 byte on
    //   USB-MIDI completes in tens of microseconds.
    // constraint: max acceptable latency from each period boundary.
    // preemptible = 0: other real-time threads must not preempt.
    let policy = ThreadTimeConstraintPolicy {
        period: ns_to_abs(20_000_000),
        computation: ns_to_abs(100_000),
        constraint: ns_to_abs(1_000_000),
        preemptible: 0,
    };

    // SAFETY: `mach_thread_self` returns a thread send-right that we
    // balance with `mach_port_deallocate` below. `thread_policy_set`
    // reads `THREAD_TIME_CONSTRAINT_POLICY_COUNT` `integer_t` slots
    // from `policy_info`, which matches the four u32 fields of
    // `ThreadTimeConstraintPolicy` (16 bytes / 4 bytes per integer_t).
    let thread = unsafe { mach_thread_self() };
    let rc = unsafe {
        thread_policy_set(
            thread,
            THREAD_TIME_CONSTRAINT_POLICY,
            &policy as *const _ as *const c_int,
            THREAD_TIME_CONSTRAINT_POLICY_COUNT,
        )
    };
    // SAFETY: `thread` was produced by the `mach_thread_self` call
    // above; `mach_task_self` is the matching task port for the
    // current process. Pairing the send-right with a deallocate
    // prevents a port-name leak across repeated playback sessions.
    unsafe {
        mach_port_deallocate(mach_task_self(), thread);
    }

    if rc == 0 {
        log::info!(
            "clock: applied THREAD_TIME_CONSTRAINT_POLICY \
             (period 20ms, computation 100us, constraint 1ms, non-preemptible)"
        );
    } else {
        log::warn!(
            "clock: thread_policy_set returned {}; running at default \
             scheduling priority",
            rc
        );
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn raise_thread_priority_time_critical() {}

/// Owned Windows waitable timer HANDLE. Created once per playback
/// session with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION` so wake
/// latency is tens of microseconds, not a multimedia-timer quantum.
/// Closed via `CloseHandle` on `Drop`.
///
/// `SAFETY`: the HANDLE is an opaque pointer that's only ever
/// consumed by Win32 APIs that accept it by value. `HANDLE` is
/// `Send` via `isize` under the hood, but the `*mut c_void` newtype
/// in `windows-sys` is not - hence the explicit `unsafe impl Send`.
/// We only ever use the handle from the single clock thread that
/// owns the guard, so there's no concurrent access.
#[cfg(windows)]
pub(super) struct WaitableTimer {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for WaitableTimer {}

#[cfg(windows)]
impl WaitableTimer {
    /// Try to create a high-resolution waitable timer. Returns
    /// `None` if the OS rejects the request (very old Win10 builds
    /// without HR timer support, or insufficient privileges). The
    /// caller falls back to `sleep_until` in that case.
    pub(super) fn try_new() -> Option<Self> {
        use windows_sys::Win32::System::Threading::{
            CreateWaitableTimerExW, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS,
        };
        // SAFETY: all pointers are null (no attributes, no name),
        // the flag and access constants come straight from the
        // `windows-sys` binding. Return value is an `HANDLE` which
        // is null on failure.
        let handle = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        if handle.is_null() {
            log::warn!(
                "clock: CreateWaitableTimerExW(HIGH_RESOLUTION) failed; \
                 falling back to hybrid sleep+spin"
            );
            None
        } else {
            log::info!("clock: using high-resolution waitable timer");
            Some(WaitableTimer { handle })
        }
    }

    /// Block the current thread until `deadline` passes, polling
    /// `stop` on every wakeup so shutdown latency is bounded by the
    /// remaining wait (never more than one tick). A short `Instant`
    /// spin after the wait compensates for the ≤few-microsecond
    /// variance in `WaitForSingleObject` wake timing.
    pub(super) fn wait_until(&self, deadline: Instant, stop: &AtomicBool) {
        use windows_sys::Win32::System::Threading::{SetWaitableTimer, WaitForSingleObject};
        loop {
            let now = Instant::now();
            let remaining = deadline.saturating_duration_since(now);
            if remaining.is_zero() {
                return;
            }
            // Convert to negative 100-ns units (Windows relative
            // due time convention). Clamp to i64::MIN/2 so the
            // multiplication can't overflow for absurd durations.
            let hundreds_ns: i64 = remaining.as_nanos().min(i64::MAX as u128 / 10) as i64 / 100;
            let due: i64 = -hundreds_ns.max(1);
            // SAFETY: `self.handle` is a valid waitable timer
            // handle; `&due` is a valid `*const i64`; null callback
            // and context are documented no-op choices; `fresume`
            // is FALSE (0) - we don't care about system suspend.
            let arm_ok = unsafe {
                SetWaitableTimer(
                    self.handle,
                    &due as *const i64,
                    0,
                    None,
                    std::ptr::null(),
                    0,
                )
            };
            if arm_ok == 0 {
                // Arming failed - fall back to a short sleep so we
                // don't busy-spin a whole period.
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            // Compute a bounded ms timeout for `WaitForSingleObject`
            // as a safety net: the timer signals the handle, but
            // clamp the blocking wait to `remaining + 1ms` so a
            // missed signal never hangs the thread.
            let wait_ms: u32 = remaining
                .as_millis()
                .saturating_add(1)
                .min(u32::MAX as u128) as u32;
            // SAFETY: `self.handle` is valid; `wait_ms` is a u32
            // matching the Win32 contract. Return value encodes
            // signalled / timeout / error - we don't differentiate
            // here because the deadline check on the next iteration
            // is the source of truth.
            unsafe {
                WaitForSingleObject(self.handle, wait_ms);
            }
            if stop.load(Ordering::Acquire) {
                return;
            }
            // Sub-microsecond spin tail to absorb the 1–2 us wake
            // jitter of WaitForSingleObject. Cheap: at most one
            // loop iteration in practice.
            let now = Instant::now();
            if now >= deadline {
                return;
            }
            if deadline.saturating_duration_since(now) < Duration::from_micros(200) {
                while Instant::now() < deadline {
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    std::hint::spin_loop();
                }
                return;
            }
            // Otherwise loop: we woke early, re-arm.
        }
    }
}

#[cfg(windows)]
impl Drop for WaitableTimer {
    fn drop(&mut self) {
        // SAFETY: `self.handle` was produced by a successful
        // `CreateWaitableTimerExW` call in `try_new`; it has not
        // been closed yet because we only `Drop` once.
        unsafe {
            let _ = windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

/// Threshold below which `sleep_until` busy-waits instead of sleeping.
/// Picked so the spin covers one multimedia-timer quantum plus slack
/// for `Sleep` wakeup variance. At 120 BPM the clock spins for ~1.5 ms
/// out of every 20.83 ms tick - ~7% duty on one core during playback.
const SPIN_THRESHOLD: Duration = Duration::from_micros(1500);

/// Sleep until `deadline` with hard sub-ms accuracy: coarse
/// `thread::sleep` for the bulk of the wait, then an `Instant::now()`
/// spin for the final sub-millisecond. Checking `stop` inside the
/// spin keeps shutdown latency at a tick period worst-case.
pub(super) fn sleep_until(deadline: Instant, stop: &AtomicBool) {
    loop {
        let now = Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            return;
        }
        if remaining <= SPIN_THRESHOLD {
            // Busy-wait the last stretch. Polling `Instant::now()` is
            // cheap (QueryPerformanceCounter on Windows); the loop
            // still yields often enough that the OS sees us as busy
            // but not pathological.
            while Instant::now() < deadline {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                std::hint::spin_loop();
            }
            return;
        }
        // Sleep just short of the spin threshold so we always land
        // inside the spin on the next iteration. `- SPIN_THRESHOLD`
        // cannot underflow because we checked `remaining > SPIN_THRESHOLD`.
        thread::sleep(remaining - SPIN_THRESHOLD);
        if stop.load(Ordering::Acquire) {
            return;
        }
    }
}
