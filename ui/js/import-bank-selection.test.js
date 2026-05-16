// Usage: node ui/js/import-bank-selection.test.js
//
// Locks down the pure selection logic for IMPORT multi-select.
// The DOM wiring in `import-bank-picker.js` (grid rendering, primary-button
// state, double-click commit, playback tracker teardown) is covered by
// manual browser verification. These tests exercise `applyClick` and
// `patternsFromSelection` so the click-math rules can't drift silently.

import {
    applyClick,
    patternsFromSelection,
} from './import-bank-selection.js';

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) {
        console.error(`  FAIL: ${msg}`);
        failed++;
        return;
    }
    passed++;
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (err) {
        console.error(`  FAIL: ${name}: ${err.stack || err.message}`);
        failed++;
    }
}

// Build a grid of `count` slots with simple slot_keys and a payload for
// populated ones. `emptyIdx` is an optional Set of indexes that should be
// marked empty (no pattern payload).
function buildSlots(count, emptyIdx = new Set()) {
    return Array.from({ length: count }, (_, i) => {
        const empty = emptyIdx.has(i);
        return {
            slot_key: `S${i}`,
            empty,
            pattern: empty ? null : { __tag: `p${i}` },
        };
    });
}

console.log('import-bank-selection tests:');

// --- applyClick: single mode (backward compat) -----------------------------

test('applyClick: single mode → any click produces a one-element selection', () => {
    const slots = buildSlots(5);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0']),
        anchorKey: 'S0',
        clickedKey: 'S2',
        multi: false,
    });
    assert(r.keys.size === 1, `size=${r.keys.size}`);
    assert(r.keys.has('S2'), 'expected S2');
    assert(r.anchorKey === 'S2', `anchor=${r.anchorKey}`);
});

test('applyClick: single mode ignores ctrl/shift modifiers', () => {
    const slots = buildSlots(5);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0', 'S1']),
        anchorKey: 'S0',
        clickedKey: 'S3',
        ctrlKey: true,
        shiftKey: true,
        multi: false,
    });
    assert(r.keys.size === 1 && r.keys.has('S3'), 'expected {S3}');
});

test('applyClick: click on empty slot is a no-op', () => {
    const slots = buildSlots(5, new Set([2]));
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0']),
        anchorKey: 'S0',
        clickedKey: 'S2',
        multi: true,
    });
    assert(r.keys.size === 1 && r.keys.has('S0'), 'selection unchanged');
    assert(r.anchorKey === 'S0', 'anchor unchanged');
});

// --- applyClick: multi mode -----------------------------------------------

test('applyClick: multi bare click resets to just the clicked slot', () => {
    const slots = buildSlots(5);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0', 'S2']),
        anchorKey: 'S2',
        clickedKey: 'S4',
        multi: true,
    });
    assert(r.keys.size === 1 && r.keys.has('S4'), 'expected {S4}');
    assert(r.anchorKey === 'S4', `anchor=${r.anchorKey}`);
});

test('applyClick: multi ctrl+click toggles membership (add)', () => {
    const slots = buildSlots(5);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0']),
        anchorKey: 'S0',
        clickedKey: 'S3',
        ctrlKey: true,
        multi: true,
    });
    assert(r.keys.size === 2, `size=${r.keys.size}`);
    assert(r.keys.has('S0') && r.keys.has('S3'), 'expected {S0,S3}');
    assert(r.anchorKey === 'S3', `anchor=${r.anchorKey}`);
});

test('applyClick: multi ctrl+click toggles membership (remove)', () => {
    const slots = buildSlots(5);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0', 'S3']),
        anchorKey: 'S3',
        clickedKey: 'S3',
        ctrlKey: true,
        multi: true,
    });
    assert(r.keys.size === 1 && r.keys.has('S0'), 'expected {S0}');
});

test('applyClick: multi shift+click selects inclusive range in grid order', () => {
    const slots = buildSlots(10);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S2']),
        anchorKey: 'S2',
        clickedKey: 'S5',
        shiftKey: true,
        multi: true,
    });
    assert(r.keys.size === 4, `size=${r.keys.size}`);
    for (const k of ['S2', 'S3', 'S4', 'S5']) {
        assert(r.keys.has(k), `expected ${k}`);
    }
    assert(r.anchorKey === 'S2', 'anchor unchanged by shift');
});

