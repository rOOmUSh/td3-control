import { test } from 'node:test';
import assert from 'node:assert/strict';
import { stepIntervalMs } from './transport-timing.js';

test('stepIntervalMs returns normal step duration', () => {
    assert.equal(stepIntervalMs(120, false), 125);
    assert.equal(stepIntervalMs(60, false), 250);
});

test('stepIntervalMs returns triplet step duration', () => {
    assert.equal(Math.round(stepIntervalMs(120, true) * 1000), 166667);
    assert.equal(stepIntervalMs(100, true), 200);
});

test('stepIntervalMs uses a bounded fallback for invalid BPM', () => {
    assert.equal(stepIntervalMs(0, false), 125);
    assert.equal(stepIntervalMs(Number.NaN, true), 60000 / (120 * 3));
});
