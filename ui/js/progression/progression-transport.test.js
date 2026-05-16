// Tests for progression-transport.js pure helpers - runs with Node.js
// Usage: node ui/js/progression/progression-transport.test.js
//
// Self-contained: inlines the pure functions since they are not exported.

// --- Inline copies of pure helpers from progression-transport.js ---

function findNextNonEmpty(tl, pos) {
    const len = tl.length;
    for (let i = 1; i <= len; i++) {
        const candidate = (pos + i) % len;
        const val = tl[candidate];
        if (val >= 1 && val <= 4) return candidate;
    }
    return -1;
}

function countNonEmpty(tl) {
    return tl.filter(v => v >= 1 && v <= 4).length;
}

function countLoopsUpTo(tl, pos) {
    let count = 0;
    for (let i = 0; i <= pos; i++) {
        if (tl[i] >= 1 && tl[i] <= 4) count++;
    }
    return count;
}

function stepIntervalMs(bpm, triplet) {
    const stepsPerBeat = triplet ? 3 : 4;
    return 60000 / (bpm * stepsPerBeat);
}

// Mirror of `nextTimelinePosAfterWrap` exported from progression-transport.js.
// Inlined (rather than imported) to match the testing style already used for
// findNextNonEmpty / countNonEmpty above - the transport module pulls in DOM-
// dependent dependencies at import time which Node can't satisfy.
function nextTimelinePosAfterWrap(tl, currentPos, pendingReset) {
    if (pendingReset) {
        for (let i = 0; i < tl.length; i += 1) {
            if (tl[i] >= 1 && tl[i] <= 4) return i;
        }
        return -1;
    }
    const len = tl.length;
    for (let i = 1; i <= len; i += 1) {
        const candidate = (currentPos + i) % len;
        if (tl[candidate] >= 1 && tl[candidate] <= 4) return candidate;
    }
    return -1;
}

// --- Test runner ---

let passed = 0, failed = 0;

function assert(condition, msg) {
    if (!condition) {
        console.error(`  FAIL: ${msg}`);
        failed++;
    } else {
        passed++;
    }
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (e) {
        console.error(`  FAIL: ${name}: ${e.message}`);
        failed++;
    }
}

// =========================================================================
// Tests
// =========================================================================

console.log('progression-transport tests:');

// --- findNextNonEmpty ---

test('findNextNonEmpty: finds next filled position', () => {
    const tl = [1, 0, 2, 0, 3, 0, 4, 0];
    assert(findNextNonEmpty(tl, 0) === 2, 'from 0 → 2');
    assert(findNextNonEmpty(tl, 2) === 4, 'from 2 → 4');
    assert(findNextNonEmpty(tl, 4) === 6, 'from 4 → 6');
});

test('findNextNonEmpty: wraps around', () => {
    const tl = [1, 0, 0, 0, 2, 0, 0, 0];
    assert(findNextNonEmpty(tl, 4) === 0, 'from 4 wraps to 0');
});

test('findNextNonEmpty: returns -1 for all-empty', () => {
    const tl = [0, 0, 0, 0];
    assert(findNextNonEmpty(tl, 0) === -1, 'empty timeline → -1');
});

test('findNextNonEmpty: single-element timeline wraps to self', () => {
    const tl = [3];
    assert(findNextNonEmpty(tl, 0) === 0, 'single non-empty wraps to self');
});

test('findNextNonEmpty: skips invalid values (5, -1, etc)', () => {
    const tl = [0, 5, -1, 100, 2];
    assert(findNextNonEmpty(tl, 0) === 4, 'only val=2 at idx 4 is valid');
});

test('findNextNonEmpty: dense timeline advances by 1', () => {
    const tl = [1, 2, 3, 4];
    assert(findNextNonEmpty(tl, 0) === 1, '0 → 1');
    assert(findNextNonEmpty(tl, 1) === 2, '1 → 2');
    assert(findNextNonEmpty(tl, 2) === 3, '2 → 3');
    assert(findNextNonEmpty(tl, 3) === 0, '3 → 0 (wrap)');
});

// --- countNonEmpty ---

test('countNonEmpty: counts filled positions', () => {
    assert(countNonEmpty([1, 0, 2, 0, 3, 0, 4, 0]) === 4, '4 filled');
    assert(countNonEmpty([0, 0, 0, 0]) === 0, '0 filled');
    assert(countNonEmpty([1, 2, 3, 4]) === 4, 'all filled');
});

test('countNonEmpty: ignores out-of-range values', () => {
    assert(countNonEmpty([0, 5, -1, 100, 2]) === 1, 'only 1 valid');
});

test('countNonEmpty: empty array', () => {
    assert(countNonEmpty([]) === 0, 'empty → 0');
});

// --- countLoopsUpTo ---

