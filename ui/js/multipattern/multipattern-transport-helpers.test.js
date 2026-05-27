// Unit tests for the pure timeline helpers. Run with `node --test` from the
// `ui/js` directory:
//
//   cd ui/js && node --test multipattern/multipattern-transport-helpers.test.js
//
// Kept DOM-free so CI can exercise the loop math without a browser.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    firstTimelinePos,
    nextTimelinePos,
    advanceCursorToDevicePattern,
    needsImmediateScratchSave,
    shouldUpdateHostAuditionPattern,
    countNonEmpty,
    repeatFill,
    randomFill,
    hslForIndex,
} from './multipattern-transport-helpers.js';

test('firstTimelinePos finds the first non-empty slot', () => {
    assert.equal(firstTimelinePos([1, 2, 3]), 0);
    assert.equal(firstTimelinePos([0, 0, 2, 3]), 2);
    assert.equal(firstTimelinePos([0, 0, 0]), -1);
    assert.equal(firstTimelinePos([]), -1);
    assert.equal(firstTimelinePos(null), -1);
});

test('nextTimelinePos wraps past the end to the first slot', () => {
    assert.equal(nextTimelinePos([1, 2, 3, 4], 3), 0);
    assert.equal(nextTimelinePos([1, 2, 3, 4], 0), 1);
});

test('nextTimelinePos skips zero slots', () => {
    assert.equal(nextTimelinePos([1, 0, 0, 4], 0), 3);
    assert.equal(nextTimelinePos([1, 0, 0, 4], 3), 0);
});

test('nextTimelinePos returns -1 when timeline is all empty', () => {
    assert.equal(nextTimelinePos([0, 0, 0], 1), -1);
    assert.equal(nextTimelinePos([], 0), -1);
});

test('nextTimelinePos supports pattern numbers above 4 (N=64 range)', () => {
    assert.equal(nextTimelinePos([17, 64, 0, 12], 0), 1);
    assert.equal(nextTimelinePos([17, 64, 0, 12], 1), 3);
});

test('advanceCursorToDevicePattern finds next matching slot going forward', () => {
    // Timeline [1,2,3,2,1]; device is playing pattern idx 1 (pat# 2).
    // From pos 0 we expect pos 1 (first "2" after 0).
    assert.equal(advanceCursorToDevicePattern([1, 2, 3, 2, 1], 0, 1), 1);
    // From pos 1 we expect pos 3 (next "2").
    assert.equal(advanceCursorToDevicePattern([1, 2, 3, 2, 1], 1, 1), 3);
});

test('advanceCursorToDevicePattern wraps around to find a match', () => {
    // Timeline [2,3,3,3]; device playing pat# 2 (idx 1). From pos 2 we
    // search (2+1)%4=3 → 3, (2+2)%4=0 → 2 ✓, so we wrap to pos 0.
    assert.equal(advanceCursorToDevicePattern([2, 3, 3, 3], 2, 1), 0);
    // From pos 3, we wrap directly to pos 0.
    assert.equal(advanceCursorToDevicePattern([2, 3, 3, 3], 3, 1), 0);
});

test('advanceCursorToDevicePattern falls back to nextTimelinePos when pattern absent', () => {
    // Timeline [1,2,3,0]; device playing pat# 5 (idx 4) - not present.
    // Expect plain next-non-empty from pos 0, which is pos 1.
    assert.equal(advanceCursorToDevicePattern([1, 2, 3, 0], 0, 4), 1);
});

test('advanceCursorToDevicePattern falls back when devicePatIdx is null/negative', () => {
    assert.equal(advanceCursorToDevicePattern([1, 2, 3], 0, null), 1);
    assert.equal(advanceCursorToDevicePattern([1, 2, 3], 0, undefined), 1);
    assert.equal(advanceCursorToDevicePattern([1, 2, 3], 0, -1), 1);
});

test('advanceCursorToDevicePattern returns -1 on empty timeline', () => {
    assert.equal(advanceCursorToDevicePattern([], 0, 0), -1);
    assert.equal(advanceCursorToDevicePattern(null, 0, 0), -1);
});

test('advanceCursorToDevicePattern tolerates out-of-range from values', () => {
    // Out-of-range `from` should still find the match via wrap.
    assert.equal(advanceCursorToDevicePattern([1, 2, 3], 99, 1), 1);
    assert.equal(advanceCursorToDevicePattern([1, 2, 3], -5, 2), 2);
});

test('needsImmediateScratchSave fires on first-checkbox transition', () => {
    // Default [1,1,1,1] device playing P1, user checks P4 -> active tl=[4].
    // Cursor lands on slot 0 (the only checked slot). Scratch still holds
    // P1 (queuedPatIdx=0), so we must save P4.
    assert.equal(needsImmediateScratchSave([4], 0, 0), true);
});

