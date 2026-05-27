// Usage: node ui/js/multipattern/multipattern-state.test.js
//
// Verifies the runtime state module for multi-pattern main-page flows.
// Covers:
//   - default N=1 + focused behaviour matches the legacy single-pattern
//     state.js surface (so main.js rewire is safe),
//   - structural ops (ADD / DUPLICATE / DEL) preserve focus/check/timeline
//     invariants,
//   - selection resolution,
//   - snapshot round-trip preserves the full shape.
//
// Provides a minimal in-memory sessionStorage polyfill because the module
// loads sessionStorage at import time.

if (typeof globalThis.sessionStorage === 'undefined') {
    const store = new Map();
    globalThis.sessionStorage = {
        getItem: (k) => (store.has(k) ? store.get(k) : null),
        setItem: (k, v) => { store.set(k, String(v)); },
        removeItem: (k) => { store.delete(k); },
        clear: () => { store.clear(); },
    };
}

const state = await import('./multipattern-state.js');

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; return; }
    passed++;
}
function test(name, fn) {
    try { fn(); console.log(`  ok: ${name}`); }
    catch (e) { console.error(`  FAIL: ${name}: ${e.stack || e.message}`); failed++; }
}

// Reset to a known baseline before every test. The module is a singleton
// so tests that mutate state have to tidy after themselves; restoreSnapshot
// is the cleanest knob.
function reset() {
    state.restoreSnapshot({
        patterns: [
            {
                active_steps: 16,
                triplet: false,
                steps: Array.from({ length: 16 }, () => ({
                    note: 'C', transpose: 'NORMAL',
                    accent: false, slide: false, time: 'NORMAL',
                })),
            },
        ],
        focusedIdx: 0,
        checked: [],
        timeline: [1],
        abMode: 'SERIAL',
        viewport: { group: 'ALL', side: 'ALL' },
    }, true);
}

console.log('multipattern-state tests:');

// ---------------------------------------------------------------------------
// Single-pattern compat surface
// ---------------------------------------------------------------------------

test('initial state: 1 pattern, focused=0, no checks', () => {
    reset();
    assert(state.getPatternCount() === 1, 'count 1');
    assert(state.getAbMode() === 'SERIAL', 'default A/B mode is serial');
    assert(state.getFocusedIdx() === 0, 'focused 0');
    assert(state.getCheckedArray().length === 0, 'no checks');
    assert(Array.isArray(state.getPattern().steps), 'focused pattern has steps');
    assert(state.getPattern().steps.length === 16, 'focused pattern 16 steps');
});

