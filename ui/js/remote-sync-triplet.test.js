import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    applyRemoteTripletCommand,
    topToolbarTripletTargets,
} from './remote-sync-triplet.js';

function stateStub({ count = 4, checked = [], triplets = [] } = {}) {
    const values = Array.from({ length: count }, (_, idx) => triplets[idx] === true);
    const calls = [];
    return {
        calls,
        values,
        getCheckedArray: () => checked.slice(),
        getPatternCount: () => count,
        setTripletBulk: (targets, value) => {
            calls.push({ targets: targets.slice(), value });
            for (const idx of targets) {
                if (idx >= 0 && idx < values.length) values[idx] = !!value;
            }
        },
        morphSend: null,
        setTripletMorphSend(value) { this.morphSend = !!value; },
    };
}

/** Records the projection press the toolbar would have run. */
function pressStub() {
    const presses = [];
    const fn = (targets, next, useMorph) => {
        presses.push({ targets: targets.slice(), next, useMorph });
    };
    fn.presses = presses;
    return fn;
}

test('topToolbarTripletTargets uses all patterns when no rows are checked', () => {
    const state = stateStub({ count: 4 });
    assert.deepEqual(topToolbarTripletTargets(state), [0, 1, 2, 3]);
});

test('topToolbarTripletTargets uses checked rows when present', () => {
    const state = stateStub({ count: 4, checked: [2, 0] });
    assert.deepEqual(topToolbarTripletTargets(state), [2, 0]);
});

test('applyRemoteTripletCommand sets triplet on without toggling', () => {
    const state = stateStub({ count: 3, triplets: [false, true, false] });
    assert.equal(applyRemoteTripletCommand({ command: 'triplet', triplet: true }, state), true);
    assert.deepEqual(state.calls, [{ targets: [0, 1, 2], value: true }]);
    assert.deepEqual(state.values, [true, true, true]);
});

test('applyRemoteTripletCommand accepts already matching off state', () => {
    const state = stateStub({ count: 2, triplets: [false, false] });
    assert.equal(applyRemoteTripletCommand({ command: 'triplet', triplet: false }, state), true);
    assert.deepEqual(state.values, [false, false]);
});

test('applyRemoteTripletCommand ignores missing boolean value', () => {
    const state = stateStub({ count: 2 });
    assert.equal(applyRemoteTripletCommand({ command: 'triplet' }, state), false);
    assert.deepEqual(state.calls, []);
});

// The bug this guards: the master wrote a 12-step morph projection while the
// peer only flipped the device's own triplet flag, so the two played
// different patterns. The press has to be mirrored, not the flag.
test('a morph press runs the projection on the peer too', () => {
    const state = stateStub({ count: 3 });
    const press = pressStub();
    const applied = applyRemoteTripletCommand(
        { command: 'triplet', triplet: true, morph: true }, state, press,
    );
    assert.equal(applied, true);
    assert.deepEqual(press.presses, [{ targets: [0, 1, 2], next: true, useMorph: true }]);
    assert.deepEqual(state.calls, [], 'the flag-only path must not also run');
});

test('an async projection settles before the remote command completes', async () => {
    const state = stateStub({ count: 1 });
    let finishProjection;
    const press = () => new Promise((resolve) => { finishProjection = resolve; });

    const pending = applyRemoteTripletCommand(
        { command: 'triplet', triplet: true, morph: true }, state, press,
    );
    assert.equal(typeof pending?.then, 'function', 'the async projection must be returned');

    let completed = false;
    pending.then(() => { completed = true; });
    await Promise.resolve();
    assert.equal(completed, false, 'the command remains in flight with its projection');

    finishProjection();
    assert.equal(await pending, true);
    assert.equal(completed, true);
});

test('a plain press still only sets the flag', () => {
    const state = stateStub({ count: 2 });
    const press = pressStub();
    applyRemoteTripletCommand({ command: 'triplet', triplet: true, morph: false }, state, press);
    assert.deepEqual(press.presses, [], 'no projection without morph');
    assert.deepEqual(state.calls, [{ targets: [0, 1], value: true }]);
});

test('triplet off restores a projection after MORPH was unchecked', async () => {
    const state = stateStub({ count: 2, triplets: [true, true] });
    const press = pressStub();

    const applied = await applyRemoteTripletCommand(
        { command: 'triplet', triplet: false, morph: false }, state, press,
    );

    assert.equal(applied, true);
    assert.deepEqual(press.presses, [{ targets: [0, 1], next: false, useMorph: false }]);
    assert.deepEqual(state.calls, [], 'the restore-capable press owns TRIPLET OFF');
});

test('the peer MORPH box follows the mode the pattern was written with', () => {
    const state = stateStub({ count: 2 });
    applyRemoteTripletCommand({ command: 'triplet', triplet: true, morph: true }, state, pressStub());
    assert.equal(state.morphSend, true);

    const other = stateStub({ count: 2 });
    applyRemoteTripletCommand({ command: 'triplet', triplet: true, morph: false }, other, pressStub());
    assert.equal(other.morphSend, false);
});

// A peer on an older build sends no `morph`, and must keep behaving the way
// it always did rather than silently morphing.
test('a command with no morph field falls back to the flag', () => {
    const state = stateStub({ count: 2 });
    const press = pressStub();
    applyRemoteTripletCommand({ command: 'triplet', triplet: true }, state, press);
    assert.deepEqual(press.presses, []);
    assert.deepEqual(state.calls, [{ targets: [0, 1], value: true }]);
});

test('a morph press with no projection available still sets the flag', () => {
    const state = stateStub({ count: 2 });
    applyRemoteTripletCommand({ command: 'triplet', triplet: true, morph: true }, state, null);
    assert.deepEqual(state.calls, [{ targets: [0, 1], value: true }],
        'better the plain flag than nothing at all');
});