test('applyClick: multi shift+click range works backward too', () => {
    const slots = buildSlots(10);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S6']),
        anchorKey: 'S6',
        clickedKey: 'S3',
        shiftKey: true,
        multi: true,
    });
    assert(r.keys.size === 4, `size=${r.keys.size}`);
    for (const k of ['S3', 'S4', 'S5', 'S6']) {
        assert(r.keys.has(k), `expected ${k}`);
    }
    assert(r.anchorKey === 'S6', 'anchor unchanged');
});

test('applyClick: multi shift+click skips empty slots inside range', () => {
    // S3 and S5 are empty - a shift range from S2 to S6 must pick up only
    // the populated cells (S2, S4, S6).
    const slots = buildSlots(8, new Set([3, 5]));
    const r = applyClick({
        slots,
        currentKeys: new Set(['S2']),
        anchorKey: 'S2',
        clickedKey: 'S6',
        shiftKey: true,
        multi: true,
    });
    assert(r.keys.size === 3, `size=${r.keys.size}`);
    assert(r.keys.has('S2') && r.keys.has('S4') && r.keys.has('S6'), 'populated only');
    assert(!r.keys.has('S3') && !r.keys.has('S5'), 'empties excluded');
});

test('applyClick: multi shift+click without anchor falls back to bare click', () => {
    const slots = buildSlots(5);
    const r = applyClick({
        slots,
        currentKeys: new Set(['S0', 'S2']),
        anchorKey: null,
        clickedKey: 'S4',
        shiftKey: true,
        multi: true,
    });
    assert(r.keys.size === 1 && r.keys.has('S4'), 'expected {S4}');
    assert(r.anchorKey === 'S4', 'anchor set to clicked');
});

test('applyClick: does not mutate caller currentKeys', () => {
    const slots = buildSlots(5);
    const orig = new Set(['S0', 'S1']);
    applyClick({
        slots,
        currentKeys: orig,
        anchorKey: 'S0',
        clickedKey: 'S2',
        ctrlKey: true,
        multi: true,
    });
    assert(orig.size === 2, 'caller set size preserved');
    assert(orig.has('S0') && orig.has('S1'), 'caller membership preserved');
});

// --- patternsFromSelection -----------------------------------------------

test('patternsFromSelection: returns patterns in grid order', () => {
    const slots = buildSlots(6);
    // Selection order doesn't matter - output follows grid order.
    const out = patternsFromSelection(slots, new Set(['S4', 'S1', 'S2']));
    assert(out.length === 3, `length=${out.length}`);
    assert(out[0].__tag === 'p1', `[0]=${out[0].__tag}`);
    assert(out[1].__tag === 'p2', `[1]=${out[1].__tag}`);
    assert(out[2].__tag === 'p4', `[2]=${out[2].__tag}`);
});

test('patternsFromSelection: empty slots dropped from output', () => {
    const slots = buildSlots(6, new Set([2]));
    // S2 is empty; even if somehow selected it must not appear.
    const out = patternsFromSelection(slots, new Set(['S1', 'S2', 'S3']));
    assert(out.length === 2, `length=${out.length}`);
    assert(out[0].__tag === 'p1' && out[1].__tag === 'p3', 'p1, p3');
});

test('patternsFromSelection: empty selection → empty array', () => {
    const slots = buildSlots(6);
    const out = patternsFromSelection(slots, new Set());
    assert(Array.isArray(out) && out.length === 0, `got ${JSON.stringify(out)}`);
});

test('patternsFromSelection: accepts any iterable (array) for keys', () => {
    const slots = buildSlots(6);
    const out = patternsFromSelection(slots, ['S0', 'S2']);
    assert(out.length === 2, `length=${out.length}`);
    assert(out[0].__tag === 'p0' && out[1].__tag === 'p2', 'p0, p2');
});

// --- summary ---------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
