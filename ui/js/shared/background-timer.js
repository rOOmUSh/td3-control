const WORKER_SOURCE = `
const timers = new Map();

self.onmessage = ({ data }) => {
    if (!data || !Number.isInteger(data.id)) return;

    if (data.type === 'clear') {
        const timer = timers.get(data.id);
        if (timer !== undefined) clearTimeout(timer);
        timers.delete(data.id);
        return;
    }

    if (data.type !== 'set') return;
    const existing = timers.get(data.id);
    if (existing !== undefined) clearTimeout(existing);
    const delayMs = Number.isFinite(data.delayMs) ? Math.max(0, data.delayMs) : 0;
    const timer = setTimeout(() => {
        timers.delete(data.id);
        self.postMessage({ type: 'fire', id: data.id });
    }, delayMs);
    timers.set(data.id, timer);
};
`;

function dependency(options, name, fallback) {
    return Object.prototype.hasOwnProperty.call(options, name) ? options[name] : fallback;
}

function normalizedDelay(delayMs) {
    try {
        const numeric = Number(delayMs);
        return Number.isFinite(numeric) ? Math.max(0, numeric) : 0;
    } catch (_) {
        return 0;
    }
}

/**
 * Create an opaque timeout scheduler that uses the page clock while visible
 * and migrates pending deadlines to a dedicated worker while backgrounded.
 * Worker timers avoid Chromium's chained main-window timer throttling without
 * taking visible-page timing away from browser clock and test APIs.
 */
