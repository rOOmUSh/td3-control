// Usage: node ui/js/shared/slot-targets.test.js
//
// Exercises the A/B slot ordering + scratch-exclusion helper.
// These are the rules that drive main-page
// badges, PUSH TO TD-3 target resolution, and LOAD ALL ingestion order,
// so regressions would silently corrupt user-visible slot mapping.

import {
    orderedSlots,
    orderedSlotsExcludingScratch,
    rotateToStart,
    slotFor,
    parseSlotLabel,
    SLOT_COUNT_TOTAL,
    SLOT_COUNT_AFTER_SCRATCH,
} from './slot-targets.js';

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

console.log('slot-targets tests:');

// --- ordered list shape / completeness -------------------------------------

test('orderedSlots ALTERNATE returns exactly 64 entries', () => {
    const list = orderedSlots('ALTERNATE');
    assert(list.length === SLOT_COUNT_TOTAL, `expected 64, got ${list.length}`);
});

test('orderedSlots SERIAL returns exactly 64 entries', () => {
    const list = orderedSlots('SERIAL');
    assert(list.length === SLOT_COUNT_TOTAL, `expected 64, got ${list.length}`);
});

test('orderedSlots covers every (group, pattern, side) triple once', () => {
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        const list = orderedSlots(mode);
        const keys = new Set(list.map((s) => s.label));
        assert(keys.size === 64, `[${mode}] expected 64 unique labels, got ${keys.size}`);
        for (let g = 1; g <= 4; g++) {
            for (let p = 1; p <= 8; p++) {
                for (const side of ['A', 'B']) {
                    const label = `G${g}P${p}${side}`;
                    assert(keys.has(label), `[${mode}] missing ${label}`);
                }
            }
        }
    }
});

test('orderedSlots rejects unknown modes', () => {
    let threw = false;
    try { orderedSlots('INVALID'); } catch (_e) { threw = true; }
    assert(threw, 'expected throw for unknown mode');
});

// --- ALTERNATE ordering spot checks ----------------------------------------

test('ALTERNATE: i=0 → G1P1A, i=1 → G1P1B (A/B pair interleaved)', () => {
    const list = orderedSlots('ALTERNATE');
    assert(list[0].label === 'G1P1A', `expected G1P1A, got ${list[0].label}`);
    assert(list[1].label === 'G1P1B', `expected G1P1B, got ${list[1].label}`);
});

test('ALTERNATE: i=15 → G1P8B (last slot of group 1)', () => {
    const list = orderedSlots('ALTERNATE');
    assert(list[15].label === 'G1P8B', `expected G1P8B, got ${list[15].label}`);
});

test('ALTERNATE: i=16 → G2P1A (first slot of group 2)', () => {
    const list = orderedSlots('ALTERNATE');
    assert(list[16].label === 'G2P1A', `expected G2P1A, got ${list[16].label}`);
});

test('ALTERNATE: i=63 → G4P8B (last slot)', () => {
    const list = orderedSlots('ALTERNATE');
    assert(list[63].label === 'G4P8B', `expected G4P8B, got ${list[63].label}`);
});

test('ALTERNATE: index-to-side matches floor(i/16)/i%2 rule', () => {
    const list = orderedSlots('ALTERNATE');
    for (let i = 0; i < 64; i++) {
        const expectedSide = i % 2 === 0 ? 'A' : 'B';
        const expectedGroup = Math.floor(i / 16) + 1;
        const expectedPattern = Math.floor((i % 16) / 2) + 1;
        assert(list[i].side === expectedSide, `i=${i} side expected ${expectedSide}, got ${list[i].side}`);
        assert(list[i].group === expectedGroup, `i=${i} group expected ${expectedGroup}, got ${list[i].group}`);
        assert(list[i].pattern === expectedPattern, `i=${i} pattern expected ${expectedPattern}, got ${list[i].pattern}`);
    }
});

