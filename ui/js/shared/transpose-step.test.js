// Usage: node ui/js/shared/transpose-step.test.js
//
// Exercises the pure semitone-shift helper. The algorithm now walks across the DOWN /
// NORMAL / UP zones at slot boundaries (B <-> C, and the C^ alias), and
// wraps within the current zone only at the hardware edges
// (DN floor and UP ceiling). Regressions on the wrap/cross rules would
// silently corrupt user patterns.

import {
    transposeStepNote,
    transposeStepsInPlace,
    transposeBasslineSetInPlace,
    BASSLINE_ARCHETYPE_KEYS,
} from './transpose-step.js';

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

function step(overrides = {}) {
    return {
        note: 'C',
        transpose: 'NORMAL',
        accent: false,
        slide: false,
        time: 'NORMAL',
        ...overrides,
    };
}

console.log('transpose-step tests:');

// --- Interior shifts inside a single zone ----------------------------------

test('+1 on C in NORMAL gives C# in NORMAL', () => {
    const out = transposeStepNote(step({ note: 'C' }), +1);
    assert(out.note === 'C#', `expected C#, got ${out.note}`);
    assert(out.transpose === 'NORMAL', 'zone preserved');
});

test('+1 on C# in UP gives D in UP', () => {
    const out = transposeStepNote(step({ note: 'C#', transpose: 'UP' }), +1);
    assert(out.note === 'D', `expected D, got ${out.note}`);
    assert(out.transpose === 'UP', 'UP zone preserved on interior shift');
});

test('-1 on C# in DOWN gives C in DOWN', () => {
    const out = transposeStepNote(step({ note: 'C#', transpose: 'DOWN' }), -1);
    assert(out.note === 'C', `expected C, got ${out.note}`);
    assert(out.transpose === 'DOWN', 'DOWN zone preserved');
});

test('-1 on C^ in UP gives B in UP', () => {
    const out = transposeStepNote(step({ note: 'C^', transpose: 'UP' }), -1);
    assert(out.note === 'B', `expected B, got ${out.note}`);
    assert(out.transpose === 'UP', 'UP zone preserved');
});

// --- Cross-zone at the B <-> C boundary ------------------------------------

test('-1 on (C, NORMAL) crosses to (B, DOWN)', () => {
    const out = transposeStepNote(step({ note: 'C', transpose: 'NORMAL' }), -1);
    assert(out.note === 'B', `expected B, got ${out.note}`);
    assert(out.transpose === 'DOWN', `expected DOWN, got ${out.transpose}`);
});

test('-1 on (C, UP) crosses to (B, NORMAL)', () => {
    const out = transposeStepNote(step({ note: 'C', transpose: 'UP' }), -1);
    assert(out.note === 'B', `expected B, got ${out.note}`);
    assert(out.transpose === 'NORMAL', `expected NORMAL, got ${out.transpose}`);
});

test('+1 on (B, DOWN) crosses to (C, NORMAL)', () => {
    const out = transposeStepNote(step({ note: 'B', transpose: 'DOWN' }), +1);
    assert(out.note === 'C', `expected C, got ${out.note}`);
    assert(out.transpose === 'NORMAL', `expected NORMAL, got ${out.transpose}`);
});

test('+1 on (B, NORMAL) crosses to (C, UP)', () => {
    const out = transposeStepNote(step({ note: 'B', transpose: 'NORMAL' }), +1);
    assert(out.note === 'C', `expected C, got ${out.note}`);
    assert(out.transpose === 'UP', `expected UP, got ${out.transpose}`);
});

// --- Cross-zone via the C^ alias (slot 12) ---------------------------------

test('+1 on (C^, NORMAL) crosses to (C#, UP)', () => {
    const out = transposeStepNote(step({ note: 'C^', transpose: 'NORMAL' }), +1);
    assert(out.note === 'C#', `expected C#, got ${out.note}`);
    assert(out.transpose === 'UP', `expected UP, got ${out.transpose}`);
});

test('+1 on (C^, DOWN) crosses to (C#, NORMAL)', () => {
    const out = transposeStepNote(step({ note: 'C^', transpose: 'DOWN' }), +1);
    assert(out.note === 'C#', `expected C#, got ${out.note}`);
    assert(out.transpose === 'NORMAL', `expected NORMAL, got ${out.transpose}`);
});

test('-1 on (C^, NORMAL) stays in NORMAL at B', () => {
    // Moving down from C^ is an interior move: C^ -> B within the same zone.
    const out = transposeStepNote(step({ note: 'C^', transpose: 'NORMAL' }), -1);
    assert(out.note === 'B', `expected B, got ${out.note}`);
    assert(out.transpose === 'NORMAL', `expected NORMAL, got ${out.transpose}`);
});

