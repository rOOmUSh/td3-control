// Trailing-edge throttle for knob-driven network sends.
//
// The first call in a quiet period fires immediately. Calls arriving
// within `intervalMs` of the last send are coalesced: only the most
// recent argument list is delivered, once the interval has elapsed. A
// drag therefore produces at most one send per interval and always ends
// on the final value.

export function createTrailingThrottle(fn, intervalMs, {
    setTimer = globalThis.setTimeout,
    clearTimer = globalThis.clearTimeout,
    now = () => Date.now(),
} = {}) {
    let lastSentAt = -Infinity;
    let pendingArgs = null;
    let timer = null;

    function flush() {
        timer = null;
        if (pendingArgs === null) return;
        const args = pendingArgs;
        pendingArgs = null;
        lastSentAt = now();
        fn(...args);
    }

    function call(...args) {
        const elapsed = now() - lastSentAt;
        if (timer === null && elapsed >= intervalMs) {
            lastSentAt = now();
            fn(...args);
            return;
        }
        pendingArgs = args;
        if (timer === null) {
            timer = setTimer(flush, Math.max(0, intervalMs - elapsed));
        }
    }

    call.cancel = () => {
        if (timer !== null) clearTimer(timer);
        timer = null;
        pendingArgs = null;
    };

    return call;
}