// --- SERIAL ordering spot checks -------------------------------------------

test('SERIAL: i=0 → G1P1A, i=7 → G1P8A (A-side group 1 bloc)', () => {
    const list = orderedSlots('SERIAL');
    assert(list[0].label === 'G1P1A', `expected G1P1A, got ${list[0].label}`);
    assert(list[7].label === 'G1P8A', `expected G1P8A, got ${list[7].label}`);
});

test('SERIAL: i=31 → G4P8A (last A-side slot)', () => {
    const list = orderedSlots('SERIAL');
    assert(list[31].label === 'G4P8A', `expected G4P8A, got ${list[31].label}`);
});

test('SERIAL: i=32 → G1P1B (first B-side slot)', () => {
    const list = orderedSlots('SERIAL');
    assert(list[32].label === 'G1P1B', `expected G1P1B, got ${list[32].label}`);
});

test('SERIAL: i=63 → G4P8B (last slot)', () => {
    const list = orderedSlots('SERIAL');
    assert(list[63].label === 'G4P8B', `expected G4P8B, got ${list[63].label}`);
});

test('SERIAL: first 32 are all A, last 32 are all B', () => {
    const list = orderedSlots('SERIAL');
    for (let i = 0; i < 32; i++) assert(list[i].side === 'A', `i=${i} expected A, got ${list[i].side}`);
    for (let i = 32; i < 64; i++) assert(list[i].side === 'B', `i=${i} expected B, got ${list[i].side}`);
});

// --- Scratch exclusion -----------------------------------------------------

test('orderedSlotsExcludingScratch drops exactly one slot when scratch matches', () => {
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        for (const target of ['G1P1A', 'G1P2A', 'G1P2B', 'G4P8A', 'G4P8B']) {
            const list = orderedSlotsExcludingScratch(mode, parseSlotLabel(target));
            assert(list.length === SLOT_COUNT_AFTER_SCRATCH,
                `[${mode}/${target}] expected 63 slots, got ${list.length}`);
            assert(!list.some((s) => s.label === target),
                `[${mode}/${target}] scratch slot ${target} still present`);
        }
    }
});

test('orderedSlotsExcludingScratch with null scratch keeps all 64', () => {
    const list = orderedSlotsExcludingScratch('ALTERNATE', null);
    assert(list.length === 64, `expected 64, got ${list.length}`);
});

// --- slotFor --------------------------------------------------------------

test('slotFor ALTERNATE + scratch=G1P2A: idx 0 → G1P1A, idx 1 → G1P1B, idx 2 → G1P2B', () => {
    const scratch = parseSlotLabel('G1P2A');
    assert(slotFor(0, scratch, 'ALTERNATE').label === 'G1P1A');
    assert(slotFor(1, scratch, 'ALTERNATE').label === 'G1P1B');
    assert(slotFor(2, scratch, 'ALTERNATE').label === 'G1P2B',
        `idx 2 expected G1P2B, got ${slotFor(2, scratch, 'ALTERNATE').label}`);
});

test('slotFor SERIAL + scratch in A-side: i=30 lands on G4P7A, i=31 lands on G4P8A', () => {
    // scratch=G1P1A removes the first slot, shifting everything up by 1
    const scratch = parseSlotLabel('G1P1A');
    // With G1P1A removed, the 32 A-side slots now span indices 0..30, and
    // index 30 is G4P8A (old index 31). Index 31 is the first B-side slot.
    assert(slotFor(30, scratch, 'SERIAL').label === 'G4P8A',
        `expected G4P8A, got ${slotFor(30, scratch, 'SERIAL').label}`);
    assert(slotFor(31, scratch, 'SERIAL').label === 'G1P1B',
        `expected G1P1B, got ${slotFor(31, scratch, 'SERIAL').label}`);
});

