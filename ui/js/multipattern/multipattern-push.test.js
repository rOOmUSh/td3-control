// Usage: node ui/js/multipattern/multipattern-push.test.js
//
// Exercises the pure target-build helper for main-page PUSH TO TD-3.
// The DOM wiring + modal rendering live in init() and are covered by
// manual browser verification (same contract as progression-push.test.js);
// this file locks down the slot sequence PUSH will actually write to.

import { buildPushTargets } from './multipattern-push.js';
import { SLOT_COUNT_AFTER_SCRATCH } from '../shared/slot-targets.js';

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

console.log('multipattern-push tests:');

// --- basic shape ------------------------------------------------------------

test('N=1, no scratch, ALTERNATE → [G1P1A]', () => {
    const { targets, error } = buildPushTargets(1, null, 'ALTERNATE');
    assert(error === null, `unexpected error ${error}`);
    assert(targets.length === 1, `expected 1 target, got ${targets.length}`);
    assert(targets[0].label === 'G1P1A', `expected G1P1A, got ${targets[0].label}`);
});

test('N=1, no scratch, SERIAL → [G1P1A]', () => {
    const { targets } = buildPushTargets(1, null, 'SERIAL');
    assert(targets[0].label === 'G1P1A', `expected G1P1A, got ${targets[0].label}`);
});

test('N=0 yields empty target list, no error', () => {
    const { targets, error } = buildPushTargets(0, null, 'ALTERNATE');
    assert(error === null, `unexpected error ${error}`);
    assert(Array.isArray(targets) && targets.length === 0, 'expected empty array');
});

test('N=4 ALTERNATE, scratch G1P2A → G1P1A, G1P1B, G1P2B, G1P3A', () => {
    const scratch = { group: 1, pattern: 2, side: 'A' };
    const { targets } = buildPushTargets(4, scratch, 'ALTERNATE');
    const labels = targets.map(t => t.label);
    assert(labels[0] === 'G1P1A', `[0]=${labels[0]}`);
    assert(labels[1] === 'G1P1B', `[1]=${labels[1]}`);
    assert(labels[2] === 'G1P2B', `[2]=${labels[2]}`);
    assert(labels[3] === 'G1P3A', `[3]=${labels[3]}`);
});

test('N=4 SERIAL, scratch G1P1A → G1P2A, G1P3A, G1P4A, G1P5A', () => {
    const scratch = { group: 1, pattern: 1, side: 'A' };
    const { targets } = buildPushTargets(4, scratch, 'SERIAL');
    const labels = targets.map(t => t.label);
    assert(labels[0] === 'G1P2A', `[0]=${labels[0]}`);
    assert(labels[1] === 'G1P3A', `[1]=${labels[1]}`);
    assert(labels[2] === 'G1P4A', `[2]=${labels[2]}`);
    assert(labels[3] === 'G1P5A', `[3]=${labels[3]}`);
});

// --- scratch exclusion completeness ----------------------------------------

test('N=63 ALTERNATE with scratch fills exactly 63 unique slots', () => {
    const scratch = { group: 2, pattern: 5, side: 'B' };
    const { targets, error } = buildPushTargets(63, scratch, 'ALTERNATE');
    assert(error === null, `unexpected error ${error}`);
    assert(targets.length === 63, `expected 63, got ${targets.length}`);
    const seen = new Set();
    for (const t of targets) {
        const key = `${t.group}/${t.pattern}/${t.side}`;
        assert(!seen.has(key), `duplicate slot ${t.label}`);
        seen.add(key);
        assert(
            !(t.group === scratch.group && t.pattern === scratch.pattern && t.side === scratch.side),
            `scratch slot ${t.label} leaked into targets`,
        );
    }
    assert(seen.size === SLOT_COUNT_AFTER_SCRATCH, `expected 63 unique, got ${seen.size}`);
});

test('N=63 SERIAL with scratch fills exactly 63 unique slots', () => {
    const scratch = { group: 3, pattern: 7, side: 'A' };
    const { targets, error } = buildPushTargets(63, scratch, 'SERIAL');
    assert(error === null, `unexpected error ${error}`);
    assert(targets.length === 63, `expected 63, got ${targets.length}`);
    const seen = new Set();
    for (const t of targets) {
        const key = `${t.group}/${t.pattern}/${t.side}`;
        assert(!seen.has(key), `duplicate slot ${t.label}`);
        seen.add(key);
    }
    assert(seen.size === SLOT_COUNT_AFTER_SCRATCH, `expected 63 unique, got ${seen.size}`);
});

// --- overflow gating -------------------------------------------------------

test('N=64 with scratch → error=overflow', () => {
    const scratch = { group: 1, pattern: 1, side: 'A' };
    const { targets, error } = buildPushTargets(64, scratch, 'ALTERNATE');
    assert(error === 'overflow', `expected 'overflow', got ${error}`);
    assert(targets === null, 'expected null targets on overflow');
});

test('N=64 with NO scratch → succeeds (64 slots total)', () => {
    // During the brief window before scratch fetches, the 64-slot list has
    // no exclusion - PUSH simply shouldn't be clickable then (the chrome
    // gates it), but the helper stays mechanical about it.
    const { targets, error } = buildPushTargets(64, null, 'ALTERNATE');
    assert(error === null, `unexpected error ${error}`);
    assert(targets.length === 64, `expected 64, got ${targets.length}`);
});

// --- bad input -------------------------------------------------------------

test('N = -1 → error=bad-n', () => {
    const { error } = buildPushTargets(-1, null, 'ALTERNATE');
    assert(error === 'bad-n', `expected 'bad-n', got ${error}`);
});

test('N non-integer → error=bad-n', () => {
    const { error } = buildPushTargets(1.5, null, 'ALTERNATE');
    assert(error === 'bad-n', `expected 'bad-n', got ${error}`);
});

// --- ALTERNATE ordering formula ---------------------------------------------

test('ALTERNATE pre-scratch ordering interleaves A/B per pattern pair', () => {
    // No scratch → 64-slot list == orderedSlots('ALTERNATE').
    const { targets } = buildPushTargets(16, null, 'ALTERNATE');
    // First 16 slots are G1's full A/B interleave (P1A, P1B, P2A, P2B, ...).
    const labels = targets.map(t => t.label);
    assert(labels[0] === 'G1P1A', `[0]=${labels[0]}`);
    assert(labels[1] === 'G1P1B', `[1]=${labels[1]}`);
    assert(labels[2] === 'G1P2A', `[2]=${labels[2]}`);
    assert(labels[15] === 'G1P8B', `[15]=${labels[15]}`);
});

test('SERIAL pre-scratch ordering does As-then-Bs', () => {
    const { targets } = buildPushTargets(33, null, 'SERIAL');
    // 32nd slot (idx 31) is G4P8A (last A-side); 33rd (idx 32) is G1P1B (first B-side).
    assert(targets[31].label === 'G4P8A', `[31]=${targets[31].label}`);
    assert(targets[32].label === 'G1P1B', `[32]=${targets[32].label}`);
});

// --- summary ---------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
