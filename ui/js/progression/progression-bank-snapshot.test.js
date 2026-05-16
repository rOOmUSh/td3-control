// Usage: node ui/js/progression/progression-bank-snapshot.test.js
//
// Locks down the pure helpers that build the bank snapshot payload from
// progression state:
//   - formatSnapshotName: deterministic SN_YYYY-MM-DD_HH-MM-SS timestamp
//   - basslineSlotForIndex: linear 1..20 → "G{1..3}-P{1..8}B" mapping
//   - buildSinglePatternSlots: 1 acid + 5 archetype basslines for a row
//   - buildProgressionSlots: 4 acid + 20 archetype basslines for the full
//     progression (position-major × archetype-minor)
//
// Naming contract (drives bank-side filtering):
//   - acid display_name = "G1P{n}A" (no separator)
//   - bassline display_name = "G1P{n}A {ARCHETYPE_LABEL} BSL"
//
// The DOM flow (BANK button click → bankApi.createSnapshotFromPatterns)
// stays covered by manual browser verification.

import {
    ARCHETYPE_KEYS_ORDERED,
    ARCHETYPE_LABELS,
    formatSnapshotName,
    basslineSlotForIndex,
    buildSinglePatternSlots,
    buildProgressionSlots,
} from './progression-bank-snapshot.js';

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

function makePattern(tag) {
    return { tag, active_steps: 16, triplet: false, steps: new Array(16).fill(0).map(() => ({})) };
}

function makeBasslineSet(tag) {
    const set = {};
    for (const key of ARCHETYPE_KEYS_ORDERED) {
        set[key] = makePattern(`${tag}:${key}`);
    }
    return set;
}

console.log('progression-bank-snapshot tests:');

// --- formatSnapshotName -----------------------------------------------------

test('formatSnapshotName: zero-pads and joins fields', () => {
    const d = new Date(2026, 3, 26, 16, 14, 9); // months are 0-indexed
    assert(formatSnapshotName(d) === 'SN_2026-04-26_16-14-09', `got ${formatSnapshotName(d)}`);
});
test('formatSnapshotName: handles single-digit month/day/h/m/s', () => {
    const d = new Date(2026, 0, 1, 1, 2, 3);
    assert(formatSnapshotName(d) === 'SN_2026-01-01_01-02-03', `got ${formatSnapshotName(d)}`);
});
test('formatSnapshotName: defaults to current Date when called without args', () => {
    const out = formatSnapshotName();
    assert(typeof out === 'string' && /^SN_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}$/.test(out),
        `got ${out}, expected /^SN_\\d{4}-\\d{2}-\\d{2}_\\d{2}-\\d{2}-\\d{2}$/`);
});

// --- basslineSlotForIndex ---------------------------------------------------

test('basslineSlotForIndex: 1 → G1-P1B (lower bound)', () => {
    assert(basslineSlotForIndex(1) === 'G1-P1B', `got ${basslineSlotForIndex(1)}`);
});
test('basslineSlotForIndex: 8 → G1-P8B (group boundary)', () => {
    assert(basslineSlotForIndex(8) === 'G1-P8B', `got ${basslineSlotForIndex(8)}`);
});
test('basslineSlotForIndex: 9 → G2-P1B (group rollover)', () => {
    assert(basslineSlotForIndex(9) === 'G2-P1B', `got ${basslineSlotForIndex(9)}`);
});
test('basslineSlotForIndex: 20 → G3-P4B (upper bound)', () => {
    assert(basslineSlotForIndex(20) === 'G3-P4B', `got ${basslineSlotForIndex(20)}`);
});
test('basslineSlotForIndex: 0 and 21 reject', () => {
    assert(basslineSlotForIndex(0) === null, 'idx=0 should be null');
    assert(basslineSlotForIndex(21) === null, 'idx=21 should be null');
});
test('basslineSlotForIndex: non-integer rejects', () => {
    assert(basslineSlotForIndex('5') === null, "string '5' should be null");
    assert(basslineSlotForIndex(1.5) === null, '1.5 should be null');
});