test('countLoopsUpTo: counts from 0 to pos inclusive', () => {
    const tl = [1, 0, 2, 0, 3, 0, 4, 0];
    assert(countLoopsUpTo(tl, 0) === 1, 'pos 0 → 1');
    assert(countLoopsUpTo(tl, 2) === 2, 'pos 2 → 2');
    assert(countLoopsUpTo(tl, 4) === 3, 'pos 4 → 3');
    assert(countLoopsUpTo(tl, 6) === 4, 'pos 6 → 4');
    assert(countLoopsUpTo(tl, 7) === 4, 'pos 7 → still 4');
});

test('countLoopsUpTo: all empty returns 0', () => {
    assert(countLoopsUpTo([0, 0, 0], 2) === 0, 'all empty → 0');
});

test('countLoopsUpTo: pos 0 with empty first slot', () => {
    assert(countLoopsUpTo([0, 1, 2], 0) === 0, 'empty at 0 → 0');
});

// --- stepIntervalMs ---

test('stepIntervalMs: 120 BPM normal = 125ms per step', () => {
    // 120 BPM × 4 steps/beat = 480 steps/min → 60000/480 = 125
    const ms = stepIntervalMs(120, false);
    assert(Math.abs(ms - 125) < 0.001, `expected 125, got ${ms}`);
});

test('stepIntervalMs: 120 BPM triplet = 166.67ms per step', () => {
    // 120 BPM × 3 steps/beat = 360 steps/min → 60000/360 ≈ 166.67
    const ms = stepIntervalMs(120, true);
    assert(Math.abs(ms - 166.667) < 0.01, `expected ~166.67, got ${ms}`);
});

test('stepIntervalMs: 60 BPM normal = 250ms per step', () => {
    const ms = stepIntervalMs(60, false);
    assert(Math.abs(ms - 250) < 0.001, `expected 250, got ${ms}`);
});

test('stepIntervalMs: 300 BPM normal = 50ms per step', () => {
    const ms = stepIntervalMs(300, false);
    assert(Math.abs(ms - 50) < 0.001, `expected 50, got ${ms}`);
});

// --- advanceBeat logic (simulated) ---
// Simulate the core advancement logic without timers/DOM

// Pure helper mirroring the transport's preload calculation
function preloadStepFor(activeSteps) {
    return Math.max(1, activeSteps - 14);
}

test('preload step is activeSteps - 14 for 16-step pattern', () => {
    // With 16 active steps, preload happens at step 2 (14 steps before wrap)
    assert(preloadStepFor(16) === 2, 'preload at step 2 for 16-step pattern');
});

test('preload step is activeSteps - 14 for 15-step pattern', () => {
    assert(preloadStepFor(15) === 1, 'preload at step 1 for 15-step pattern');
});

test('preload clamps to step 1 for 14-step pattern', () => {
    // activeSteps - 14 = 0 would never fire (step starts at 1), so clamp to 1
    assert(preloadStepFor(14) === 1, 'preload at step 1 for 14-step pattern (clamped)');
});

test('preload clamps to step 1 for short patterns (< 14 steps)', () => {
    assert(preloadStepFor(12) === 1, 'preload at step 1 for 12-step pattern (clamped)');
    assert(preloadStepFor(8) === 1, 'preload at step 1 for 8-step pattern (clamped)');
    assert(preloadStepFor(4) === 1, 'preload at step 1 for 4-step pattern (clamped)');
});

test('advanceBeat logic: pattern wraps at activeSteps', () => {
    // When step reaches activeSteps, it should reset to 0
    const activeSteps = 16;
    let step = 16; // just reached end
    if (step >= activeSteps) step = 0;
    assert(step === 0, 'step resets to 0 at end');
});

test('save only on pattern change, not on repeat', () => {
    // Simulate: timeline [1,1,1,1,2,2,2,2] - saves should only happen
    // when transitioning from P1→P2 and P2→P1 (wrap)
    const tl = [1,1,1,1,2,2,2,2];
    const saves = [];
    // Walk through all positions, check if next differs
    for (let pos = 0; pos < tl.length; pos++) {
        const currentPat = tl[pos] - 1;
        const nextPos = findNextNonEmpty(tl, pos);
        const nextPat = tl[nextPos] - 1;
        if (nextPat !== currentPat) {
            saves.push({ pos, from: currentPat + 1, to: nextPat + 1 });
        }
    }
    // Only 2 transitions: pos 3 (P1→P2) and pos 7 (P2→P1 wrap)
    assert(saves.length === 2, `expected 2 saves, got ${saves.length}`);
    assert(saves[0].pos === 3, 'first save at pos 3');
    assert(saves[0].from === 1 && saves[0].to === 2, 'P1 → P2');
    assert(saves[1].pos === 7, 'second save at pos 7');
    assert(saves[1].from === 2 && saves[1].to === 1, 'P2 → P1 (wrap)');
});

test('no saves when all positions are same pattern', () => {
    const tl = [1,1,1,1,1,1,1,1];
    let saveCount = 0;
    for (let pos = 0; pos < tl.length; pos++) {
        const currentPat = tl[pos] - 1;
        const nextPos = findNextNonEmpty(tl, pos);
        const nextPat = tl[nextPos] - 1;
        if (nextPat !== currentPat) saveCount++;
    }
    assert(saveCount === 0, 'no saves needed when pattern never changes');
});

