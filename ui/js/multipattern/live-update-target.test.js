import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLiveUpdateTargetIndex } from './live-update-target.js';

test('live update target uses focused pattern when nothing is checked', () => {
    assert.equal(resolveLiveUpdateTargetIndex([], 2, 4), 2);
});

test('live update target keeps focused pattern when it is checked', () => {
    assert.equal(resolveLiveUpdateTargetIndex([1, 3], 3, 5), 3);
});

test('live update target falls back to first checked pattern when focus is outside checks', () => {
    assert.equal(resolveLiveUpdateTargetIndex([4, 1, 3], 0, 6), 1);
});

test('live update target rejects invalid focused and checked indexes', () => {
    assert.equal(resolveLiveUpdateTargetIndex([-1, 9], 8, 3), -1);
    assert.equal(resolveLiveUpdateTargetIndex(null, 8, 3), -1);
    assert.equal(resolveLiveUpdateTargetIndex([2, 99], null, 3), 2);
});