test('slotFor returns null for overflow (idx 63 with scratch present)', () => {
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        for (const target of ['G1P2A', 'G4P8B']) {
            const scratch = parseSlotLabel(target);
            assert(slotFor(63, scratch, mode) === null,
                `[${mode}/${target}] idx 63 should be null`);
        }
    }
});

test('slotFor returns null for out-of-range indexes', () => {
    const scratch = parseSlotLabel('G1P1A');
    assert(slotFor(-1, scratch, 'ALTERNATE') === null, 'neg idx should be null');
    assert(slotFor(64, scratch, 'ALTERNATE') === null, 'idx 64 should be null (65 slots only when scratch absent - still handled)');
    assert(slotFor(999, scratch, 'ALTERNATE') === null, 'large idx should be null');
});

test('slotFor with null scratch still maps idx 0..63 to 64-slot list', () => {
    // Rare - used before the server has replied with scratch. We should
    // degrade gracefully rather than crash.
    const s0 = slotFor(0, null, 'ALTERNATE');
    assert(s0 && s0.label === 'G1P1A', 'idx 0 should map when scratch null');
    const s63 = slotFor(63, null, 'ALTERNATE');
    assert(s63 && s63.label === 'G4P8B', 'idx 63 should map when scratch null (no overflow)');
});

// --- parseSlotLabel --------------------------------------------------------

test('parseSlotLabel round-trips every canonical slot', () => {
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        for (const s of orderedSlots(mode)) {
            const parsed = parseSlotLabel(s.label);
            assert(parsed !== null, `parseSlotLabel(${s.label}) returned null`);
            assert(parsed.group === s.group, `group mismatch for ${s.label}`);
            assert(parsed.pattern === s.pattern, `pattern mismatch for ${s.label}`);
            assert(parsed.side === s.side, `side mismatch for ${s.label}`);
        }
    }
});

test('parseSlotLabel rejects malformed labels', () => {
    assert(parseSlotLabel('') === null, 'empty string');
    assert(parseSlotLabel('G1-P1A') === null, 'dashed form (progression push uses it but we keep this parser strict)');
    assert(parseSlotLabel('G5P1A') === null, 'group out of range');
    assert(parseSlotLabel('G1P9A') === null, 'pattern out of range');
    assert(parseSlotLabel('G1P1C') === null, 'side out of range');
    assert(parseSlotLabel(null) === null, 'null');
    assert(parseSlotLabel(42) === null, 'number');
});

// --- rotateToStart / startSlot anchor -------------------------------------

test('rotateToStart with null/missing startSlot returns list unchanged', () => {
    const list = orderedSlots('ALTERNATE');
    assert(rotateToStart(list, null) === list, 'null startSlot should return same list ref');
    const missing = { group: 9, pattern: 9, side: 'A' };
    const same = rotateToStart(list, missing);
    assert(same[0].label === 'G1P1A' && same[63].label === 'G4P8B',
        'missing startSlot should leave ordering intact');
});

test('rotateToStart ALTERNATE: startSlot=G1P4A pivots to index 0', () => {
    const list = orderedSlots('ALTERNATE');
    const rotated = rotateToStart(list, parseSlotLabel('G1P4A'));
    assert(rotated[0].label === 'G1P4A', `rot[0] expected G1P4A, got ${rotated[0].label}`);
    assert(rotated[1].label === 'G1P4B', `rot[1] expected G1P4B, got ${rotated[1].label}`);
    assert(rotated[rotated.length - 1].label === 'G1P3B',
        `rot[-1] expected G1P3B, got ${rotated[rotated.length - 1].label}`);
});

test('slotFor ALTERNATE + scratch=G1P1A + startSlot=G1P4A: idx 0→G1P4A, idx 1→G1P4B', () => {
    const scratch = parseSlotLabel('G1P1A');
    const startSlot = parseSlotLabel('G1P4A');
    assert(slotFor(0, scratch, 'ALTERNATE', startSlot).label === 'G1P4A',
        `idx 0 expected G1P4A, got ${slotFor(0, scratch, 'ALTERNATE', startSlot).label}`);
    assert(slotFor(1, scratch, 'ALTERNATE', startSlot).label === 'G1P4B',
        `idx 1 expected G1P4B, got ${slotFor(1, scratch, 'ALTERNATE', startSlot).label}`);
});

