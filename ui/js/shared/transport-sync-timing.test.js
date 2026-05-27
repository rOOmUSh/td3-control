import { test } from 'node:test';
import assert from 'node:assert/strict';
import { delayToNextStep, nextStepInCycle, preloadStep } from './transport-sync-timing.js';

test('preloadStep clamps the configured save step into the active-step window', () => {
    assert.equal(preloadStep(16, 2), 2);
    assert.equal(preloadStep(4, 2), 2);
    assert.equal(preloadStep(2, 7), 1);
    assert.equal(preloadStep(16, 100), 15);
    assert.equal(preloadStep(16, 0), 1);
});

test('preloadStep falls back to a 16-step window when active steps are invalid', () => {
    assert.equal(preloadStep(undefined, 2), 2);
    assert.equal(preloadStep(Number.NaN, 20), 15);
});

test('delayToNextStep returns a full interval without a usable start sync', () => {
    assert.equal(delayToNextStep(null, 125, 1000), 125);
    assert.equal(delayToNextStep({}, 125, 1000), 125);
    assert.equal(delayToNextStep({ startedAtEpochMs: 0 }, 125, 1000), 125);
});

test('delayToNextStep aligns to the next step boundary', () => {
    const startSync = { startedAtEpochMs: 1000 };
    assert.equal(delayToNextStep(startSync, 125, 1001), 125);
    assert.equal(delayToNextStep(startSync, 125, 1060), 65);
    assert.equal(delayToNextStep(startSync, 125, 1124), 1);
    assert.equal(delayToNextStep(startSync, 125, 1125), 125);
    assert.equal(delayToNextStep(startSync, 125, 1126), 125);
});

test('delayToNextStep waits for a scheduled start before the next step', () => {
    assert.equal(delayToNextStep({ startedAtEpochMs: 2000 }, 125, 1000), 1125);
});

test('delayToNextStep returns zero for invalid intervals', () => {
    assert.equal(delayToNextStep({ startedAtEpochMs: 1000 }, 0, 1200), 0);
    assert.equal(delayToNextStep({ startedAtEpochMs: 1000 }, Number.NaN, 1200), 0);
});

test('nextStepInCycle advances inside active step window', () => {
    assert.deepEqual(nextStepInCycle(0, 16), { step: 1, wrapped: false });
    assert.deepEqual(nextStepInCycle(14, 16), { step: 15, wrapped: false });
});

test('nextStepInCycle wraps at active step count', () => {
    assert.deepEqual(nextStepInCycle(15, 16), { step: 0, wrapped: true });
    assert.deepEqual(nextStepInCycle(7, 8), { step: 0, wrapped: true });
});

test('nextStepInCycle falls back to 16 steps for invalid active step count', () => {
    assert.deepEqual(nextStepInCycle(15, 0), { step: 0, wrapped: true });
    assert.deepEqual(nextStepInCycle(8, Number.NaN), { step: 9, wrapped: false });
});