// --- Hardware-edge wraps (no zone beyond) ----------------------------------

test('(C, DOWN) -1 wraps within DOWN to (B, DOWN) - floor edge', () => {
    const out = transposeStepNote(step({ note: 'C', transpose: 'DOWN' }), -1);
    assert(out.note === 'B', `expected B, got ${out.note}`);
    assert(out.transpose === 'DOWN', `expected DOWN (wrap in DOWN), got ${out.transpose}`);
});

test('(C^, UP) +1 wraps within UP to (C#, UP) - ceiling edge', () => {
    const out = transposeStepNote(step({ note: 'C^', transpose: 'UP' }), +1);
    assert(out.note === 'C#', `expected C#, got ${out.note}`);
    assert(out.transpose === 'UP', `expected UP (wrap in UP), got ${out.transpose}`);
});

test('(B, UP) +1 reaches (C^, UP) - ceiling top', () => {
    const out = transposeStepNote(step({ note: 'B', transpose: 'UP' }), +1);
    assert(out.note === 'C^', `expected C^, got ${out.note}`);
    assert(out.transpose === 'UP', `expected UP (top of hardware range), got ${out.transpose}`);
});

// --- Full matrix -----

const EXAMPLES = [
    // [beforeNote, beforeZone, delta, afterNote, afterZone, description]
    ['C',  'NORMAL', -1, 'B',  'DOWN',   'C NORMAL -1 -> B DOWN'],
    ['C^', 'NORMAL', +1, 'C#', 'UP',     'C^ NORMAL +1 -> C# UP'],
    ['C#', 'UP',     +1, 'D',  'UP',     'C# UP +1 -> D UP'],
    ['C',  'DOWN',   -1, 'B',  'DOWN',   'C DOWN -1 wraps to B DOWN (floor)'],
    ['C^', 'UP',     +1, 'C#', 'UP',     'C^ UP +1 wraps to C# UP (ceiling)'],
    ['C^', 'UP',     -1, 'B',  'UP',     'C^ UP -1 -> B UP'],
    ['C^', 'NORMAL', -1, 'B',  'NORMAL', 'C^ NORMAL -1 -> B NORMAL'],
    ['C',  'UP',     -1, 'B',  'NORMAL', 'C UP -1 -> B NORMAL'],
    ['B',  'DOWN',   +1, 'C',  'NORMAL', 'B DOWN +1 -> C NORMAL'],
];

for (const [note, zone, delta, wantNote, wantZone, desc] of EXAMPLES) {
    test(`example matrix: ${desc}`, () => {
        const out = transposeStepNote(step({ note, transpose: zone }), delta);
        assert(out.note === wantNote,  `note: expected ${wantNote}, got ${out.note}`);
        assert(out.transpose === wantZone, `zone: expected ${wantZone}, got ${out.transpose}`);
    });
}

// --- Canonical zone label is `'DOWN'` (matches step-card-view.js DN button) --

test('floor-wrap emits canonical DOWN so the DN button lights up', () => {
    // step-card-view.js checks `step.transpose === 'DOWN'` for the DN button
    // active style. If the transpose helper ever emitted `'DN'` (the label)
    // the button would stay inactive despite the step being down-shifted.
    const out = transposeStepNote(step({ note: 'C', transpose: 'NORMAL' }), -1);
    assert(out.transpose === 'DOWN', `expected 'DOWN' (canonical), got ${out.transpose}`);
});

test("legacy 'DN' input is normalised to 'DOWN' on output", () => {
    // The helper tolerates that on input but emits the canonical 'DOWN' so
    // the step-card UP/DN buttons render in the correct active state.
    const out = transposeStepNote(step({ note: 'E', transpose: 'DN' }), +1);
    assert(out.transpose === 'DOWN', `expected 'DOWN' (normalised), got ${out.transpose}`);
    assert(out.note === 'F', 'note still shifts');
});

// --- Field preservation (accent, slide, time) ------------------------------

test('accent / slide / time flags survive shift', () => {
    const before = step({
        note: 'E',
        transpose: 'UP',
        accent: true,
        slide: true,
        time: 'SLIDE',
    });
    const after = transposeStepNote(before, +1);
    assert(after.note === 'F', `expected F, got ${after.note}`);
    assert(after.transpose === 'UP', 'UP preserved on interior shift');
    assert(after.accent === true, 'accent preserved');
    assert(after.slide === true, 'slide preserved');
    assert(after.time === 'SLIDE', 'time preserved');
});

