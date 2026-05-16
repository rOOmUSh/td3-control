// Usage: node ui/js/multipattern/multipattern-snapshot.test.js
//
// Locks down the pure helpers for the N=64 overflow flow:
//   - toDashedSlotKey: frontend label → backend dashed slot_key
//   - buildSnapshotSlots: 64 patterns + mode → 64 {slot_key, pattern} pairs
//     in canonical ALTERNATE/SERIAL order, scratch NOT excluded
//   - buildOverflowDeviceTargets: 64 patterns + scratch + mode → first 63
//     non-scratch device targets + aligned pattern subset
//   - defaultSnapshotName: deterministic "main-overflow-YYYY-MM-DD"
//
// The DOM flow (openOverflowPushFlow) is covered by manual browser
// verification - same contract as multipattern-push.js::init.

import {
    toDashedSlotKey,
    buildSnapshotSlots,
    buildOverflowDeviceTargets,
    defaultSnapshotName,
} from './multipattern-snapshot.js';
import { SLOT_COUNT_TOTAL, SLOT_COUNT_AFTER_SCRATCH } from '../shared/slot-targets.js';

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

function makeMockPattern(tag) {
    // Minimal pattern-shaped object - the helpers only pass it through, so
    // identity is all that matters for the tests.
    return { __tag: tag };
}

console.log('multipattern-snapshot tests:');

// --- toDashedSlotKey --------------------------------------------------------

test('toDashedSlotKey G1P1A → G1-P1A', () => {
    assert(toDashedSlotKey('G1P1A') === 'G1-P1A', `got ${toDashedSlotKey('G1P1A')}`);
});
test('toDashedSlotKey G4P8B → G4-P8B', () => {
    assert(toDashedSlotKey('G4P8B') === 'G4-P8B', `got ${toDashedSlotKey('G4P8B')}`);
});
test('toDashedSlotKey rejects malformed input', () => {
    assert(toDashedSlotKey('G5P1A') === null, 'G5 (>4) should be null');
    assert(toDashedSlotKey('G1P9A') === null, 'P9 (>8) should be null');
    assert(toDashedSlotKey('G1P1C') === null, 'side C should be null');
    assert(toDashedSlotKey('G1-P1A') === null, 'already-dashed should be null');
    assert(toDashedSlotKey('') === null, 'empty should be null');
    assert(toDashedSlotKey(null) === null, 'null should be null');
    assert(toDashedSlotKey(42) === null, 'number should be null');
});

// --- defaultSnapshotName ----------------------------------------------------

test('defaultSnapshotName uses local YYYY-MM-DD with zero-padded month/day', () => {
    // 2026 Jan 5 (month index 0, day 5) → "main-overflow-2026-01-05".
    const fixed = new Date(2026, 0, 5, 12, 30, 0);
    const name = defaultSnapshotName(fixed);
    assert(name === 'main-overflow-2026-01-05', `got '${name}'`);
});

test('defaultSnapshotName on month+day boundaries keeps 2-digit padding', () => {
    const dec31 = new Date(2026, 11, 31, 23, 59, 59);
    const name = defaultSnapshotName(dec31);
    assert(name === 'main-overflow-2026-12-31', `got '${name}'`);
});

// --- buildSnapshotSlots -----------------------------------------------------

test('buildSnapshotSlots rejects wrong array length', () => {
    const { slots, error } = buildSnapshotSlots([makeMockPattern('p1')], 'ALTERNATE');
    assert(error === 'bad-n', `expected 'bad-n', got ${error}`);
    assert(slots === null, 'expected null slots');
});

test('buildSnapshotSlots rejects non-array input', () => {
    assert(buildSnapshotSlots(null, 'ALTERNATE').error === 'bad-n', 'null');
    assert(buildSnapshotSlots({ length: 64 }, 'ALTERNATE').error === 'bad-n', 'object with length');
});

test('buildSnapshotSlots rejects bad mode', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const { slots, error } = buildSnapshotSlots(patterns, 'BOGUS');
    assert(error === 'bad-mode', `expected 'bad-mode', got ${error}`);
    assert(slots === null, 'expected null slots');
});

test('buildSnapshotSlots ALTERNATE: 64 pairs, dashed keys, first+last canonical', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const { slots, error } = buildSnapshotSlots(patterns, 'ALTERNATE');
    assert(error === null, `unexpected error ${error}`);
    assert(slots.length === SLOT_COUNT_TOTAL, `expected 64, got ${slots.length}`);
    assert(slots[0].slot_key === 'G1-P1A', `slots[0]=${slots[0].slot_key}`);
    assert(slots[1].slot_key === 'G1-P1B', `slots[1]=${slots[1].slot_key}`);
    assert(slots[15].slot_key === 'G1-P8B', `slots[15]=${slots[15].slot_key}`);
    assert(slots[16].slot_key === 'G2-P1A', `slots[16]=${slots[16].slot_key}`);
    assert(slots[63].slot_key === 'G4-P8B', `slots[63]=${slots[63].slot_key}`);
    // Pattern objects must be handed back in UI-index order.
    assert(slots[0].pattern.__tag === 'p0', 'slot 0 pattern tag');
    assert(slots[63].pattern.__tag === 'p63', 'slot 63 pattern tag');
});