// --- buildSinglePatternSlots ------------------------------------------------

test('buildSinglePatternSlots: returns 6 slots in fixed order', () => {
    const pat = makePattern('lead');
    const set = makeBasslineSet('p0');
    const { slots, error } = buildSinglePatternSlots(0, pat, set);
    assert(error === null, `error=${error}`);
    assert(slots.length === 6, `len=${slots.length}`);
    assert(slots[0].slot_key === 'G1-P1A', `acid slot=${slots[0].slot_key}`);
    assert(slots[1].slot_key === 'G1-P1B', `s1=${slots[1].slot_key}`);
    assert(slots[5].slot_key === 'G1-P5B', `s5=${slots[5].slot_key}`);
});
test('buildSinglePatternSlots: acid display_name is "G1P1A" (no dash)', () => {
    const { slots } = buildSinglePatternSlots(0, makePattern('a'), makeBasslineSet('b'));
    assert(slots[0].display_name === 'G1P1A',
        `got ${slots[0].display_name}, expected G1P1A`);
});
test('buildSinglePatternSlots: bassline names follow "G1P1A {ARCH} BSL"', () => {
    const { slots } = buildSinglePatternSlots(0, makePattern('a'), makeBasslineSet('b'));
    const expected = [
        'G1P1A PEDAL BSL',
        'G1P1A PULSE BSL',
        'G1P1A OFFBEAT BSL',
        'G1P1A SHADOW BSL',
        'G1P1A ARP BSL',
    ];
    for (let i = 0; i < 5; i++) {
        assert(slots[i + 1].display_name === expected[i],
            `slot ${i + 1}: got ${slots[i + 1].display_name}, expected ${expected[i]}`);
    }
});
test('buildSinglePatternSlots: archetype patterns route to PEDAL→P1B, PULSE→P2B...', () => {
    const set = makeBasslineSet('p0');
    const { slots } = buildSinglePatternSlots(0, makePattern('a'), set);
    assert(slots[1].pattern === set.pedal, 'P1B should hold pedal');
    assert(slots[2].pattern === set.rootPulse, 'P2B should hold rootPulse');
    assert(slots[3].pattern === set.offbeat, 'P3B should hold offbeat');
    assert(slots[4].pattern === set.shadow, 'P4B should hold shadow');
    assert(slots[5].pattern === set.arpeggio, 'P5B should hold arpeggio');
});
test('buildSinglePatternSlots: idx > 0 still emits G1P1A (single-row push is index-agnostic)', () => {
    const { slots } = buildSinglePatternSlots(2, makePattern('p3'), makeBasslineSet('b3'));
    assert(slots[0].slot_key === 'G1-P1A', 'always G1-P1A regardless of idx');
    assert(slots[1].display_name === 'G1P1A PEDAL BSL', 'always uses G1P1A acid label');
});
test('buildSinglePatternSlots: rejects missing pattern', () => {
    const r = buildSinglePatternSlots(0, null, makeBasslineSet('b'));
    assert(r.slots === null && r.error === 'missing-pattern', `got ${JSON.stringify(r)}`);
});
test('buildSinglePatternSlots: rejects missing basslineSet', () => {
    const r = buildSinglePatternSlots(0, makePattern('a'), null);
    assert(r.slots === null && r.error === 'missing-basslines', `got ${JSON.stringify(r)}`);
});
test('buildSinglePatternSlots: rejects missing archetype', () => {
    const set = makeBasslineSet('b');
    delete set.shadow;
    const r = buildSinglePatternSlots(0, makePattern('a'), set);
    assert(r.slots === null && r.error === 'missing-archetype:shadow', `got ${JSON.stringify(r)}`);
});

// --- buildProgressionSlots --------------------------------------------------