test('slotFor SERIAL + scratch=G1P1A + startSlot=G1P4A: idx 0→G1P4A, idx 1→G1P5A', () => {
    const scratch = parseSlotLabel('G1P1A');
    const startSlot = parseSlotLabel('G1P4A');
    assert(slotFor(0, scratch, 'SERIAL', startSlot).label === 'G1P4A',
        `idx 0 expected G1P4A, got ${slotFor(0, scratch, 'SERIAL', startSlot).label}`);
    assert(slotFor(1, scratch, 'SERIAL', startSlot).label === 'G1P5A',
        `idx 1 expected G1P5A, got ${slotFor(1, scratch, 'SERIAL', startSlot).label}`);
});

test('slotFor wraps past end back to start: SERIAL+G1P1A scratch, startSlot=G4P8A, idx 0→G4P8A, idx 1→G1P1B', () => {
    // SERIAL order: all As, then all Bs. Rotating to G4P8A (last A) means
    // idx 0 = G4P8A; idx 1 walks forward into the B block; scratch G1P1A is
    // filtered but it doesn't affect this segment (it sat before G4P8A).
    const scratch = parseSlotLabel('G1P1A');
    const startSlot = parseSlotLabel('G4P8A');
    assert(slotFor(0, scratch, 'SERIAL', startSlot).label === 'G4P8A',
        `idx 0 expected G4P8A, got ${slotFor(0, scratch, 'SERIAL', startSlot).label}`);
    assert(slotFor(1, scratch, 'SERIAL', startSlot).label === 'G1P1B',
        `idx 1 expected G1P1B, got ${slotFor(1, scratch, 'SERIAL', startSlot).label}`);
});

test('slotFor scratch==startSlot still filters scratch and advances to next slot', () => {
    // If the user selects G1P4A as anchor and scratch is also G1P4A, P1
    // can't land on the scratch - the filter drops it, so P1 lands on the
    // next slot in mode order (G1P4B under ALTERNATE, G1P5A under SERIAL).
    const clash = parseSlotLabel('G1P4A');
    assert(slotFor(0, clash, 'ALTERNATE', clash).label === 'G1P4B',
        `ALT: idx 0 expected G1P4B, got ${slotFor(0, clash, 'ALTERNATE', clash).label}`);
    assert(slotFor(0, clash, 'SERIAL', clash).label === 'G1P5A',
        `SER: idx 0 expected G1P5A, got ${slotFor(0, clash, 'SERIAL', clash).label}`);
});

// --- Full-bank target coverage --------------------------------------------

test('N=63 push (PUSH TO TD-3 at cap): every non-scratch slot appears exactly once', () => {
    for (const mode of ['ALTERNATE', 'SERIAL']) {
        for (const target of ['G1P2A', 'G3P5B', 'G4P8B']) {
            const scratch = parseSlotLabel(target);
            const seen = new Set();
            for (let i = 0; i < 63; i++) {
                const slot = slotFor(i, scratch, mode);
                assert(slot !== null, `[${mode}/${target}] idx ${i} should not be null`);
                assert(!seen.has(slot.label),
                    `[${mode}/${target}] duplicate target ${slot.label} at idx ${i}`);
                seen.add(slot.label);
                assert(slot.label !== target,
                    `[${mode}/${target}] idx ${i} equals scratch ${target}`);
            }
            assert(seen.size === 63, `[${mode}/${target}] expected 63 unique, got ${seen.size}`);
        }
    }
});

// --- Summary ---------------------------------------------------------------

if (failed > 0) {
    console.error(`\nslot-targets: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}
console.log(`\nslot-targets: ${passed} passed`);