test('buildSnapshotSlots SERIAL: 32 As then 32 Bs, canonical', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const { slots, error } = buildSnapshotSlots(patterns, 'SERIAL');
    assert(error === null, `unexpected error ${error}`);
    assert(slots.length === 64, `expected 64, got ${slots.length}`);
    // First 32 are As: G1-P1A .. G4-P8A
    assert(slots[0].slot_key === 'G1-P1A', `[0]=${slots[0].slot_key}`);
    assert(slots[31].slot_key === 'G4-P8A', `[31]=${slots[31].slot_key}`);
    // Next 32 are Bs: G1-P1B .. G4-P8B
    assert(slots[32].slot_key === 'G1-P1B', `[32]=${slots[32].slot_key}`);
    assert(slots[63].slot_key === 'G4-P8B', `[63]=${slots[63].slot_key}`);
});

test('buildSnapshotSlots: every slot_key is unique across all 64', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        const { slots } = buildSnapshotSlots(patterns, mode);
        const seen = new Set();
        for (const s of slots) {
            assert(!seen.has(s.slot_key), `duplicate ${s.slot_key} in ${mode}`);
            seen.add(s.slot_key);
        }
        assert(seen.size === 64, `${mode}: expected 64 unique keys, got ${seen.size}`);
    }
});

test('buildSnapshotSlots: patterns are passed through by reference (no clone)', () => {
    const one = makeMockPattern('sentinel');
    const patterns = Array.from({ length: 64 }, () => makeMockPattern('other'));
    patterns[17] = one;
    const { slots } = buildSnapshotSlots(patterns, 'ALTERNATE');
    assert(slots[17].pattern === one, 'expected same reference at idx 17');
});

// --- buildOverflowDeviceTargets --------------------------------------------

test('buildOverflowDeviceTargets rejects wrong n', () => {
    const { targets, error } = buildOverflowDeviceTargets(
        [makeMockPattern('p1')], { group: 1, pattern: 1, side: 'A' }, 'ALTERNATE'
    );
    assert(error === 'bad-n', `expected 'bad-n', got ${error}`);
    assert(targets === null, 'expected null targets');
});

test('buildOverflowDeviceTargets requires scratch', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const { targets, error } = buildOverflowDeviceTargets(patterns, null, 'ALTERNATE');
    assert(error === 'no-scratch', `expected 'no-scratch', got ${error}`);
    assert(targets === null, 'expected null targets');
});

test('buildOverflowDeviceTargets ALTERNATE with scratch G1P1A → 63 targets excluding scratch', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const scratch = { group: 1, pattern: 1, side: 'A', label: 'G1P1A' };
    const { targets, patternsToWrite, error } = buildOverflowDeviceTargets(patterns, scratch, 'ALTERNATE');
    assert(error === null, `unexpected error ${error}`);
    assert(targets.length === SLOT_COUNT_AFTER_SCRATCH, `expected 63, got ${targets.length}`);
    assert(patternsToWrite.length === 63, `expected 63 patterns, got ${patternsToWrite.length}`);
    // Scratch must not appear in targets.
    const scratchPresent = targets.some(
        (t) => t.group === 1 && t.pattern === 1 && t.side === 'A',
    );
    assert(!scratchPresent, 'scratch leaked into targets');
    // First target should be G1P1B (ALTERNATE skips the A scratch slot).
    assert(targets[0].label === 'G1P1B', `[0]=${targets[0].label}`);
    // patternsToWrite is the first 63 UI patterns (P64 dropped from device).
    assert(patternsToWrite[0].__tag === 'p0', 'first pattern tag');
    assert(patternsToWrite[62].__tag === 'p62', 'last device pattern tag');
    // P64 (patterns[63]) is NOT in patternsToWrite - it lives in the snapshot only.
    assert(!patternsToWrite.includes(patterns[63]), 'p63 (UI P64) leaked into device writes');
});

test('buildOverflowDeviceTargets SERIAL with scratch G2P5B → 63 targets', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const scratch = { group: 2, pattern: 5, side: 'B', label: 'G2P5B' };
    const { targets, patternsToWrite } = buildOverflowDeviceTargets(patterns, scratch, 'SERIAL');
    assert(targets.length === 63, `expected 63, got ${targets.length}`);
    assert(patternsToWrite.length === 63, `expected 63 patterns`);
    const scratchPresent = targets.some(
        (t) => t.group === 2 && t.pattern === 5 && t.side === 'B',
    );
    assert(!scratchPresent, 'scratch leaked into SERIAL targets');
});

test('buildOverflowDeviceTargets target uniqueness (no duplicates) for all 63', () => {
    const patterns = Array.from({ length: 64 }, (_, i) => makeMockPattern(`p${i}`));
    const scratch = { group: 3, pattern: 4, side: 'A', label: 'G3P4A' };
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        const { targets } = buildOverflowDeviceTargets(patterns, scratch, mode);
        const seen = new Set();
        for (const t of targets) {
            const key = `${t.group}/${t.pattern}/${t.side}`;
            assert(!seen.has(key), `duplicate ${t.label} in ${mode}`);
            seen.add(key);
        }
        assert(seen.size === 63, `${mode}: expected 63 unique, got ${seen.size}`);
    }
});

// --- summary ---------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
