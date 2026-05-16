// Usage: node ui/js/shared/pointer-activation.test.js

import { bindPointerPressActivation } from './pointer-activation.js';

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
    } catch (err) {
        console.error(`  FAIL: ${name}: ${err.stack || err.message}`);
        failed += 1;
    }
}

function fakeButton() {
    const listeners = new Map();
    return {
        addEventListener(type, listener) {
            listeners.set(type, listener);
        },
        dispatch(type, event = {}) {
            const listener = listeners.get(type);
            if (listener) listener(event);
        },
    };
}

console.log('pointer-activation tests:');

test('primary pointerdown activates immediately and release click is ignored', () => {
    const button = fakeButton();
    let calls = 0;
    bindPointerPressActivation(button, () => {
        calls += 1;
    });

    button.dispatch('pointerdown', { button: 0 });
    assert(calls === 1, 'pointerdown activates once');

    button.dispatch('click', { button: 0, detail: 1 });
    assert(calls === 1, 'follow-up pointer click is ignored');
});

test('keyboard click activates without a pointer press', () => {
    const button = fakeButton();
    let calls = 0;
    bindPointerPressActivation(button, () => {
        calls += 1;
    });

    button.dispatch('click', { button: 0, detail: 0 });
    assert(calls === 1, 'keyboard click activates once');
});

test('non-primary pointer input does not activate', () => {
    const button = fakeButton();
    let calls = 0;
    bindPointerPressActivation(button, () => {
        calls += 1;
    });

    button.dispatch('pointerdown', { button: 2 });
    button.dispatch('click', { button: 2, detail: 1 });
    assert(calls === 0, 'right pointer input is ignored');
});

test('keyboard click still works after a pointer press without click release', () => {
    const button = fakeButton();
    let calls = 0;
    bindPointerPressActivation(button, () => {
        calls += 1;
    });

    button.dispatch('pointerdown', { button: 0 });
    button.dispatch('click', { button: 0, detail: 0 });
    assert(calls === 2, 'keyboard click is not swallowed by stale pointer state');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