test('every position triggers save when pattern alternates', () => {
    const tl = [1,2,1,2];
    let saveCount = 0;
    for (let pos = 0; pos < tl.length; pos++) {
        const currentPat = tl[pos] - 1;
        const nextPos = findNextNonEmpty(tl, pos);
        const nextPat = tl[nextPos] - 1;
        if (nextPat !== currentPat) saveCount++;
    }
    assert(saveCount === 4, `all 4 transitions differ: got ${saveCount}`);
});

test('advanceBeat logic: default timeline progression', () => {
    // Default timeline: [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4]
    const tl = [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4];

    // Starting at pos 0, next non-empty is 1
    assert(findNextNonEmpty(tl, 0) === 1, 'pos 0 → 1');
    // After all P1 slots, reaches P2
    assert(findNextNonEmpty(tl, 3) === 4, 'pos 3 → 4 (first P2)');
    // P2 to P3
    assert(findNextNonEmpty(tl, 7) === 8, 'pos 7 → 8 (first P3)');
    // P4 wraps to P1
    assert(findNextNonEmpty(tl, 15) === 0, 'pos 15 → 0 (wrap to P1)');
});

test('full cycle simulation: 16-slot timeline visits all positions', () => {
    const tl = [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4];
    const visited = [];
    let pos = 0;
    for (let i = 0; i < 16; i++) {
        visited.push(pos);
        pos = findNextNonEmpty(tl, pos);
    }
    // Should visit 0..15
    for (let i = 0; i < 16; i++) {
        assert(visited[i] === i, `visited[${i}] should be ${i}, got ${visited[i]}`);
    }
    // After 16 advances, should wrap back to 0
    assert(pos === 0, 'wraps back to 0');
});

test('sparse timeline skips empty slots', () => {
    const tl = [1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0];
    const visited = [];
    let pos = 0;
    for (let i = 0; i < 4; i++) {
        visited.push(tl[pos]);
        pos = findNextNonEmpty(tl, pos);
    }
    assert(visited[0] === 1, 'first = P1');
    assert(visited[1] === 2, 'second = P2');
    assert(visited[2] === 3, 'third = P3');
    assert(visited[3] === 4, 'fourth = P4');
    assert(pos === 0, 'wraps back to start');
});

// --- nextTimelinePosAfterWrap ---
//
// Models the branch that chooses between "walk to next non-empty from the
// current position" (normal progression cycle) and "jump to first non-empty
// in the timeline" (randomize-during-playback case, deferred to the wrap so
// the visual highlight matches what the device is actually playing).

test('nextTimelinePosAfterWrap: pendingReset=false matches findNextNonEmpty', () => {
    const tl = [1, 2, 3, 4];
    assert(nextTimelinePosAfterWrap(tl, 0, false) === 1, '0 → 1');
    assert(nextTimelinePosAfterWrap(tl, 3, false) === 0, '3 → 0 (wrap)');
});

test('nextTimelinePosAfterWrap: pendingReset=true jumps to pos 0 when tl[0] is filled', () => {
    const tl = [1, 2, 3, 4];
    // Even if we were mid-cycle at pos=2, pending reset takes us back to 0.
    assert(nextTimelinePosAfterWrap(tl, 2, true) === 0, 'reset from pos 2 → 0');
    assert(nextTimelinePosAfterWrap(tl, 3, true) === 0, 'reset from pos 3 → 0');
});

test('nextTimelinePosAfterWrap: pendingReset skips leading empty slots', () => {
    const tl = [0, 0, 1, 2];
    // The "first non-empty" semantics must honour empty leaders - otherwise
    // a reset could land on an empty column and the highlight would not
    // correspond to an actual pattern row.
    assert(nextTimelinePosAfterWrap(tl, 2, true) === 2, 'reset from 2 → 2 (first filled)');
    assert(nextTimelinePosAfterWrap(tl, 3, true) === 2, 'reset from 3 → 2');
});

test('nextTimelinePosAfterWrap: pendingReset on all-empty timeline returns -1', () => {
    const tl = [0, 0, 0, 0];
    assert(nextTimelinePosAfterWrap(tl, 0, true) === -1, 'empty + reset → -1');
});

test('nextTimelinePosAfterWrap: pendingReset overrides normal walk', () => {
    // Without reset, from pos=0 we'd walk to pos=2 (next non-empty). With
    // reset, we must snap to pos=0 itself because tl[0] is filled - i.e.
    // the reset path doesn't advance past the first filled slot.
    const tl = [1, 0, 2, 0];
    assert(nextTimelinePosAfterWrap(tl, 0, false) === 2, 'no-reset: 0 → 2');
    assert(nextTimelinePosAfterWrap(tl, 0, true) === 0, 'reset:    0 → 0');
});

// --- Summary ---

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