test('buildProgressionSlots: returns 4 acid + 20 bassline slots = 24 total', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    const { slots, error } = buildProgressionSlots(patterns, blByPattern);
    assert(error === null, `error=${error}`);
    assert(slots.length === 24, `len=${slots.length}`);
});
test('buildProgressionSlots: acid slots are G1-P1A..G1-P4A in order', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    const { slots } = buildProgressionSlots(patterns, blByPattern);
    for (let i = 0; i < 4; i++) {
        assert(slots[i].slot_key === `G1-P${i + 1}A`,
            `slot ${i}: got ${slots[i].slot_key}`);
        assert(slots[i].display_name === `G1P${i + 1}A`,
            `slot ${i} name: got ${slots[i].display_name}`);
        assert(slots[i].pattern === patterns[i], `slot ${i} pattern identity`);
    }
});
test('buildProgressionSlots: bassline slots fill G1-P1B..G3-P4B in linear order', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    const { slots } = buildProgressionSlots(patterns, blByPattern);
    const expectedKeys = [];
    for (let g = 1; g <= 3; g++) {
        const max = g === 3 ? 4 : 8;
        for (let p = 1; p <= max; p++) expectedKeys.push(`G${g}-P${p}B`);
    }
    for (let i = 0; i < 20; i++) {
        assert(slots[i + 4].slot_key === expectedKeys[i],
            `bassline slot ${i}: got ${slots[i + 4].slot_key}, expected ${expectedKeys[i]}`);
    }
});
test('buildProgressionSlots: bassline names are position-major × archetype-minor', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    const { slots } = buildProgressionSlots(patterns, blByPattern);
    // First 5 basslines all reference G1P1A
    for (let i = 0; i < 5; i++) {
        const expected = `G1P1A ${ARCHETYPE_LABELS[ARCHETYPE_KEYS_ORDERED[i]]} BSL`;
        assert(slots[4 + i].display_name === expected,
            `slot ${4 + i}: got ${slots[4 + i].display_name}, expected ${expected}`);
    }
    // pos1 starts at slot 4+5=9 (linear bassline 6, → G1-P6B)
    assert(slots[9].slot_key === 'G1-P6B', `pos1.pedal slot=${slots[9].slot_key}`);
    assert(slots[9].display_name === 'G1P2A PEDAL BSL',
        `pos1.pedal name=${slots[9].display_name}`);
    // pos3 final = slot index 23 (last entry), → G3-P4B, "G1P4A ARP BSL"
    assert(slots[23].slot_key === 'G3-P4B', `last slot key=${slots[23].slot_key}`);
    assert(slots[23].display_name === 'G1P4A ARP BSL',
        `last name=${slots[23].display_name}`);
});
test('buildProgressionSlots: archetype pattern identity preserved', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    const { slots } = buildProgressionSlots(patterns, blByPattern);
    // pos2 = patterns idx=2; first archetype = pedal → linear 11, slot index 4+10=14
    assert(slots[14].pattern === blByPattern[2].pedal,
        'pos2.pedal must be at linear bassline 11 (slot index 14)');
    assert(slots[14].slot_key === 'G2-P3B', `slot 14 key=${slots[14].slot_key}`);
});
test('buildProgressionSlots: rejects bad-shape inputs', () => {
    const ok = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    let r = buildProgressionSlots(null, ok);
    assert(r.error === 'bad-patterns-shape', `got ${r.error}`);
    r = buildProgressionSlots([makePattern('a')], ok);
    assert(r.error === 'bad-patterns-shape', `len 1 should fail`);
    r = buildProgressionSlots([0, 1, 2, 3].map(i => makePattern(`p${i}`)), null);
    assert(r.error === 'bad-basslines-shape', `got ${r.error}`);
});
test('buildProgressionSlots: rejects when one bassline set is missing', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    blByPattern[2] = null;
    const r = buildProgressionSlots(patterns, blByPattern);
    assert(r.error === 'missing-basslines:2', `got ${r.error}`);
});
test('buildProgressionSlots: rejects when one archetype is missing', () => {
    const patterns = [0, 1, 2, 3].map(i => makePattern(`p${i}`));
    const blByPattern = [0, 1, 2, 3].map(i => makeBasslineSet(`p${i}`));
    delete blByPattern[1].arpeggio;
    const r = buildProgressionSlots(patterns, blByPattern);
    assert(r.error === 'missing-archetype:1:arpeggio', `got ${r.error}`);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