export function createBackgroundTimer(options = {}) {
    const WorkerCtor = dependency(options, 'WorkerCtor', globalThis.Worker);
    const BlobCtor = dependency(options, 'BlobCtor', globalThis.Blob);
    const urlApi = dependency(options, 'urlApi', globalThis.URL);
    const createObjectURL = dependency(
        options,
        'createObjectURL',
        typeof urlApi?.createObjectURL === 'function'
            ? urlApi.createObjectURL.bind(urlApi)
            : null,
    );
    const revokeObjectURL = dependency(
        options,
        'revokeObjectURL',
        typeof urlApi?.revokeObjectURL === 'function'
            ? urlApi.revokeObjectURL.bind(urlApi)
            : null,
    );
    const setTimeoutFn = dependency(
        options,
        'setTimeoutFn',
        typeof globalThis.setTimeout === 'function'
            ? globalThis.setTimeout.bind(globalThis)
            : null,
    );
    const clearTimeoutFn = dependency(
        options,
        'clearTimeoutFn',
        typeof globalThis.clearTimeout === 'function'
            ? globalThis.clearTimeout.bind(globalThis)
            : null,
    );
    const now = dependency(options, 'now', Date.now);
    const visibilityTarget = dependency(options, 'visibilityTarget', globalThis.document ?? null);
    const isBackgrounded = dependency(
        options,
        'isBackgrounded',
        () => visibilityTarget?.hidden === true,
    );

    let worker = null;
    let workerObjectUrl = null;
    let workerDisabled = false;
    let disposed = false;
    let nextWorkerTimerId = 1;
    const workerTimers = new Map();
    const nativeTimers = new Set();

    function releaseWorker() {
        const currentWorker = worker;
        worker = null;
        if (currentWorker) {
            currentWorker.onmessage = null;
            currentWorker.onerror = null;
            try {
                currentWorker.terminate();
            } catch (_) {
                // The worker is already unavailable.
            }
        }
        if (workerObjectUrl && typeof revokeObjectURL === 'function') {
            try {
                revokeObjectURL(workerObjectUrl);
            } catch (_) {
                // Revocation is cleanup only.
            }
        }
        workerObjectUrl = null;
    }

    function scheduleNative(timer, delayMs) {
        if (typeof setTimeoutFn !== 'function') {
            timer.active = false;
            throw new Error('No timeout scheduler is available');
        }
        timer.source = 'native';
        timer.generation += 1;
        const generation = timer.generation;
        const callback = () => {
            nativeTimers.delete(timer);
            if (!timer.active
                || timer.source !== 'native'
                || timer.generation !== generation) {
                return;
            }
            timer.active = false;
            timer.callback();
        };
        timer.nativeHandle = setTimeoutFn(callback, delayMs);
        nativeTimers.add(timer);
    }

    function disableWorker() {
        workerDisabled = true;
        const pending = [...workerTimers.values()];
        workerTimers.clear();
        releaseWorker();
        const nowMs = now();
        for (const timer of pending) {
            if (!timer.active) continue;
            scheduleNative(timer, Math.max(0, timer.deadlineMs - nowMs));
        }
    }

    function fireWorkerTimer(id) {
        const timer = workerTimers.get(id);
        if (!timer || !timer.active) return;
        workerTimers.delete(id);
        timer.active = false;
        timer.callback();
    }

    function ensureWorker() {
        if (worker) return true;
        if (workerDisabled
            || typeof WorkerCtor !== 'function'
            || typeof BlobCtor !== 'function'
            || typeof createObjectURL !== 'function') {
            workerDisabled = true;
            return false;
        }

        let candidate = null;
        let candidateUrl = null;
        try {
            const blob = new BlobCtor([WORKER_SOURCE], { type: 'text/javascript' });
            candidateUrl = createObjectURL(blob);
            candidate = new WorkerCtor(candidateUrl);
            candidate.onmessage = ({ data }) => {
                if (data?.type === 'fire' && Number.isInteger(data.id)) {
                    fireWorkerTimer(data.id);
                }
            };
            candidate.onerror = (event) => {
                event?.preventDefault?.();
                disableWorker();
            };
            worker = candidate;
            workerObjectUrl = candidateUrl;
            return true;
        } catch (_) {
            try {
                candidate?.terminate();
            } catch (_) {
                // Worker setup has already failed.
            }
            if (candidateUrl && typeof revokeObjectURL === 'function') {
                try {
                    revokeObjectURL(candidateUrl);
                } catch (_) {
                    // Revocation is cleanup only.
                }
            }
            workerDisabled = true;
            return false;
        }
    }

    function pageIsBackgrounded() {
        try {
            return isBackgrounded() === true;
        } catch (_) {
            return false;
        }
    }

    function scheduleWorker(timer, delayMs) {
        if (!ensureWorker()) return false;
        timer.source = 'worker';
        timer.generation += 1;
        timer.id = nextWorkerTimerId;
        nextWorkerTimerId += 1;
        workerTimers.set(timer.id, timer);
        try {
            worker.postMessage({ type: 'set', id: timer.id, delayMs });
            return true;
        } catch (_) {
            disableWorker();
            return timer.source === 'native';
        }
    }

    function moveNativeTimersToWorker() {
        const pending = [...nativeTimers];
        nativeTimers.clear();
        for (const timer of pending) {
            if (typeof clearTimeoutFn === 'function') {
                clearTimeoutFn(timer.nativeHandle);
            }
            timer.source = null;
        }
        const nowMs = now();
        for (const timer of pending) {
            if (!timer.active) continue;
            const delayMs = Math.max(0, timer.deadlineMs - nowMs);
            if (!scheduleWorker(timer, delayMs) && timer.source !== 'native') {
                scheduleNative(timer, delayMs);
            }
        }
    }

    function moveWorkerTimersToNative() {
        const pending = [...workerTimers.entries()];
        workerTimers.clear();
        const nowMs = now();
        for (const [id, timer] of pending) {
            if (worker) {
                try {
                    worker.postMessage({ type: 'clear', id });
                } catch (_) {
                    disableWorker();
                }
            }
            if (!timer.active) continue;
            timer.source = null;
            scheduleNative(timer, Math.max(0, timer.deadlineMs - nowMs));
        }
    }

    function handleVisibilityChange() {
        if (disposed) return;
        if (pageIsBackgrounded()) {
            moveNativeTimersToWorker();
        } else {
            moveWorkerTimersToNative();
        }
    }

    function setBackgroundTimeout(callback, delayMs = 0) {
        if (typeof callback !== 'function') {
            throw new TypeError('Timeout callback must be a function');
        }
        if (disposed) {
            throw new Error('Background timer is disposed');
        }

        const delay = normalizedDelay(delayMs);
        const timer = {
            active: true,
            callback,
            deadlineMs: now() + delay,
            generation: 0,
            id: null,
            nativeHandle: null,
            source: null,
        };

        if (!pageIsBackgrounded() || !scheduleWorker(timer, delay)) {
            if (timer.source !== 'native') {
                // The caller's delay, not the deadline recomputed against a
                // second clock reading. Both describe the same instant, but
                // deriving one from the other loses whatever time passed in
                // between: on a slow machine a 75 ms timeout was scheduled
                // for 74. Later reschedules do need the deadline, because by
                // then time really has passed.
                scheduleNative(timer, delay);
            }
        }
        return timer;
    }

    function clearBackgroundTimeout(timer) {
        if (!timer || !timer.active) return;
        timer.active = false;

        if (timer.source === 'worker') {
            workerTimers.delete(timer.id);
            if (worker) {
                try {
                    worker.postMessage({ type: 'clear', id: timer.id });
                } catch (_) {
                    disableWorker();
                }
            }
            return;
        }

        nativeTimers.delete(timer);
        if (typeof clearTimeoutFn === 'function') {
            clearTimeoutFn(timer.nativeHandle);
        }
    }

    function dispose() {
        if (disposed) return;
        disposed = true;
        if (typeof visibilityTarget?.removeEventListener === 'function') {
            visibilityTarget.removeEventListener('visibilitychange', handleVisibilityChange);
        }
        for (const timer of workerTimers.values()) timer.active = false;
        workerTimers.clear();
        for (const timer of nativeTimers) {
            timer.active = false;
            if (typeof clearTimeoutFn === 'function') {
                clearTimeoutFn(timer.nativeHandle);
            }
        }
        nativeTimers.clear();
        releaseWorker();
    }

    if (typeof visibilityTarget?.addEventListener === 'function') {
        visibilityTarget.addEventListener('visibilitychange', handleVisibilityChange);
    }

    return {
        clearTimeout: clearBackgroundTimeout,
        dispose,
        setTimeout: setBackgroundTimeout,
    };
}

const sharedBackgroundTimer = createBackgroundTimer();

export function setBackgroundTimeout(callback, delayMs) {
    return sharedBackgroundTimer.setTimeout(callback, delayMs);
}

export function clearBackgroundTimeout(timer) {
    sharedBackgroundTimer.clearTimeout(timer);
}
