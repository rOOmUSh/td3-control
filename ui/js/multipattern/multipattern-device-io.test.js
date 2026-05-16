// Usage: node ui/js/multipattern/multipattern-device-io.test.js
//
// Locks down the pure routing helpers for LOAD / LOAD ALL / SAVE:
// resolveSaveAction and buildSaveSelectionTargets. The DOM
// wiring in init()/wireLoad/wireLoadAll/wireSave is covered by manual
// browser verification (same contract as progression-push.js::init).

import {
    resolveSaveAction,
    buildSaveSelectionTargets,
} from './multipattern-device-io.js';

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

function mockPatterns(n) {
    return Array.from({ length: n }, (_, i) => ({ __tag: `p${i}` }));
}

console.log('multipattern-device-io tests:');

// --- resolveSaveAction -----------------------------------------------------

test('resolveSaveAction: empty selection → none', () => {
    const a = resolveSaveAction([]);
    assert(a.kind === 'none', `got ${a.kind}`);
});

test('resolveSaveAction: non-array → none', () => {
    assert(resolveSaveAction(null).kind === 'none', 'null');
    assert(resolveSaveAction(undefined).kind === 'none', 'undefined');
    assert(resolveSaveAction('nope').kind === 'none', 'string');
});

test('resolveSaveAction: single index → single action', () => {
    const a = resolveSaveAction([3]);
    assert(a.kind === 'single', `got ${a.kind}`);
    assert(a.index === 3, `got ${a.index}`);
});

test('resolveSaveAction: two+ indexes → multi, sorted', () => {
    const a = resolveSaveAction([5, 2, 7, 0]);
    assert(a.kind === 'multi', `got ${a.kind}`);
    assert(JSON.stringify(a.indexes) === '[0,2,5,7]', `got ${JSON.stringify(a.indexes)}`);
});

test('resolveSaveAction: does not mutate its input', () => {
    const input = [5, 2, 7];
    const snap = JSON.stringify(input);
    resolveSaveAction(input);
    assert(JSON.stringify(input) === snap, 'input was mutated');
});

// --- buildSaveSelectionTargets ---------------------------------------------

test('buildSaveSelectionTargets: rejects <2 indexes (not a multi save)', () => {
    assert(buildSaveSelectionTargets([], mockPatterns(4), null, 'ALTERNATE').error === 'bad-indexes', 'empty');
    assert(buildSaveSelectionTargets([0], mockPatterns(4), null, 'ALTERNATE').error === 'bad-indexes', 'single');
});

test('buildSaveSelectionTargets: rejects non-array patterns', () => {
    const r = buildSaveSelectionTargets([0, 1], null, null, 'ALTERNATE');
    assert(r.error === 'bad-patterns', `got ${r.error}`);
});

test('buildSaveSelectionTargets: rejects out-of-range index', () => {
    const r = buildSaveSelectionTargets([0, 4], mockPatterns(2), null, 'ALTERNATE');
    assert(r.error === 'index-out-of-range', `got ${r.error}`);
});

test('buildSaveSelectionTargets: indexes 0..2 ALTERNATE no scratch → first 3 slots', () => {
    const patterns = mockPatterns(10);
    const r = buildSaveSelectionTargets([0, 1, 2], patterns, null, 'ALTERNATE');
    assert(r.error === null, `unexpected error ${r.error}`);
    assert(r.targets.length === 3, `expected 3 targets, got ${r.targets.length}`);
    assert(r.targets[0].label === 'G1P1A', `[0]=${r.targets[0].label}`);
    assert(r.targets[1].label === 'G1P1B', `[1]=${r.targets[1].label}`);
    assert(r.targets[2].label === 'G1P2A', `[2]=${r.targets[2].label}`);
    assert(r.patternsToWrite[0].__tag === 'p0', 'p0 tag');
    assert(r.patternsToWrite[1].__tag === 'p1', 'p1 tag');
    assert(r.patternsToWrite[2].__tag === 'p2', 'p2 tag');
});

test('buildSaveSelectionTargets: sparse selection picks the right slots', () => {
    // Select UI indexes 0, 3, 7. With no scratch + ALTERNATE the 64-slot
    // list is [G1P1A, G1P1B, G1P2A, G1P2B, G1P3A, G1P3B, G1P4A, G1P4B, ...].
    const patterns = mockPatterns(10);
    const r = buildSaveSelectionTargets([0, 3, 7], patterns, null, 'ALTERNATE');
    assert(r.error === null, `unexpected error ${r.error}`);
    assert(r.targets[0].label === 'G1P1A', `[0]=${r.targets[0].label}`);
    assert(r.targets[1].label === 'G1P2B', `[1]=${r.targets[1].label}`);
    assert(r.targets[2].label === 'G1P4B', `[2]=${r.targets[2].label}`);
    // patternsToWrite must carry the actual pattern objects, not the
    // slot-aligned ones (P0 goes to G1P1A, P3 goes to G1P2B, P7 goes to G1P4B).
    assert(r.patternsToWrite[0].__tag === 'p0', 'p0 tag');
    assert(r.patternsToWrite[1].__tag === 'p3', 'p3 tag');
    assert(r.patternsToWrite[2].__tag === 'p7', 'p7 tag');
});

test('buildSaveSelectionTargets: scratch excluded from target sequence', () => {
    // scratch at G1P1A. Selection indexes 0, 1 (ALTERNATE) should hit
    // G1P1B (first non-scratch slot) and G1P2A (second).
    const patterns = mockPatterns(5);
    const scratch = { group: 1, pattern: 1, side: 'A', label: 'G1P1A' };
    const r = buildSaveSelectionTargets([0, 1], patterns, scratch, 'ALTERNATE');
    assert(r.error === null, `unexpected error ${r.error}`);
    assert(r.targets[0].label === 'G1P1B', `[0]=${r.targets[0].label}`);
    assert(r.targets[1].label === 'G1P2A', `[1]=${r.targets[1].label}`);
});

test('buildSaveSelectionTargets: selecting idx 63 with scratch → overflow error', () => {
    // With scratch present, idx 63 is the snapshot-only pattern.
    // SAVE must surface this as `overflow`, not silently drop.
    const patterns = mockPatterns(64);
    const scratch = { group: 1, pattern: 1, side: 'A', label: 'G1P1A' };
    const r = buildSaveSelectionTargets([10, 63], patterns, scratch, 'ALTERNATE');
    assert(r.error === 'overflow', `got ${r.error}`);
    assert(r.targets === null, 'expected null targets');
});

test('buildSaveSelectionTargets: SERIAL mode walks As then Bs', () => {
    // SERIAL 64-slot order: 32 As (G1P1A..G4P8A) then 32 Bs (G1P1B..G4P8B).
    // Selection [31, 32] (boundary) should yield G4P8A and G1P1B.
    const patterns = mockPatterns(40);
    const r = buildSaveSelectionTargets([31, 32], patterns, null, 'SERIAL');
    assert(r.error === null, `unexpected error ${r.error}`);
    assert(r.targets[0].label === 'G4P8A', `[0]=${r.targets[0].label}`);
    assert(r.targets[1].label === 'G1P1B', `[1]=${r.targets[1].label}`);
});

// --- summary ---------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
