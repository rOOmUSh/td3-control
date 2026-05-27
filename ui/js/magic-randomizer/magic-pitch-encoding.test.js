// Usage: node ui/js/magic-randomizer/magic-pitch-encoding.test.js

import {
    TD3_PITCH_MIN, TD3_PITCH_MAX,
    isPitchInRange, encodePitch, decodePitch,
    buildScalePitches, nearestPitch,
} from './magic-pitch-encoding.js';

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

// --- Range / encode / decode round-trip ---

test('range constants', () => {
    assert(TD3_PITCH_MIN === -12, 'min is -12');
    assert(TD3_PITCH_MAX === 24, 'max is 24');
});

test('isPitchInRange', () => {
    assert(isPitchInRange(0), '0 in range');
    assert(isPitchInRange(-12), '-12 in range');
    assert(isPitchInRange(24), '24 in range');
    assert(!isPitchInRange(-13), '-13 out');
    assert(!isPitchInRange(25), '25 out');
    assert(!isPitchInRange(0.5), 'non-int rejected');
    assert(!isPitchInRange(NaN), 'NaN rejected');
});

test('encodePitch in NORMAL row', () => {
    const e = encodePitch(0);
    assert(e.note === 'C' && e.transpose === 'NORMAL' && e.noteIdx === 0, '0 → (C, NORMAL)');
    const e2 = encodePitch(7);
    assert(e2.note === 'G' && e2.transpose === 'NORMAL', '7 → (G, NORMAL)');
    const e3 = encodePitch(12);
    assert(e3.note === 'C^' && e3.transpose === 'NORMAL', '12 → (C^, NORMAL) preferred');
});

test('encodePitch in DOWN row', () => {
    const e = encodePitch(-1);
    assert(e.note === 'B' && e.transpose === 'DOWN', '-1 → (B, DOWN)');
    const e2 = encodePitch(-12);
    assert(e2.note === 'C' && e2.transpose === 'DOWN', '-12 → (C, DOWN)');
});

test('encodePitch in UP row', () => {
    const e = encodePitch(13);
    assert(e.note === 'C#' && e.transpose === 'UP', '13 → (C#, UP)');
    const e2 = encodePitch(24);
    assert(e2.note === 'C^' && e2.transpose === 'UP', '24 → (C^, UP)');
});

test('encodePitch out-of-range returns null', () => {
    assert(encodePitch(-13) === null, '-13 null');
    assert(encodePitch(25) === null, '25 null');
});

test('decodePitch round-trip', () => {
    for (let p = TD3_PITCH_MIN; p <= TD3_PITCH_MAX; p++) {
        const e = encodePitch(p);
        const back = decodePitch(e);
        assert(back === p, `round-trip ${p} got ${back}`);
    }
});

test('decodePitch handles step object form', () => {
    const back = decodePitch({ note: 'G', transpose: 'NORMAL' });
    assert(back === 7, 'step object → 7');
});

test('decodePitch handles invalid input', () => {
    assert(decodePitch('???', 'NORMAL') === null, 'bad note name');
    assert(decodePitch('C', 'WHATEVER') === null, 'bad transpose');
});

// --- Scale pitch building ---

test('buildScalePitches: C major spans three octaves', () => {
    const scale = { intervals: [0, 2, 4, 5, 7, 9, 11] };
    const pitches = buildScalePitches(0, scale);
    // Major scale has 7 pitch classes; over [-12, 24] (37 semitones) we
    // expect 7*3 + 1 (the closing C^ at 24) = 22 entries.
    assert(pitches.length === 22, `expected 22, got ${pitches.length}: ${pitches.join(',')}`);
    assert(pitches[0] === -12, 'starts at -12');
    assert(pitches[pitches.length - 1] === 24, 'ends at 24');
    assert(pitches.includes(0) && pitches.includes(12) && pitches.includes(24), 'roots present');
});

test('buildScalePitches: C minor pentatonic (5 notes)', () => {
    const scale = { intervals: [0, 3, 5, 7, 10] };
    const pitches = buildScalePitches(0, scale);
    // 5 pcs * 3 octaves + closing C^ = 16 entries
    assert(pitches.length === 16, `expected 16, got ${pitches.length}`);
});

test('buildScalePitches: rooted at non-zero', () => {
    // A minor uses pcs {9, 11, 0, 2, 4, 5, 7}. From root A (=9), every pitch
    // p with pc in that set must appear, e.g. 9, 11, 12, 14, 16, 17, 19...
    const scale = { intervals: [0, 2, 3, 5, 7, 8, 10] };
    const pitches = buildScalePitches(9, scale);
    assert(pitches.includes(9), 'A=9 present');
    assert(pitches.includes(0), 'C=0 present (in A minor)');
    assert(pitches.includes(-3), 'A below root present at -3');
    assert(pitches.every(p => p >= TD3_PITCH_MIN && p <= TD3_PITCH_MAX), 'all in range');
});

test('buildScalePitches: empty/invalid input safe', () => {
    assert(buildScalePitches(0, null).length === 0, 'null scale → empty');
    assert(buildScalePitches(0, {}).length === 0, 'no intervals → empty');
});

// --- Nearest pitch ---

test('nearestPitch picks closest', () => {
    const pitches = [-5, 0, 3, 7, 12];
    assert(nearestPitch(4, pitches) === 3, '4 → 3 (closer than 7)');
    assert(nearestPitch(11, pitches) === 12, '11 → 12');
    assert(nearestPitch(-100, pitches) === -5, '-100 → -5 (closest)');
});

test('nearestPitch on empty list returns null', () => {
    assert(nearestPitch(0, []) === null, 'empty → null');
    assert(nearestPitch(0, null) === null, 'null → null');
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