test('needsImmediateScratchSave fires on last-uncheck transition (regression)', () => {
    // Checked [4] device playing P4 (queuedPatIdx=3 from the prior first-
    // check save), user unchecks P4 -> active tl=[1,1,1,1]. Cursor lands
    // on slot 0 (pat 1). Scratch still holds P4, so we must save P1 or the
    // device keeps looping P4 forever.
    assert.equal(needsImmediateScratchSave([1, 1, 1, 1], 0, 3), true);
});

test('needsImmediateScratchSave fires when playing pattern is removed mid-mode', () => {
    // Checked [1,4] device playing P4 in slot 1 (queuedPatIdx=3), user
    // unchecks P4 -> active tl=[1]. Cursor falls back to slot 0 (pat 1).
    // Scratch still has P4 from the pre-load, so save P1.
    assert.equal(needsImmediateScratchSave([1], 0, 3), true);
});

test('needsImmediateScratchSave is a no-op when cursor matches scratch', () => {
    // Add/drag/reorder cases where queued pattern still matches the cursor.
    assert.equal(needsImmediateScratchSave([1, 2], 0, 0), false);
    assert.equal(needsImmediateScratchSave([1, 2], 1, 1), false);
});

test('needsImmediateScratchSave forces save when scratch state is unknown', () => {
    // Transitioning into timeline mode from single-pattern: queuedPatternIdx
    // may be null until the seed runs. Force a save so the device adopts
    // the cursor's pattern on the next wrap.
    assert.equal(needsImmediateScratchSave([1, 2], 0, null), true);
    assert.equal(needsImmediateScratchSave([1, 2], 0, undefined), true);
});

test('needsImmediateScratchSave returns false on empty/invalid input', () => {
    assert.equal(needsImmediateScratchSave([], 0, 0), false);
    assert.equal(needsImmediateScratchSave(null, 0, 0), false);
    assert.equal(needsImmediateScratchSave([1, 2], -1, 0), false);
    assert.equal(needsImmediateScratchSave([1, 2], 5, 0), false);
    // Empty (zero) cursor slot: nothing to save.
    assert.equal(needsImmediateScratchSave([0, 1, 2], 0, 0), false);
});

test('shouldUpdateHostAuditionPattern updates only no-save pattern changes', () => {
    assert.equal(shouldUpdateHostAuditionPattern(false, true, true, 0, 1), true);
    assert.equal(shouldUpdateHostAuditionPattern(true, true, true, 0, 1), false);
    assert.equal(shouldUpdateHostAuditionPattern(false, false, true, 0, 1), false);
    assert.equal(shouldUpdateHostAuditionPattern(false, true, false, 0, 1), false);
    assert.equal(shouldUpdateHostAuditionPattern(false, true, true, 2, 2), false);
    assert.equal(shouldUpdateHostAuditionPattern(false, true, true, null, 1), false);
    assert.equal(shouldUpdateHostAuditionPattern(false, true, true, 1, -1), false);
});

test('countNonEmpty counts populated slots only', () => {
    assert.equal(countNonEmpty([1, 0, 2, 0, 3]), 3);
    assert.equal(countNonEmpty([]), 0);
    assert.equal(countNonEmpty([0, 0]), 0);
});

test('repeatFill repeats a source cyclically', () => {
    assert.deepEqual(repeatFill([1, 2, 3], 7), [1, 2, 3, 1, 2, 3, 1]);
    assert.deepEqual(repeatFill([5], 4), [5, 5, 5, 5]);
});

test('repeatFill yields zeros for an empty source', () => {
    assert.deepEqual(repeatFill([], 3), [0, 0, 0]);
    assert.deepEqual(repeatFill(null, 2), [0, 0]);
});

test('randomFill draws from 1..patternCount', () => {
    // Deterministic RNG: always returns 0.5 → every slot is (1 + floor(0.5 * N)).
    const rand = () => 0.5;
    assert.deepEqual(randomFill(4, 3, rand), [3, 3, 3]);
    assert.deepEqual(randomFill(8, 2, rand), [5, 5]);
});

test('randomFill returns zeros when patternCount <= 0', () => {
    assert.deepEqual(randomFill(0, 3, () => 0.5), [0, 0, 0]);
    assert.deepEqual(randomFill(-1, 2, () => 0.5), [0, 0]);
});

test('hslForIndex walks by the golden angle', () => {
    // Row 0 sits at hue 0; row 1 lands at the classic golden-angle 137.508°.
    assert.equal(hslForIndex(0), 'hsl(0, 70%, 55%)');
    assert.equal(hslForIndex(1), 'hsl(137.508, 70%, 55%)');
    // Saturation/lightness overrides are respected.
    assert.equal(hslForIndex(0, 55, 40), 'hsl(0, 55%, 40%)');
});
