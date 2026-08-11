import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    cycleAnchorPulseIndex,
    delayToNextStep,
    elapsedStepBoundaries,
    nextStepInCycle,
    phasePreservingStepDelay,
    preloadStep,
    pulsesPerStep,
    stepAtPulse,
    stepSyncFromCycle,
    stepSyncFromPulse,
    stepSyncFromTransportStart,
} from './transport-sync-timing.js';

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

test('delayToNextStep prefers the acknowledged pulse epoch when available', () => {
    const startSync = {
        startedAtEpochMs: 1000,
        effectiveAtEpochMicros: 1_000_500,
    };
    assert.equal(delayToNextStep(startSync, 125, 1063), 62.5);
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

test('stepAtPulse uses TD-3 normal and triplet pulse boundaries', () => {
    assert.equal(pulsesPerStep(false), 6);
    assert.equal(stepAtPulse(0, 0, 16, false), 0);
    assert.equal(stepAtPulse(5, 0, 16, false), 0);
    assert.equal(stepAtPulse(6, 0, 16, false), 1);
    assert.equal(stepAtPulse(7, 0, 16, true), 0);
    assert.equal(stepAtPulse(8, 0, 16, true), 1);
});

test('stepAtPulse honors a cumulative pulse anchor and wraps active steps', () => {
    assert.equal(stepAtPulse(100, 100, 4, false), 0);
    assert.equal(stepAtPulse(123, 100, 4, false), 3);
    assert.equal(stepAtPulse(124, 100, 4, false), 0);
    assert.equal(stepAtPulse(164, 100, 8, true), 0);
});

test('stepSyncFromPulse derives the current step and next normal boundary', () => {
    const sync = stepSyncFromPulse({
        pulseIndex: 0,
        pulseEpochMicros: 1_000_000,
        anchorPulseIndex: 0,
        centibpm: 12_000,
        activeSteps: 16,
        triplet: false,
    }, 1_062_500);

    assert.equal(sync.tickPeriodMicros, 20_833);
    assert.equal(sync.currentPulseIndex, 3);
    assert.equal(sync.step, 0);
    assert.equal(sync.nextStepPulseIndex, 6);
    assert.equal(sync.nextStepEpochMicros, 1_124_998);
    assert.equal(sync.delayMs, 62.498);
});

test('stepSyncFromPulse advances exactly on normal and triplet boundaries', () => {
    const normal = stepSyncFromPulse({
        pulseIndex: 6,
        pulseEpochMicros: 2_000_000,
        centibpm: 12_000,
        activeSteps: 16,
        triplet: false,
    }, 2_000_000);
    assert.equal(normal.step, 1);
    assert.equal(normal.nextStepPulseIndex, 12);

    const triplet = stepSyncFromPulse({
        pulseIndex: 8,
        pulseEpochMicros: 2_000_000,
        centibpm: 12_000,
        activeSteps: 16,
        triplet: true,
    }, 2_000_000);
    assert.equal(triplet.step, 1);
    assert.equal(triplet.nextStepPulseIndex, 16);
});

test('stepSyncFromPulse wraps from the last active step to step zero', () => {
    const sync = stepSyncFromPulse({
        pulseIndex: 24,
        pulseEpochMicros: 3_000_000,
        anchorPulseIndex: 0,
        centibpm: 12_000,
        activeSteps: 4,
        triplet: false,
    }, 3_000_000);
    assert.equal(sync.step, 0);
    assert.equal(sync.nextStepPulseIndex, 30);
});

test('stepSyncFromTransportStart projects delayed normal playback', () => {
    const sync = stepSyncFromTransportStart({
        startSync: {
            centibpm: 12_000,
            effectivePulseIndex: 0,
            effectiveAtEpochMicros: 1_000_000,
        },
        bpm: 120,
        activeSteps: 16,
        triplet: false,
    }, 1_312_500);
    assert.equal(sync.step, 2);
    assert.equal(sync.currentPulseIndex, 15);
    assert.equal(sync.nextStepPulseIndex, 18);
    assert.equal(sync.nextStepEpochMicros, 1_374_994);
});

test('stepSyncFromTransportStart honors triplet and explicit resume anchors', () => {
    const sync = stepSyncFromTransportStart({
        startSync: {
            centibpm: 12_000,
            effectivePulseIndex: 45,
            effectiveAtEpochMicros: 2_000_000,
            anchorPulseIndex: 32,
        },
        bpm: 120,
        activeSteps: 8,
        triplet: true,
    }, 2_000_000);
    assert.equal(sync.step, 1);
    assert.equal(sync.currentPulseIndex, 45);
    assert.equal(sync.nextStepPulseIndex, 48);
});

test('stepSyncFromTransportStart requires an applied pulse acknowledgement', () => {
    assert.equal(stepSyncFromTransportStart({
        startSync: { startedAtEpochMs: 1000 },
        bpm: 120,
        activeSteps: 16,
        triplet: false,
    }, 1_000_000), null);
});

test('cycleAnchorPulseIndex chooses the current normal and triplet cycle', () => {
    assert.equal(cycleAnchorPulseIndex(215, 16, false), 192);
    assert.equal(cycleAnchorPulseIndex(215, 8, true), 192);
    assert.equal(cycleAnchorPulseIndex(23, 4, false), 0);
    assert.equal(cycleAnchorPulseIndex(24, 4, false), 24);
    assert.equal(cycleAnchorPulseIndex(173, 8, false, 50), 146);
});

test('stepSyncFromCycle projects host audition phase across cycle wraps', () => {
    const sync = stepSyncFromCycle({
        cycleEpochMicros: 1_000_000,
        cyclePeriodMicros: 1_000_000,
        activeSteps: 8,
    }, 2_312_500);
    assert.equal(sync.step, 2);
    assert.equal(sync.stepPeriodMicros, 125_000);
    assert.equal(sync.nextStepEpochMicros, 2_375_000);
    assert.equal(sync.delayMs, 62.5);
});

test('phasePreservingStepDelay keeps fractional progress through a tempo edit', () => {
    assert.equal(phasePreservingStepDelay(250, 125, 1_250, 1_125), 62.5);
    assert.equal(phasePreservingStepDelay(250, 125, 1_250, 1_000), 125);
    assert.equal(phasePreservingStepDelay(250, 125, 1_250, 1_250), 0);
    assert.equal(phasePreservingStepDelay(0, 125, 1_250, 1_125), 125);
});

test('elapsedStepBoundaries preserves absolute timer cadence and detects stalls', () => {
    assert.equal(elapsedStepBoundaries(1_000, 125, 1_000), 1);
    assert.equal(elapsedStepBoundaries(1_000, 125, 1_124), 1);
    assert.equal(elapsedStepBoundaries(1_000, 125, 1_125), 2);
    assert.equal(elapsedStepBoundaries(1_000, 125, 1_380), 4);
    assert.equal(elapsedStepBoundaries(1_000, 0, 1_380), 1);
});