test('setStep(stepIdx, step) without patIdx targets focused pattern', () => {
    reset();
    state.setStep(0, { note: 'D#', transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    assert(state.getPattern().steps[0].note === 'D#', 'focused pattern updated');
    assert(state.getPattern().steps[0].accent === true, 'accent flag');
});

test('setStep(patIdx, stepIdx, step) with explicit patIdx', () => {
    reset();
    state.addPattern();
    state.setStep(1, 4, { note: 'G', transpose: 'NORMAL', accent: false, slide: true, time: 'TIE' });
    assert(state.getPattern(1).steps[4].note === 'G', 'pat 1 step 4 note');
    assert(state.getPattern(1).steps[4].slide === true, 'pat 1 step 4 slide');
    assert(state.getPattern(0).steps[4].note === 'C', 'pat 0 untouched');
});

test('cycleTime / toggleAccent / toggleSlide single-arg defaults to focused', () => {
    reset();
    state.cycleTime(0);
    assert(state.getPattern().steps[0].time === 'REST', 'time REST after one cycle');
    state.toggleAccent(1);
    assert(state.getPattern().steps[1].accent === true, 'accent toggled');
    state.toggleSlide(2);
    assert(state.getPattern().steps[2].slide === true, 'slide toggled');
});

test('transposePattern / shiftSteps / resetPattern default to focused', () => {
    reset();
    state.setStep(0, { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.transposePattern(+1);
    assert(state.getPattern().steps[0].note === 'C#', 'transpose +1 applied');

    state.shiftSteps(2);
    assert(state.getPattern().steps[2].note === 'C#', 'step shifted to index 2');

    state.resetPattern();
    assert(state.getPattern().steps[0].note === 'C', 'reset brought step 0 back to default');
});

// ---------------------------------------------------------------------------
// Structural ops
// ---------------------------------------------------------------------------

test('ADD appends a default pattern and focuses it', () => {
    reset();
    const ok = state.addPattern();
    assert(ok === true, 'add returned true');
    assert(state.getPatternCount() === 2, 'count 2');
    assert(state.getFocusedIdx() === 1, 'focus landed on new pattern');
    assert(state.getTimeline().slice(-1)[0] === 2, 'timeline appended pattern 2');
});

test('DUPLICATE focused inserts after source and focuses copy', () => {
    reset();
    state.setStep(0, { note: 'E', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.addPattern();       // idx 1 (blank)
    state.setFocused(0);
    const ok = state.duplicatePattern();
    assert(ok === true, 'dup returned true');
    assert(state.getPatternCount() === 3, 'count 3');
    assert(state.getFocusedIdx() === 1, 'focus on copy at idx 1');
    assert(state.getPattern(1).steps[0].note === 'E', 'copy has source data');
    assert(state.getPattern(2).steps[0].note === 'C', 'original idx-1 shifted to idx 2 (blank)');
});

test('DEL removes focused and lands focus on sibling', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setFocused(1);
    state.deletePattern();
    assert(state.getPatternCount() === 2, 'count 2');
    assert(state.getFocusedIdx() === 1, 'focus on new idx 1 (was 2)');
});

test('DEL on last pattern focuses previous sibling', () => {
    reset();
    state.addPattern();
    state.setFocused(1);
    state.deletePattern();
    assert(state.getPatternCount() === 1, 'count 1');
    assert(state.getFocusedIdx() === 0, 'focus on idx 0');
});

test('DEL when N=1 resets the sole pattern (keeps N>=1)', () => {
    reset();
    state.setStep(0, { note: 'G', transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    state.deletePattern();
    assert(state.getPatternCount() === 1, 'still 1 pattern');
    assert(state.getPattern().steps[0].note === 'C', 'reset to default');
    assert(state.getPattern().steps[0].accent === false, 'accent cleared');
});

test('DUPLICATE appends new pattern number to timeline (regression)', () => {
    // Regression: duplicate was orphaned from playback because it never
    // got a timeline entry. shiftTimelineForInsert bumps existing
    // references to preserve content-pointing, but the new pattern
    // itself was missing until we append its number too - mirroring
    // ADD semantics.
    reset();
    state.addPattern();     // timeline = [1, 2]
    state.addPattern();     // timeline = [1, 2, 3]
    state.setFocused(1);    // source = pattern 2 at idx 1
    state.duplicatePattern();
    const tl = state.getTimeline();
    // Source was at idx 1 (pat #2); copy inserts at idx 2 (pat #3);
    // original idx 2 (pat #3) shifts to idx 3 (pat #4). Existing timeline
    // [1,2,3] → shift entries >=3 up → [1,2,4] → append new pat #3 → [1,2,4,3].
    assert(tl.length === 4, `timeline extended, got length ${tl.length}`);
    assert(tl[0] === 1 && tl[1] === 2 && tl[2] === 4, `existing entries preserved by shift, got [${tl.join(',')}]`);
    assert(tl[3] === 3, `new duplicate (pat #3) appended, got tl[3]=${tl[3]}`);
});

test('DUPLICATE re-indexes checked set past the insertion point', () => {
    reset();
    state.addPattern();   // 1
    state.addPattern();   // 2
    state.addPattern();   // 3
    state.setChecked(2, true);
    state.setChecked(3, true);
    state.setFocused(1);
    state.duplicatePattern();
    // new copy lands at idx 2; old idx 2 -> 3, old idx 3 -> 4
    const checked = state.getCheckedArray();
    assert(checked.length === 2, 'still two checks');
    assert(checked[0] === 3 && checked[1] === 4, `checks shifted to [3,4], got [${checked.join(',')}]`);
});

test('DEL re-indexes checked set and drops the deleted index', () => {
    reset();
    state.addPattern();   // 1
    state.addPattern();   // 2
    state.addPattern();   // 3
    state.setChecked(1, true);
    state.setChecked(2, true);
    state.setChecked(3, true);
    state.deletePattern(2);
    const checked = state.getCheckedArray();
    assert(checked.length === 2, 'dropped deleted index');
    assert(checked[0] === 1 && checked[1] === 2, `expected [1,2] got [${checked.join(',')}]`);
});

test('ADD refuses past MAX_PATTERNS (64) and leaves state unchanged', () => {
    reset();
    while (state.getPatternCount() < 64) {
        assert(state.addPattern() === true, `add ok at count ${state.getPatternCount()}`);
    }
    assert(state.getPatternCount() === 64, 'count at cap');
    const focusBefore = state.getFocusedIdx();
    assert(state.addPattern() === false, 'add past cap returned false');
    assert(state.getPatternCount() === 64, 'count unchanged');
    assert(state.getFocusedIdx() === focusBefore, 'focus unchanged');
});

test('DUPLICATE refuses past MAX_PATTERNS and leaves state unchanged', () => {
    reset();
    while (state.getPatternCount() < 64) state.addPattern();
    state.setFocused(0);
    const focusBefore = state.getFocusedIdx();
    assert(state.duplicatePattern() === false, 'duplicate at cap returned false');
    assert(state.getPatternCount() === 64, 'count unchanged');
    assert(state.getFocusedIdx() === focusBefore, 'focus unchanged');
});

test('resetPattern(idx) targets a specific pattern, leaves others alone', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setStep(0, 0, { note: 'D', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(1, 0, { note: 'E', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'F', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.resetPattern(1);
    assert(state.getPattern(0).steps[0].note === 'D', 'pat 0 untouched');
    assert(state.getPattern(1).steps[0].note === 'C', 'pat 1 reset');
    assert(state.getPattern(2).steps[0].note === 'F', 'pat 2 untouched');
});

test('getSelectionIndexes returns checked indexes, or focused index when nothing is checked', () => {
    reset();
    state.addPattern();
    state.addPattern();
    for (let i = 0; i < 3; i++) {
        state.setStep(i, 0, { note: 'G', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    }
    // No checks means selection is the focused pattern.
    state.setFocused(0);
    const selOne = state.getSelectionIndexes();
    assert(selOne.length === 1 && selOne[0] === 0, 'selection = [0]');
    for (const i of selOne) state.resetPattern(i);
    assert(state.getPattern(0).steps[0].note === 'C', 'focused reset');
    assert(state.getPattern(1).steps[0].note === 'G', 'others untouched');
    // Checked patterns replace the focused fallback.
    state.setChecked(1, true);
    state.setChecked(2, true);
    const selMany = state.getSelectionIndexes();
    assert(selMany.length === 2, 'selection = checked when non-empty');
    for (const i of selMany) state.resetPattern(i);
    assert(state.getPattern(1).steps[0].note === 'C', 'pat 1 reset');
    assert(state.getPattern(2).steps[0].note === 'C', 'pat 2 reset');
});

test('ADD/DEL update timeline pattern numbers correctly', () => {
    reset();
    state.addPattern();     // timeline = [1, 2]
    state.addPattern();     // timeline = [1, 2, 3]
    state.setFocused(1);
    state.deletePattern();  // removes pattern 2, compacts + renumbers 3->2
    const tl = state.getTimeline();
    // Timeline compacts on delete: the slot referencing the deleted
    // pattern is removed entirely so playback tracks UI pattern count.
    assert(tl.length === 2, `timeline length compacted, got ${tl.length}`);
    assert(tl[0] === 1, 'tl[0] = 1');
    assert(tl[1] === 2, 'tl[1] = 2 (was pattern 3, renumbered)');
});

// ---------------------------------------------------------------------------
// Selection resolution
// ---------------------------------------------------------------------------

test('Reading X: with 0 checks, selection = [focused]', () => {
    reset();
    state.addPattern();
    state.setFocused(1);
    const sel = state.getSelectionIndexes();
    assert(sel.length === 1 && sel[0] === 1, `expected [1], got [${sel.join(',')}]`);
});

test('Reading X: with >=1 check, selection = checked (ignores focused)', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setFocused(2);
    state.setChecked(0, true);
    state.setChecked(1, true);
    const sel = state.getSelectionIndexes();
    assert(sel.length === 2 && sel[0] === 0 && sel[1] === 1, `expected [0,1], got [${sel.join(',')}]`);
});

// ---------------------------------------------------------------------------
// AUTO-STEP FWD focus advance
// ---------------------------------------------------------------------------

test('advanceFocusAfterWrap: no checks, wraps within full list', () => {
    reset();
    state.addPattern();      // idx 1
    state.addPattern();      // idx 2
    state.setFocused(0);
    assert(state.advanceFocusAfterWrap() === 1, '0 -> 1');
    assert(state.advanceFocusAfterWrap() === 2, '1 -> 2');
    assert(state.advanceFocusAfterWrap() === 0, '2 wraps -> 0');
});

test('advanceFocusAfterWrap: with checks, walks the checked ring', () => {
    reset();
    state.addPattern();      // idx 1
    state.addPattern();      // idx 2
    state.addPattern();      // idx 3
    state.setChecked(1, true);
    state.setChecked(3, true);
    state.setFocused(1);
    assert(state.advanceFocusAfterWrap() === 3, 'checked ring: 1 -> 3');
    assert(state.advanceFocusAfterWrap() === 1, 'checked ring wraps: 3 -> 1');
});

test('advanceFocusAfterWrap: focus outside checked ring lands on first checked', () => {
    reset();
    state.addPattern();      // 1
    state.addPattern();      // 2
    state.setChecked(1, true);
    state.setFocused(0);
    assert(state.advanceFocusAfterWrap() === 1, 'lands on first checked');
});

test('advanceFocusAfterWrap: single-element ring stays put', () => {
    reset();
    state.addPattern();      // 1
    state.setChecked(0, true);
    state.setFocused(0);
    assert(state.advanceFocusAfterWrap() === 0, 'single-check ring: stay');
});

// ---------------------------------------------------------------------------
// Snapshot round-trip
// ---------------------------------------------------------------------------

test('getSnapshot / restoreSnapshot round-trip preserves full shape', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setFocused(2);
    state.setChecked(0, true);
    state.setChecked(2, true);
    state.setAbMode('SERIAL');
    state.setViewport({ group: 'G2', side: 'B' });
    state.setStep(1, 5, { note: 'F#', transpose: 'UP', accent: true, slide: true, time: 'TIE' });

    const snap = state.getSnapshot();

    // Mutate away to prove restore overwrites everything.
    state.deletePattern();
    state.clearChecked();
    state.setAbMode('ALTERNATE');
    state.setViewport({ group: 'ALL', side: 'ALL' });

    state.restoreSnapshot(snap, true);

    assert(state.getPatternCount() === 3, 'pattern count restored');
    assert(state.getFocusedIdx() === 2, 'focus restored');
    const checks = state.getCheckedArray();
    assert(checks.length === 2 && checks[0] === 0 && checks[1] === 2, `checks restored, got [${checks.join(',')}]`);
    assert(state.getAbMode() === 'SERIAL', 'abMode restored');
    assert(state.getViewport().group === 'G2' && state.getViewport().side === 'B', 'viewport restored');
    assert(state.getPattern(1).steps[5].note === 'F#', 'step data restored');
    assert(state.getPattern(1).steps[5].transpose === 'UP', 'step transpose flag restored');
});

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

test('copyFocused / pasteIntoFocused transfer pattern data', () => {
    reset();
    state.addPattern();
    state.setFocused(0);
    state.setStep(0, 3, { note: 'A', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    assert(state.copyFocused() === true, 'copy ok');
    state.setFocused(1);
    assert(state.pasteIntoFocused() === true, 'paste ok');
    assert(state.getPattern(1).steps[3].note === 'A', 'pasted step landed');
    assert(state.getPattern(0).steps[3].note === 'A', 'source untouched');
});

// ---------------------------------------------------------------------------
// LOAD ALL guard
// ---------------------------------------------------------------------------

test('hasNonDefaultPatterns flips when any pattern is edited', () => {
    reset();
    assert(state.hasNonDefaultPatterns() === false, 'fresh: all default');
    state.setStep(0, 0, { note: 'E', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    assert(state.hasNonDefaultPatterns() === true, 'after edit: non-default present');
});

// ---------------------------------------------------------------------------
// Scratch slot + A/B mode + viewport
// ---------------------------------------------------------------------------

test('setScratchSlot accepts a valid descriptor and exposes it via getScratchSlot', () => {
    reset();
    state.setScratchSlot({ group: 2, pattern: 3, side: 'B' });
    const s = state.getScratchSlot();
    assert(s !== null, 'scratch slot set');
    assert(s.group === 2 && s.pattern === 3 && s.side === 'B', 'fields correct');
    assert(s.label === 'G2P3B', 'default label synthesized');
});

test('setScratchSlot(null) clears the slot', () => {
    reset();
    state.setScratchSlot({ group: 1, pattern: 1, side: 'A' });
    state.setScratchSlot(null);
    assert(state.getScratchSlot() === null, 'scratch cleared');
});

test('setScratchSlot rejects malformed descriptors (keeps old value)', () => {
    reset();
    state.setScratchSlot({ group: 1, pattern: 1, side: 'A' });
    state.setScratchSlot({ group: 5, pattern: 1, side: 'A' });         // group OOR
    state.setScratchSlot({ group: 1, pattern: 9, side: 'A' });         // pattern OOR
    state.setScratchSlot({ group: 1, pattern: 1, side: 'C' });         // side OOR
    const s = state.getScratchSlot();
    assert(s && s.group === 1 && s.pattern === 1 && s.side === 'A',
        'old value preserved after malformed updates');
});

test('setAbMode flips between ALTERNATE and SERIAL', () => {
    reset();
    state.setAbMode('SERIAL');
    assert(state.getAbMode() === 'SERIAL', 'SERIAL set');
    state.setAbMode('ALTERNATE');
    assert(state.getAbMode() === 'ALTERNATE', 'ALTERNATE set');
    // Invalid input ignored.
    state.setAbMode('INVALID');
    assert(state.getAbMode() === 'ALTERNATE', 'invalid ignored');
});

test('setViewport normalises missing fields to ALL', () => {
    reset();
    state.setViewport({ group: '2', side: 'A' });
    assert(state.getViewport().group === '2' && state.getViewport().side === 'A', 'fields set');
    state.setViewport({});
    assert(state.getViewport().group === 'ALL' && state.getViewport().side === 'ALL', 'empty → ALL');
});

test('snapshot round-trips scratch-independent fields (scratch is transient)', () => {
    reset();
    state.setAbMode('SERIAL');
    state.setViewport({ group: '3', side: 'B' });
    state.setScratchSlot({ group: 1, pattern: 2, side: 'A' });
    const snap = state.getSnapshot();
    // Corrupt state; restore should bring back abMode + viewport, but leave
    // scratch alone (it's fetched from the backend, not part of undo state).
    state.setAbMode('ALTERNATE');
    state.setViewport({ group: 'ALL', side: 'ALL' });
    state.restoreSnapshot(snap, true);
    assert(state.getAbMode() === 'SERIAL', 'abMode restored');
    assert(state.getViewport().group === '3', 'viewport group restored');
    assert(state.getViewport().side === 'B', 'viewport side restored');
    assert(state.getScratchSlot() !== null, 'scratch not wiped by restore');
});

// ---------------------------------------------------------------------------
// movePattern - drag-to-reorder on the main page
// ---------------------------------------------------------------------------

function reset3WithFocusAndChecks() {
    reset();
    state.addPattern(); // N=2, focus=1
    state.addPattern(); // N=3, focus=2
    // Give each pattern a distinct step[0] note so we can identify them
    // after a move (default N uses 'C', so we repaint 3 distinct notes).
    state.setStep(0, 0, { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(1, 0, { note: 'D', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'E', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setFocused(0);
    state.setChecked(0, true);
    state.setChecked(2, true);
}

test('movePattern(from=0, to=2) splices pattern to end, content follows', () => {
    reset3WithFocusAndChecks();
    const ok = state.movePattern(0, 2);
    assert(ok === true, 'move returned true');
    assert(state.getPattern(0).steps[0].note === 'D', 'idx 0 now holds old P2 (D)');
    assert(state.getPattern(1).steps[0].note === 'E', 'idx 1 now holds old P3 (E)');
    assert(state.getPattern(2).steps[0].note === 'C', 'idx 2 now holds moved pattern (C)');
});

test('movePattern remaps focused index to follow the moved pattern', () => {
    reset3WithFocusAndChecks();
    // focused=0 before; after moving 0 → 2, focus should land on 2.
    state.movePattern(0, 2);
    assert(state.getFocusedIdx() === 2, `focus expected 2, got ${state.getFocusedIdx()}`);
});

test('movePattern remaps checked indexes so marks stay on same pattern content', () => {
    reset3WithFocusAndChecks();
    // checks at {0, 2} before; after moving 0 → 2:
    //   old 0 → new 2
    //   old 2 → new 1 (slid down one because from < to swept past it)
    state.movePattern(0, 2);
    const checked = state.getCheckedArray();
    assert(JSON.stringify(checked) === '[1,2]', `checked expected [1,2], got ${JSON.stringify(checked)}`);
});

test('movePattern(from=2, to=0) upward move also remaps focus + checks', () => {
    reset3WithFocusAndChecks();
    // checks at {0, 2}; focus=0. After moving 2 → 0:
    //   old 2 → new 0
    //   old 0 → new 1
    //   check set becomes {0, 1}; focus follows old 0 → new 1.
    state.movePattern(2, 0);
    assert(state.getFocusedIdx() === 1, `focus expected 1, got ${state.getFocusedIdx()}`);
    const checked = state.getCheckedArray();
    assert(JSON.stringify(checked) === '[0,1]', `checked expected [0,1], got ${JSON.stringify(checked)}`);
    assert(state.getPattern(0).steps[0].note === 'E', 'idx 0 = old P3 (E)');
    assert(state.getPattern(1).steps[0].note === 'C', 'idx 1 = old P1 (C)');
    assert(state.getPattern(2).steps[0].note === 'D', 'idx 2 = old P2 (D)');
});

test('movePattern is a no-op on from===to and on out-of-range indexes', () => {
    reset3WithFocusAndChecks();
    assert(state.movePattern(1, 1) === false, 'same idx returns false');
    assert(state.movePattern(-1, 2) === false, 'negative from returns false');
    assert(state.movePattern(1, 99) === false, 'out-of-range to returns false');
    assert(state.getPatternCount() === 3, 'count still 3');
    assert(state.getPattern(0).steps[0].note === 'C', 'idx 0 still C');
    assert(state.getPattern(2).steps[0].note === 'E', 'idx 2 still E');
});

test('movePattern returns false when N < 2', () => {
    reset();
    assert(state.movePattern(0, 0) === false, 'N=1 cannot reorder');
});

test('movePattern leaves timeline numbers alone (visual order drives playback)', () => {
    // After this move, timeline [1,2,3] now means [old2, old3, old1] by
    // content - that is the intended semantic (visual order = playback order).
    reset3WithFocusAndChecks();
    const before = state.getTimeline().slice();
    state.movePattern(0, 2);
    const after = state.getTimeline();
    assert(JSON.stringify(after) === JSON.stringify(before),
        `timeline expected unchanged, was ${JSON.stringify(before)}, got ${JSON.stringify(after)}`);
});

// ---------------------------------------------------------------------------
// Dual-timeline model (checkbox-driven)
// ---------------------------------------------------------------------------

test('dual-timeline: no checks → getTimeline returns timelineDefault', () => {
    reset();
    state.addPattern(); state.addPattern(); state.addPattern(); // N=4, timeline [1,2,3,4]
    state.clearChecked();
    assert(state.isCheckedMode() === false, 'not in checked mode');
    const tl = state.getTimeline();
    assert(JSON.stringify(tl) === '[1,2,3,4]', `default tl expected [1,2,3,4], got ${JSON.stringify(tl)}`);
});

test('dual-timeline: check P2 → getTimeline returns [2]', () => {
    reset();
    state.addPattern(); state.addPattern(); state.addPattern();
    state.setChecked(1, true); // pat #2 by 1-based numbering, idx 1
    assert(state.isCheckedMode() === true, 'checked mode on');
    assert(JSON.stringify(state.getTimeline()) === '[2]', `expected [2], got ${JSON.stringify(state.getTimeline())}`);
});

test('dual-timeline: check sequence P2 then P5 appends in order', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern(); // N=5
    state.setChecked(1, true); // P2
    state.setChecked(4, true); // P5
    assert(JSON.stringify(state.getTimeline()) === '[2,5]', `expected [2,5], got ${JSON.stringify(state.getTimeline())}`);
});

test('dual-timeline: rearrange checked-timeline persists across re-render', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern(); // N=5
    state.setChecked(1, true); state.setChecked(4, true); // [2,5]
    state.setTimeline([2, 2, 2, 2, 5, 5, 5, 5]);
    const tl = state.getTimeline();
    assert(JSON.stringify(tl) === '[2,2,2,2,5,5,5,5]', 'custom arrangement stored');
});

test('dual-timeline: checking a 3rd pattern appends one entry to existing arrangement', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern(); // N=5
    state.setChecked(1, true); state.setChecked(4, true);
    state.setTimeline([2, 2, 2, 2, 5, 5, 5, 5]);
    state.setChecked(2, true); // P3 → append one 3
    const tl = state.getTimeline();
    assert(JSON.stringify(tl) === '[2,2,2,2,5,5,5,5,3]', `expected [2,2,2,2,5,5,5,5,3], got ${JSON.stringify(tl)}`);
});

test('dual-timeline: unchecking a pattern strips every entry for it', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern();
    state.setChecked(1, true); state.setChecked(4, true);
    state.setTimeline([2, 2, 2, 2, 5, 5, 5, 5]);
    state.setChecked(2, true); // [2,2,2,2,5,5,5,5,3]
    state.setChecked(4, false); // uncheck P5 → drop every 5
    const tl = state.getTimeline();
    assert(JSON.stringify(tl) === '[2,2,2,2,3]', `expected [2,2,2,2,3], got ${JSON.stringify(tl)}`);
});

test('dual-timeline: uncheck-all returns playback to default timeline (default preserved)', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern();
    state.setTimeline([5, 4, 3, 2, 1]); // user custom default timeline
    state.setChecked(1, true); // switch to checked mode
    state.setTimeline([2, 2, 2]); // custom checked timeline
    assert(JSON.stringify(state.getTimeline()) === '[2,2,2]', 'checked timeline active');
    state.setChecked(1, false); // back to default mode
    assert(state.isCheckedMode() === false, 'no more checks');
    assert(JSON.stringify(state.getTimeline()) === '[5,4,3,2,1]', 'default timeline preserved');
});

test('dual-timeline: clearChecked drains timelineChecked completely', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern();
    state.setChecked(1, true); state.setChecked(2, true); state.setChecked(4, true);
    state.setTimeline([2, 3, 5, 2, 3, 5]); // arranged
    state.clearChecked();
    assert(state.isCheckedMode() === false, 'checked cleared');
    // Re-check anything → arrangement starts fresh
    state.setChecked(0, true);
    assert(JSON.stringify(state.getTimeline()) === '[1]', 'checked tl restarted fresh');
});

test('dual-timeline: setAllChecked selects every pattern in pattern order', () => {
    reset();
    for (let i = 0; i < 4; i++) state.addPattern();
    state.setChecked(2, true);
    state.setTimeline([3, 3, 3]);
    state.setAllChecked(true);
    assert(JSON.stringify(state.getCheckedArray()) === '[0,1,2,3,4]', 'all patterns checked');
    assert(JSON.stringify(state.getTimelineChecked()) === '[1,2,3,4,5]', 'checked timeline reset to pattern order');
});

test('dual-timeline: setAllChecked false clears every checked pattern', () => {
    reset();
    state.addPattern(); state.addPattern();
    state.setAllChecked(true);
    state.setAllChecked(false);
    assert(JSON.stringify(state.getCheckedArray()) === '[]', 'all checks cleared');
    assert(JSON.stringify(state.getTimelineChecked()) === '[]', 'checked timeline cleared');
});

test('dual-timeline: ADD appends to default only, leaves checked arrangement alone', () => {
    reset();
    state.addPattern(); state.addPattern(); // N=3, default tl=[1,2,3]
    state.setChecked(1, true); // checked tl=[2]
    state.setTimeline([2, 2, 2]); // arranged
    state.addPattern(); // N=4
    // Inspect both timelines directly via the dedicated getters so we
    // don't have to toggle modes.
    assert(JSON.stringify(state.getTimelineDefault()) === '[1,2,3,4]',
        `default grew to 4, got ${JSON.stringify(state.getTimelineDefault())}`);
    assert(JSON.stringify(state.getTimelineChecked()) === '[2,2,2]',
        `checked arrangement preserved through ADD, got ${JSON.stringify(state.getTimelineChecked())}`);
});

test('dual-timeline: DUPLICATE appends to default only, leaves checked arrangement alone', () => {
    reset();
    state.addPattern(); state.addPattern(); // N=3
    state.setChecked(2, true); // check P3 - checked tl=[3]
    state.setTimeline([3, 3]); // arrange checked
    state.setFocused(0);
    state.duplicatePattern(); // insert copy at idx 1, N=4
    // Default tl: was [1,2,3] → shift entries >=2 → [1,3,4] → append new 2 → [1,3,4,2]
    assert(JSON.stringify(state.getTimelineDefault()) === '[1,3,4,2]',
        `default tl after dup expected [1,3,4,2], got ${JSON.stringify(state.getTimelineDefault())}`);
    // Checked tl: [3,3] → shift entries >=2 by +1 → [4,4]. The P3 content
    // is now at slot 4, and entries keep pointing at it. checkedSet
    // remaps 2→3 to stay on the P3 content.
    assert(JSON.stringify(state.getTimelineChecked()) === '[4,4]',
        `checked tl after dup expected [4,4], got ${JSON.stringify(state.getTimelineChecked())}`);
    assert(state.isChecked(3) === true, 'P3 content still checked at its new idx 3');
});

test('dual-timeline: DEL removes pattern from both timelines', () => {
    reset();
    state.addPattern(); state.addPattern(); state.addPattern(); // N=4, default=[1,2,3,4]
    state.setChecked(1, true); state.setChecked(2, true); // checked tl=[2,3]
    state.setTimeline([2, 3, 2, 3]);
    state.clearChecked(); // drain checked
    state.setTimeline([1, 2, 3, 4]); // ensure default
    state.setChecked(1, true); state.setChecked(2, true);
    state.setTimeline([2, 3, 2, 3]); // re-arrange checked
    state.deletePattern(1); // delete P2
    // default: [1,2,3,4] → drop 2s, decrement >2 → [1,2,3]
    // checked: [2,3,2,3] → drop 2s, decrement 3→2 → [2,2]
    // checkedSet: {1,2} → remove 1 (deleted), remap 2→1 → {1}
    const currentTl = state.getTimeline(); // still checked mode
    assert(JSON.stringify(currentTl) === '[2,2]', `checked after del expected [2,2], got ${JSON.stringify(currentTl)}`);
    state.clearChecked();
    assert(JSON.stringify(state.getTimeline()) === '[1,2,3]', `default after del expected [1,2,3], got ${JSON.stringify(state.getTimeline())}`);
});

test('dual-timeline: DEL of sole pattern resets both timelines', () => {
    reset();
    state.setChecked(0, true);
    assert(JSON.stringify(state.getTimeline()) === '[1]', 'checked tl = [1]');
    state.deletePattern(); // N=1, keeps single pattern, resets
    assert(state.isCheckedMode() === false, 'checks cleared on sole-reset');
    assert(JSON.stringify(state.getTimeline()) === '[1]', 'default reset to [1]');
    // timelineChecked should also be reset - check by re-entering checked mode
    state.setChecked(0, true);
    assert(JSON.stringify(state.getTimeline()) === '[1]', 'checked tl restarted fresh after sole-reset');
});

test('dual-timeline: toggleChecked off twice in a row is no-op', () => {
    reset();
    state.setChecked(0, false); // already unchecked - nothing to do
    assert(JSON.stringify(state.getTimelineChecked()) === '[]', 'checked tl untouched');
});

test('dual-timeline: snapshot round-trips both timelines', () => {
    reset();
    state.addPattern(); state.addPattern();
    state.setTimeline([3, 2, 1]); // custom default
    state.setChecked(1, true);
    state.setTimeline([2, 2, 2]); // custom checked
    const snap = state.getSnapshot();
    reset(); // wipes both
    state.restoreSnapshot(snap);
    assert(JSON.stringify(state.getTimeline()) === '[2,2,2]', 'checked tl restored (in checked mode)');
    state.clearChecked();
    assert(JSON.stringify(state.getTimeline()) === '[3,2,1]', 'default tl restored');
});

test('dual-timeline: legacy snapshot with only `timeline` key loads as default', () => {
    reset();
    state.addPattern();
    // Simulate a pre-split snapshot - no timelineDefault/timelineChecked keys.
    const legacySnap = {
        patterns: [state.getPattern(0), state.getPattern(1)],
        focusedIdx: 0,
        checked: [],
        timeline: [2, 1, 2, 1],
        abMode: 'ALTERNATE',
        viewport: { group: 'ALL', side: 'ALL' },
    };
    state.restoreSnapshot(legacySnap);
    assert(JSON.stringify(state.getTimeline()) === '[2,1,2,1]', 'legacy timeline loaded as default');
    assert(JSON.stringify(state.getTimelineChecked()) === '[]', 'legacy → checked tl blank');
});

// ---------------------------------------------------------------------------
// Per-pattern active_steps + global apply-to-all
// ---------------------------------------------------------------------------

test('setActiveSteps with explicit idx affects only that pattern', () => {
    reset();
    state.addPattern();
    state.addPattern(); // P1, P2, P3 all default 16
    state.setActiveSteps(0, 12);
    state.setActiveSteps(1, 10);
    state.setActiveSteps(2, 3);
    assert(state.getActiveSteps(0) === 12, 'P1 → 12');
    assert(state.getActiveSteps(1) === 10, 'P2 → 10');
    assert(state.getActiveSteps(2) === 3,  'P3 → 3');
});

test('setActiveSteps clamps to 1..16', () => {
    reset();
    state.setActiveSteps(0, 99);
    assert(state.getActiveSteps(0) === 16, 'over-cap clamped to 16');
    state.setActiveSteps(0, -5);
    assert(state.getActiveSteps(0) === 1, 'under-floor clamped to 1');
});

test('getMaxActiveSteps returns the longest across patterns', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setActiveSteps(0, 12);
    state.setActiveSteps(1, 10);
    state.setActiveSteps(2, 3);
    assert(state.getMaxActiveSteps() === 12, 'max(12,10,3) = 12');
    state.setActiveSteps(2, 16);
    assert(state.getMaxActiveSteps() === 16, 'after raising P3, max = 16');
});

test('getMaxActiveSteps with single pattern returns its value', () => {
    reset();
    state.setActiveSteps(0, 7);
    assert(state.getMaxActiveSteps() === 7, 'single pattern → its own value');
});

test('setAllActiveSteps overwrites every pattern with the new value', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setActiveSteps(0, 12);
    state.setActiveSteps(1, 10);
    state.setActiveSteps(2, 3);
    state.setAllActiveSteps(13);
    assert(state.getActiveSteps(0) === 13, 'P1 → 13');
    assert(state.getActiveSteps(1) === 13, 'P2 → 13');
    assert(state.getActiveSteps(2) === 13, 'P3 → 13');
    assert(state.getMaxActiveSteps() === 13, 'max reflects the new uniform value');
});

test('setAllActiveSteps clamps to 1..16', () => {
    reset();
    state.addPattern();
    state.setAllActiveSteps(99);
    assert(state.getActiveSteps(0) === 16 && state.getActiveSteps(1) === 16, 'over-cap clamped to 16');
    state.setAllActiveSteps(0);
    assert(state.getActiveSteps(0) === 1 && state.getActiveSteps(1) === 1, 'under-floor clamped to 1');
});

test('per-pattern active_steps survive snapshot round-trip', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setActiveSteps(0, 12);
    state.setActiveSteps(1, 10);
    state.setActiveSteps(2, 3);
    const snap = state.getSnapshot();
    reset();
    state.restoreSnapshot(snap);
    assert(state.getActiveSteps(0) === 12, 'P1 restored to 12');
    assert(state.getActiveSteps(1) === 10, 'P2 restored to 10');
    assert(state.getActiveSteps(2) === 3,  'P3 restored to 3');
});

// ---------------------------------------------------------------------------
// Bulk toolbar ops (DUPLICATE / DEL / SHIFT / TRNSPS - checkbox-aware)
// ---------------------------------------------------------------------------

test('getAllIndexes returns [0..N-1]', () => {
    reset();
    assert(JSON.stringify(state.getAllIndexes()) === '[0]', 'N=1 → [0]');
    state.addPattern();
    state.addPattern();
    assert(JSON.stringify(state.getAllIndexes()) === '[0,1,2]', 'N=3 → [0,1,2]');
});

test('duplicateCheckedToBottom: P1+P3 checked → appends P1 then P3 at bottom', () => {
    reset();
    state.addPattern(); // P2
    state.addPattern(); // P3
    // Tag each pattern's first step note so we can detect copy origins.
    state.setStep(0, 0, { note: 'C',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(1, 0, { note: 'D',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'E',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setChecked(0, true);
    state.setChecked(2, true);
    const added = state.duplicateCheckedToBottom();
    assert(added === 2, 'returns 2');
    assert(state.getPatternCount() === 5, 'N=5');
    assert(state.getPattern(3).steps[0].note === 'C', 'P4 is copy of P1');
    assert(state.getPattern(4).steps[0].note === 'E', 'P5 is copy of P3 (checked-order)');
});

test('duplicateCheckedToBottom: appends to timelineDefault, not timelineChecked', () => {
    reset();
    state.addPattern(); // P2
    state.setChecked(0, true);
    const tlDefaultBefore = state.getTimelineDefault().slice();
    state.duplicateCheckedToBottom();
    const tlDefaultAfter = state.getTimelineDefault();
    assert(tlDefaultAfter.length === tlDefaultBefore.length + 1,
        'default timeline grew by one');
    assert(tlDefaultAfter[tlDefaultAfter.length - 1] === 3,
        'default timeline appended new pattern number 3');
    // timelineChecked may have other entries from setChecked but the
    // duplicate itself shouldn't add P3 to it.
    const tlCheckedAfter = state.getTimelineChecked();
    assert(!tlCheckedAfter.includes(3),
        'checked timeline does NOT include the new copy');
});

test('duplicateCheckedToBottom: caps at MAX_PATTERNS, returns truncated count', () => {
    reset();
    // Fill to 63 (we already have 1).
    for (let i = 0; i < 62; i++) state.addPattern();
    assert(state.getPatternCount() === 63, 'pre: N=63');
    // Check the last 5 → only 1 slot of room.
    state.setChecked(58, true);
    state.setChecked(59, true);
    state.setChecked(60, true);
    state.setChecked(61, true);
    state.setChecked(62, true);
    const added = state.duplicateCheckedToBottom();
    assert(added === 1, 'returns 1 (capped)');
    assert(state.getPatternCount() === 64, 'N=64 (cap)');
    // Second call: no room.
    const added2 = state.duplicateCheckedToBottom();
    assert(added2 === 0, 'second call returns 0');
});

test('duplicateCheckedToBottom: empty checked → returns 0, no-op', () => {
    reset();
    state.addPattern();
    const before = state.getPatternCount();
    const added = state.duplicateCheckedToBottom();
    assert(added === 0, 'returns 0');
    assert(state.getPatternCount() === before, 'count unchanged');
});

test('deleteCheckedPatterns: P1+P3 checked of [P1,P2,P3] → leaves [P2]', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setStep(0, 0, { note: 'C',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(1, 0, { note: 'D',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'E',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setChecked(0, true);
    state.setChecked(2, true);
    const removed = state.deleteCheckedPatterns();
    assert(removed === 2, 'returns 2');
    assert(state.getPatternCount() === 1, 'N=1');
    assert(state.getPattern(0).steps[0].note === 'D', 'survivor is original P2');
});

test('deleteCheckedPatterns: deleting all patterns floors to single default', () => {
    reset();
    state.addPattern();
    // Mutate so the floor reset is observable.
    state.setStep(0, 0, { note: 'D#', transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    state.setChecked(0, true);
    state.setChecked(1, true);
    state.deleteCheckedPatterns();
    assert(state.getPatternCount() === 1, 'N=1 (floor preserved)');
    assert(state.getPattern(0).steps[0].note === 'C', 'survivor was reset to default');
    assert(state.getCheckedSet().size === 0, 'checked cleared');
});

test('deleteCheckedPatterns: empty checked → returns 0, no-op', () => {
    reset();
    state.addPattern();
    const before = state.getPatternCount();
    const removed = state.deleteCheckedPatterns();
    assert(removed === 0, 'returns 0');
    assert(state.getPatternCount() === before, 'count unchanged');
});

test('shiftStepsBulk applies to every listed pattern, single notify', () => {
    reset();
    state.addPattern();
    state.addPattern();
    // Mark step[0] in each to verify the rotation lands.
    state.setStep(0, 0, { note: 'C',  transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    state.setStep(1, 0, { note: 'D',  transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'E',  transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    let notifyCount = 0;
    state.onChange(() => { notifyCount++; });
    state.shiftStepsBulk([0, 2], 1);
    assert(notifyCount === 1, 'one notify for bulk');
    assert(state.getStep(0, 1).accent === true, 'P1 step rotated +1');
    assert(state.getStep(1, 0).accent === true, 'P2 untouched (not in list)');
    assert(state.getStep(2, 1).accent === true, 'P3 step rotated +1');
});

test('shiftStepsBulk no-op on empty list / zero shift', () => {
    reset();
    state.addPattern();
    state.setStep(0, 0, { note: 'C', transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    state.shiftStepsBulk([], 1);
    assert(state.getStep(0, 0).accent === true, 'empty list: no rotation');
    state.shiftStepsBulk([0], 16);
    assert(state.getStep(0, 0).accent === true, 'shift normalizes to 0: no rotation');
});

test('transposeBulk applies +1 to every listed pattern, single notify', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setStep(0, 0, { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(1, 0, { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    let notifyCount = 0;
    state.onChange(() => { notifyCount++; });
    state.transposeBulk([0, 2], 1);
    assert(notifyCount === 1, 'one notify for bulk');
    assert(state.getStep(0, 0).note === 'C#', 'P1 C → C# (+1)');
    assert(state.getStep(1, 0).note === 'C',  'P2 untouched (not in list)');
    assert(state.getStep(2, 0).note === 'C#', 'P3 C → C# (+1)');
});

test('transposeBulk no-op on empty list / zero delta', () => {
    reset();
    state.setStep(0, 0, { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' });
    state.transposeBulk([], 1);
    assert(state.getStep(0, 0).note === 'C', 'empty list: no transpose');
    state.transposeBulk([0], 0);
    assert(state.getStep(0, 0).note === 'C', 'zero delta: no transpose');
});

test('shuffleSteps preserves the multiset of step contents', () => {
    reset();
    const distinct = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G',
                      'G#', 'A', 'A#', 'B', 'C^', 'C', 'D', 'E'];
    for (let i = 0; i < 16; i++) {
        state.setStep(0, i, {
            note: distinct[i],
            transpose: 'NORMAL',
            accent: (i % 2) === 0,
            slide:  (i % 3) === 0,
            time:   'NORMAL',
        });
    }
    const before = Array.from({ length: 16 }, (_, i) => ({ ...state.getStep(0, i) }));
    state.shuffleSteps(0);
    const after = Array.from({ length: 16 }, (_, i) => ({ ...state.getStep(0, i) }));
    const key = (s) => `${s.note}|${s.transpose}|${s.accent}|${s.slide}|${s.time}`;
    const a = before.map(key).sort();
    const b = after.map(key).sort();
    assert(a.length === b.length && a.every((v, i) => v === b[i]),
           'shuffled pattern preserves the multiset of step entries');
    assert(after.length === 16, 'still 16 steps after shuffle');
});

test('shuffleStepsBulk applies to every listed pattern, single notify', () => {
    reset();
    state.addPattern();
    state.addPattern();
    state.setStep(0, 0, { note: 'C',  transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    state.setStep(2, 0, { note: 'E',  transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    let notifyCount = 0;
    state.onChange(() => { notifyCount++; });
    state.shuffleStepsBulk([0, 2]);
    assert(notifyCount === 1, 'one notify for bulk shuffle');
});

test('shuffleStepsBulk no-op on empty list', () => {
    reset();
    state.setStep(0, 0, { note: 'C', transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' });
    let notifyCount = 0;
    state.onChange(() => { notifyCount++; });
    state.shuffleStepsBulk([]);
    assert(notifyCount === 0, 'empty list: no notify');
    assert(state.getStep(0, 0).accent === true, 'empty list: pattern untouched');
});

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
