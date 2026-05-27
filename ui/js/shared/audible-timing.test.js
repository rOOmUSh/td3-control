import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    adoptQueuedTiming,
    playbackTiming,
    snapshotTiming,
} from './audible-timing.js';

test('snapshotTiming clamps active steps and preserves triplet', () => {
    assert.deepEqual(
        snapshotTiming({ active_steps: 20, triplet: true }),
        { activeSteps: 16, triplet: true },
    );
    assert.deepEqual(
        snapshotTiming({ active_steps: 0, triplet: false }),
        { activeSteps: 1, triplet: false },
    );
});

test('snapshotTiming uses fallback for missing pattern fields', () => {
    assert.deepEqual(
        snapshotTiming(null, { activeSteps: 10, triplet: true }),
        { activeSteps: 10, triplet: true },
    );
});

test('playbackTiming uses audible snapshot only for live update device playback', () => {
    const audibleTiming = { activeSteps: 16, triplet: false };
    const fallbackTiming = { activeSteps: 10, triplet: true };
    assert.equal(
        playbackTiming({
            liveUpdate: true,
            auditionMode: false,
            audibleTiming,
            fallbackTiming,
        }),
        audibleTiming,
    );
});

test('playbackTiming uses edited timing during no-save audition', () => {
    const audibleTiming = { activeSteps: 16, triplet: false };
    const fallbackTiming = { activeSteps: 10, triplet: true };
    assert.equal(
        playbackTiming({
            liveUpdate: false,
            auditionMode: true,
            audibleTiming,
            fallbackTiming,
        }),
        fallbackTiming,
    );
});

test('adoptQueuedTiming keeps current timing when no queued timing exists', () => {
    const current = { activeSteps: 12, triplet: false };
    assert.equal(adoptQueuedTiming(current, null), current);
});

test('adoptQueuedTiming adopts queued timing at wrap', () => {
    const current = { activeSteps: 16, triplet: false };
    const queued = { activeSteps: 10, triplet: true };
    assert.equal(adoptQueuedTiming(current, queued), queued);
});