test('REST and TIE_REST steps also transpose', () => {
    const rest = transposeStepNote(step({ note: 'E', time: 'REST' }), +1);
    assert(rest.note === 'F', 'REST step note shifts');
    assert(rest.time === 'REST', 'REST time flag preserved');

    const tieRest = transposeStepNote(step({ note: 'E', time: 'TIE_REST' }), -1);
    assert(tieRest.note === 'D#', 'TIE_REST step note shifts');
    assert(tieRest.time === 'TIE_REST', 'TIE_REST time flag preserved');
});

// --- Immutability ----------------------------------------------------------

test('input step is not mutated', () => {
    const before = step({ note: 'C', transpose: 'NORMAL' });
    transposeStepNote(before, -1);
    assert(before.note === 'C', 'original step.note unchanged');
    assert(before.transpose === 'NORMAL', 'original step.transpose unchanged');
});

// --- Round-trip (interior shifts are fully reversible) ---------------------

test('interior shift is reversible: +1 then -1 returns the same note', () => {
    const before = step({ note: 'E' });
    const after = transposeStepNote(transposeStepNote(before, +1), -1);
    assert(after.note === 'E', `expected E, got ${after.note}`);
    assert(after.transpose === 'NORMAL', 'zone preserved round-trip');
});

test('cross-zone shift is reversible: (B, DOWN) +1 -1 returns to (B, DOWN)', () => {
    const before = step({ note: 'B', transpose: 'DOWN' });
    const after = transposeStepNote(transposeStepNote(before, +1), -1);
    // +1: (B, DOWN) -> (C, NORMAL)
    // -1: (C, NORMAL) -> (B, DOWN)
    assert(after.note === 'B', `expected B, got ${after.note}`);
    assert(after.transpose === 'DOWN', `expected DOWN, got ${after.transpose}`);
});

test('floor wrap is lossy by design: (C, DOWN) -1 +1 lands on (C, NORMAL), not (C, DOWN)', () => {
    // Trace:
    //   -1: (C, DOWN) wraps within DOWN  -> (B, DOWN)   [floor edge]
    //   +1: (B, DOWN) crosses to NORMAL  -> (C, NORMAL) [B↔C boundary]
    // The zone slipped from DOWN to NORMAL. Round-trip is intentionally not
    // an identity here so every click always moves *something* at the edge.
    const before = step({ note: 'C', transpose: 'DOWN' });
    const after = transposeStepNote(transposeStepNote(before, -1), +1);
    assert(after.note === 'C', `expected C (documented wrap loss), got ${after.note}`);
    assert(after.transpose === 'NORMAL', `expected NORMAL (zone drift from wrap), got ${after.transpose}`);
});

test('ceiling wrap is lossy by design: (C^, UP) +1 -1 lands on (C, UP), not (C^, UP)', () => {
    const before = step({ note: 'C^', transpose: 'UP' });
    const after = transposeStepNote(transposeStepNote(before, +1), -1);
    // +1: (C^, UP) -> (C#, UP) (ceiling wrap)
    // -1: (C#, UP) -> (C, UP)
    assert(after.note === 'C', `expected C (documented wrap loss), got ${after.note}`);
    assert(after.transpose === 'UP', 'stays in UP zone');
});

// --- Input validation ------------------------------------------------------

test('rejects delta values other than ±1', () => {
    let threw = false;
    try { transposeStepNote(step(), 2); } catch (_e) { threw = true; }
    assert(threw, 'delta=2 should throw');

    threw = false;
    try { transposeStepNote(step(), 0); } catch (_e) { threw = true; }
    assert(threw, 'delta=0 should throw');
});

test('rejects unknown note names', () => {
    let threw = false;
    try { transposeStepNote({ note: 'H', transpose: 'NORMAL' }, +1); } catch (_e) { threw = true; }
    assert(threw, 'unknown note name should throw');
});

test('rejects unknown transpose zone values', () => {
    let threw = false;
    try { transposeStepNote({ note: 'C', transpose: 'BONKERS' }, +1); } catch (_e) { threw = true; }
    assert(threw, 'unknown transpose flag should throw');
});

// --- Array helper ----------------------------------------------------------

test('transposeStepsInPlace shifts every entry in the array in place', () => {
    const steps = [
        step({ note: 'C',  transpose: 'NORMAL' }),
        step({ note: 'E',  transpose: 'NORMAL' }),
        step({ note: 'C^', transpose: 'NORMAL' }),
    ];
    transposeStepsInPlace(steps, +1);
    assert(steps[0].note === 'C#', `entry 0 expected C#, got ${steps[0].note}`);
    assert(steps[0].transpose === 'NORMAL', 'entry 0 zone');
    assert(steps[1].note === 'F', `entry 1 expected F, got ${steps[1].note}`);
    assert(steps[2].note === 'C#', `entry 2 (cross-zone) expected C#, got ${steps[2].note}`);
    assert(steps[2].transpose === 'UP', `entry 2 expected UP zone after cross, got ${steps[2].transpose}`);
});

