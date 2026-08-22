// Usage: node ui/js/shared/trailing-throttle.test.js

import { createTrailingThrottle } from './trailing-throttle.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`  FAIL: ${message}`);
        failed += 1;
        return;
    }
    passed += 1;
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

// Deterministic clock and timer queue so the tests never sleep.
function fakeClock() {
    let time = 0;
    const timers = [];
    let nextId = 1;
    return {
        now: () => time,
        setTimer(fn, delay) {
            const id = nextId;
            nextId += 1;
            timers.push({ id, at: time + delay, fn });
            return id;
        },
        clearTimer(id) {
            const index = timers.findIndex((t) => t.id === id);
            if (index >= 0) timers.splice(index, 1);
        },
        advance(ms) {
            const target = time + ms;
            for (;;) {
                timers.sort((a, b) => a.at - b.at);
                const next = timers[0];
                if (!next || next.at > target) break;
                timers.shift();
                time = next.at;
                next.fn();
            }
            time = target;
        },
        pending() { return timers.length; },
    };
}

function fixture(intervalMs = 30) {
    const clock = fakeClock();
    const sent = [];
    const send = createTrailingThrottle((value) => sent.push([clock.now(), value]), intervalMs, {
        setTimer: clock.setTimer,
        clearTimer: clock.clearTimer,
        now: clock.now,
    });
    return { clock, sent, send };
}

console.log('trailing-throttle tests:');

test('the first call in a quiet period sends immediately', () => {
    const { sent, send } = fixture();
    send(10);
    assert(sent.length === 1 && sent[0][1] === 10, 'sent at once');
});

test('a burst collapses to the first value now and the last value later', () => {
    const { clock, sent, send } = fixture(30);
    send(1);
    clock.advance(5);
    send(2);
    clock.advance(5);
    send(3);
    clock.advance(5);
    send(4);
    assert(sent.length === 1, 'intermediate values are not sent during the interval');
    clock.advance(30);
    assert(sent.length === 2, 'one trailing send');
    assert(sent[1][1] === 4, 'trailing send carries the final value');
    assert(sent[1][0] === 30, 'trailing send lands when the interval elapses');
    assert(clock.pending() === 0, 'no timer left behind');
});

test('a call after the interval sends immediately again', () => {
    const { clock, sent, send } = fixture(30);
    send(1);
    clock.advance(31);
    send(2);
    assert(sent.length === 2 && sent[1][1] === 2, 'second quiet-period call is immediate');
});

test('cancel drops the pending trailing send', () => {
    const { clock, sent, send } = fixture(30);
    send(1);
    send(2);
    send.cancel();
    clock.advance(100);
    assert(sent.length === 1, 'pending value discarded');
    assert(clock.pending() === 0, 'timer cleared');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
