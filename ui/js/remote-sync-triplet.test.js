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
    };
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