test('transposeStepsInPlace is a no-op on null / non-array inputs', () => {
    let threw = false;
    try {
        transposeStepsInPlace(null, +1);
        transposeStepsInPlace(undefined, -1);
        transposeStepsInPlace('not an array', +1);
    } catch (_e) {
        threw = true;
    }
    assert(!threw, 'non-array input should be tolerated, not throw');
});

// --- Bassline set helper ---------------------------------------------------

function makeArchetypePattern(notes, zone = 'NORMAL') {
    return {
        active_steps: notes.length,
        triplet: false,
        steps: notes.map(n => step({ note: n, transpose: zone })),
    };
}

test('archetype key list matches progression-row chips', () => {
    assert(BASSLINE_ARCHETYPE_KEYS.length === 5, `expected 5 keys, got ${BASSLINE_ARCHETYPE_KEYS.length}`);
    assert(BASSLINE_ARCHETYPE_KEYS.includes('pedal'), 'pedal missing');
    assert(BASSLINE_ARCHETYPE_KEYS.includes('rootPulse'), 'rootPulse missing');
    assert(BASSLINE_ARCHETYPE_KEYS.includes('offbeat'), 'offbeat missing');
    assert(BASSLINE_ARCHETYPE_KEYS.includes('shadow'), 'shadow missing');
    assert(BASSLINE_ARCHETYPE_KEYS.includes('arpeggio'), 'arpeggio missing');
});

test('transposeBasslineSetInPlace shifts every archetype pattern', () => {
    const set = {
        pedal:     makeArchetypePattern(['C',  'C',  'C',  'C']),
        rootPulse: makeArchetypePattern(['C',  'E',  'G',  'C']),
        offbeat:   makeArchetypePattern(['D',  'F',  'A',  'C']),
        shadow:    makeArchetypePattern(['E',  'G',  'B',  'C']),
        arpeggio:  makeArchetypePattern(['C^', 'B',  'A',  'G']),
    };
    transposeBasslineSetInPlace(set, +1);
    assert(set.pedal.steps[0].note     === 'C#', 'pedal[0] -> C#');
    assert(set.rootPulse.steps[1].note === 'F',  'rootPulse[1] -> F');
    assert(set.offbeat.steps[2].note   === 'A#', 'offbeat[2] -> A#');
    // shadow[2] was B in NORMAL; B NORMAL +1 crosses to (C, UP).
    assert(set.shadow.steps[2].note    === 'C', 'shadow[2] -> C (cross-zone)');
    assert(set.shadow.steps[2].transpose === 'UP', 'shadow[2] lands in UP zone');
    // arpeggio[0] was C^ in NORMAL; C^ NORMAL +1 crosses to (C#, UP).
    assert(set.arpeggio.steps[0].note       === 'C#', 'arpeggio[0] -> C# (cross-zone)');
    assert(set.arpeggio.steps[0].transpose  === 'UP', 'arpeggio[0] lands in UP');
});

test('transposeBasslineSetInPlace is a no-op on null', () => {
    let threw = false;
    try { transposeBasslineSetInPlace(null, +1); } catch (_e) { threw = true; }
    assert(!threw, 'null set should be silently skipped');
});

test('transposeBasslineSetInPlace tolerates missing archetype keys', () => {
    const set = { rootPulse: makeArchetypePattern(['C', 'E']) };
    let threw = false;
    try { transposeBasslineSetInPlace(set, +1); } catch (_e) { threw = true; }
    assert(!threw, 'missing keys should not throw');
    assert(set.rootPulse.steps[0].note === 'C#', 'present archetype still transposes');
    assert(set.rootPulse.steps[1].note === 'F',  'present archetype still transposes');
});

test('transposeBasslineSetInPlace preserves archetype shape (active_steps, triplet)', () => {
    const set = {
        pedal: {
            active_steps: 12,
            triplet: true,
            steps: [step({ note: 'A' }), step({ note: 'B' })],
        },
    };
    transposeBasslineSetInPlace(set, -1);
    assert(set.pedal.active_steps === 12, 'active_steps preserved');
    assert(set.pedal.triplet === true,   'triplet flag preserved');
    assert(set.pedal.steps[0].note === 'G#', 'step 0 shifts');
    assert(set.pedal.steps[1].note === 'A#', 'step 1 shifts');
});

// --- Summary ---------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
