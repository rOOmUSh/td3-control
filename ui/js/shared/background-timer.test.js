import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createBackgroundTimer } from './background-timer.js';

class FakeWorker {
    static instances = [];

    constructor(url) {
        this.url = url;
        this.messages = [];
        this.onmessage = null;
        this.onerror = null;
        this.terminated = false;
        FakeWorker.instances.push(this);
    }

    postMessage(message) {
        this.messages.push(message);
    }

    fire(id) {
        this.onmessage?.({ data: { type: 'fire', id } });
    }

    terminate() {
        this.terminated = true;
    }
}

class FakeVisibilityTarget {
    constructor() {
        this.hidden = false;
        this.listeners = new Set();
    }

    addEventListener(type, listener) {
        if (type === 'visibilitychange') this.listeners.add(listener);
    }

    removeEventListener(type, listener) {
        if (type === 'visibilitychange') this.listeners.delete(listener);
    }

    setHidden(hidden) {
        this.hidden = hidden;
        for (const listener of this.listeners) listener();
    }
}

function makeWorkerTimer() {
    FakeWorker.instances = [];
    const revoked = [];
    const timer = createBackgroundTimer({
        WorkerCtor: FakeWorker,
        BlobCtor: class FakeBlob {
            constructor(parts, options) {
                this.parts = parts;
                this.options = options;
            }
        },
        createObjectURL: () => 'blob:test-background-timer',
        revokeObjectURL: (url) => revoked.push(url),
        isBackgrounded: () => true,
        now: () => 1_000,
    });
    return { timer, revoked };
}

test('background timer schedules callbacks through a dedicated worker', () => {
    const { timer } = makeWorkerTimer();
    let calls = 0;

    timer.setTimeout(() => { calls += 1; }, 125);

    const worker = FakeWorker.instances[0];
    assert.ok(worker);
    assert.equal(worker.url, 'blob:test-background-timer');
    assert.deepEqual(worker.messages, [{ type: 'set', id: 1, delayMs: 125 }]);
    assert.equal(calls, 0);

    worker.fire(1);
    worker.fire(1);
    assert.equal(calls, 1);
});

test('visible timers use the native clock and migrate to a worker when hidden', () => {
    FakeWorker.instances = [];
    const visibility = new FakeVisibilityTarget();
    const scheduled = [];
    const cleared = [];
    let calls = 0;
    const timer = createBackgroundTimer({
        WorkerCtor: FakeWorker,
        BlobCtor: class FakeBlob {},
        createObjectURL: () => 'blob:test-visibility-timer',
        revokeObjectURL: () => {},
        visibilityTarget: visibility,
        isBackgrounded: () => visibility.hidden,
        setTimeoutFn(fn, delayMs) {
            scheduled.push({ fn, delayMs });
            return 91;
        },
        clearTimeoutFn(handle) {
            cleared.push(handle);
        },
        now: () => 1_000,
    });

    timer.setTimeout(() => { calls += 1; }, 125);
    assert.equal(scheduled.length, 1);
    assert.equal(FakeWorker.instances.length, 0);

    visibility.setHidden(true);
    assert.deepEqual(cleared, [91]);
    assert.equal(FakeWorker.instances.length, 1);
    assert.deepEqual(FakeWorker.instances[0].messages, [
        { type: 'set', id: 1, delayMs: 125 },
    ]);

    scheduled[0].fn();
    assert.equal(calls, 0);
    FakeWorker.instances[0].fire(1);
    assert.equal(calls, 1);
});

test('background timer cancellation removes a pending worker callback', () => {
    const { timer } = makeWorkerTimer();
    let calls = 0;

    const handle = timer.setTimeout(() => { calls += 1; }, 50);
    const worker = FakeWorker.instances[0];
    timer.clearTimeout(handle);
    worker.fire(1);

    assert.deepEqual(worker.messages, [
        { type: 'set', id: 1, delayMs: 50 },
        { type: 'clear', id: 1 },
    ]);
    assert.equal(calls, 0);
});

test('background timer falls back to native timers when workers are unavailable', () => {
    const scheduled = [];
    const cleared = [];
    let calls = 0;
    const timer = createBackgroundTimer({
        WorkerCtor: null,
        BlobCtor: null,
        createObjectURL: null,
        setTimeoutFn(fn, delayMs) {
            scheduled.push({ fn, delayMs });
            return 81;
        },
        clearTimeoutFn(handle) {
            cleared.push(handle);
        },
    });

    const handle = timer.setTimeout(() => { calls += 1; }, 75);
    timer.clearTimeout(handle);
    scheduled[0].fn();

    assert.equal(scheduled.length, 1);
    assert.equal(scheduled[0].delayMs, 75);
    assert.deepEqual(cleared, [81]);
    assert.equal(calls, 0);
});

test('background timer disposes its worker and cancels pending callbacks', () => {
    const { timer, revoked } = makeWorkerTimer();
    let calls = 0;

    timer.setTimeout(() => { calls += 1; }, 25);
    const worker = FakeWorker.instances[0];
    timer.dispose();
    worker.fire(1);

    assert.equal(worker.terminated, true);
    assert.deepEqual(revoked, ['blob:test-background-timer']);
    assert.equal(calls, 0);
});
