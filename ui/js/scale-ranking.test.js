// Tests for scale-ranking.js - runs with Node.js
// Usage: node ui/js/scale-ranking.test.js
//
// Inline copy of the production `rankScales` (same pattern as
// key-detection.test.js) so this test file stays runnable without a DOM.

const CHROMATIC_ID = 'chromatic';

function rankScales({ scales, hist, root }) {
    const total = hist.reduce((a, b) => a + b, 0);
    if (total === 0) {
        return scales.map(s => ({ scale: s, fit: 0, size: s.intervals.length }));
    }
    const rows = scales.map(s => {
        const pcSet = new Set(s.intervals.map(i => (((root + i) % 12) + 12) % 12));
        let inside = 0;
        for (const pc of pcSet) inside += hist[pc];
        return { scale: s, fit: inside / total, size: s.intervals.length };
    });
    rows.sort((a, b) => {
        const aChrom = a.scale.id === CHROMATIC_ID;
        const bChrom = b.scale.id === CHROMATIC_ID;
        if (aChrom && !bChrom) return 1;
        if (bChrom && !aChrom) return -1;
        if (b.fit !== a.fit) return b.fit - a.fit;
        return a.size - b.size;
    });
    return rows;
}

// --- Fixture scales (subset of the real config) -----------------------------

const SCALES = [
    { id: 'major',              name: 'Major',              intervals: [0,2,4,5,7,9,11],       tags: ['safe'] },
    { id: 'natural_minor',      name: 'Natural Minor',      intervals: [0,2,3,5,7,8,10],       tags: ['safe'] },
    { id: 'dorian',             name: 'Dorian',             intervals: [0,2,3,5,7,9,10],       tags: ['safe'] },
    { id: 'mixolydian',         name: 'Mixolydian',         intervals: [0,2,4,5,7,9,10],       tags: ['safe'] },
    { id: 'major_pentatonic',   name: 'Major Pentatonic',   intervals: [0,2,4,7,9],            tags: ['safe'] },
    { id: 'minor_pentatonic',   name: 'Minor Pentatonic',   intervals: [0,3,5,7,10],           tags: ['safe'] },
    { id: 'harmonic_minor',     name: 'Harmonic Minor',     intervals: [0,2,3,5,7,8,11],       tags: ['dark'] },
    { id: 'phrygian',           name: 'Phrygian',           intervals: [0,1,3,5,7,8,10],       tags: ['dark'] },
    { id: 'minor_blues',        name: 'Minor Blues',        intervals: [0,3,5,6,7,10],         tags: ['dark'] },
    { id: 'whole_tone',         name: 'Whole Tone',         intervals: [0,2,4,6,8,10],         tags: ['tension'] },
    { id: 'chromatic',          name: 'Chromatic',          intervals: [0,1,2,3,4,5,6,7,8,9,10,11], tags: ['tension'] },
];

// --- Harness ----------------------------------------------------------------

let passed = 0, failed = 0;
function test(name, fn) {
    try { fn(); console.log(`  ok - ${name}`); passed++; }
    catch (e) { console.log(`  FAIL - ${name}\n    ${e.message}`); failed++; }
}
function assert(cond, msg) { if (!cond) throw new Error(msg || 'assertion failed'); }
function eq(a, b, msg) { if (a !== b) throw new Error(`${msg || 'mismatch'}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`); }

// --- Scoring correctness ----------------------------------------------------

console.log('rankScales: scoring correctness');

test('A-minor-only pattern at root=A scores minor_pentatonic at 1.0', () => {
    // Histogram with weight only on A pentatonic pcs: A(9) C(0) D(2) E(4) G(7)
    const hist = new Array(12).fill(0);
    hist[9] = 3; hist[0] = 2; hist[2] = 1; hist[4] = 2; hist[7] = 1;
    const ranked = rankScales({ scales: SCALES, hist, root: 9 });
    // Minor pentatonic at A = {A, C, D, E, G} → covers every pc with weight.
    assert(ranked[0].fit === 1, `expected 1.0, got ${ranked[0].fit}`);
    // Minor pentatonic (5 notes) should beat natural_minor (7 notes) on
    // tiebreak because both have fit=1 but minor_pent is tighter.
    assert(
        ranked[0].scale.id === 'minor_pentatonic' || ranked[0].scale.id === 'major_pentatonic',
        `expected a pentatonic at top, got ${ranked[0].scale.id}`
    );
});

test('C major scale pattern at root=C ranks major first', () => {
    // Histogram covering all 7 C major pcs with equal weight: C D E F G A B.
    // Only major covers all 7 → fit=1; pentatonic is a subset (5/7) so scores
    // lower despite being tighter.
    const hist = new Array(12).fill(0);
    [0, 2, 4, 5, 7, 9, 11].forEach(pc => { hist[pc] = 1; });
    const ranked = rankScales({ scales: SCALES, hist, root: 0 });
    eq(ranked[0].scale.id, 'major', 'major is the unique fit=1 scale');
    assert(ranked[0].fit === 1, `top fit should be 1.0, got ${ranked[0].fit}`);
});

test('chromatic never ranks first even when its fit is 1.0', () => {
    // Every histogram gives chromatic fit=1. It must still be last.
    const hist = [3,2,1,2,3,1,2,3,1,2,3,1]; // spread across all 12 pcs
    const ranked = rankScales({ scales: SCALES, hist, root: 0 });
    eq(ranked[ranked.length - 1].scale.id, 'chromatic', 'chromatic always last');
});

test('empty histogram returns input order with fit=0', () => {
    const hist = new Array(12).fill(0);
    const ranked = rankScales({ scales: SCALES, hist, root: 0 });
    eq(ranked.length, SCALES.length, 'same length');
    assert(ranked.every(r => r.fit === 0), 'all zero');
});

test('fit is rotation-invariant across roots', () => {
    // A pattern at root=C should produce the same fit scores as the
    // transposed equivalent at root=G - scale ranking is relative to the
    // chosen root, so the histogram + root combo is what matters.
    const histC = new Array(12).fill(0);
    [0, 2, 4, 5, 7, 9, 11].forEach(pc => { histC[pc] = 1; });
    const histG = new Array(12).fill(0);
    [7, 9, 11, 0, 2, 4, 6].forEach(pc => { histG[pc] = 1; });
    const rankedC = rankScales({ scales: SCALES, hist: histC, root: 0 });
    const rankedG = rankScales({ scales: SCALES, hist: histG, root: 7 });
    // Same ordering and same fit scores (up to the non-chromatic rows).
    for (let i = 0; i < rankedC.length; i++) {
        eq(rankedC[i].scale.id, rankedG[i].scale.id, `row ${i} id`);
        eq(rankedC[i].fit, rankedG[i].fit, `row ${i} fit`);
    }
});

test('tiebreak prefers tighter scale when fit is equal', () => {
    // Pattern plays only C and G (perfect 5th). At root=C:
    //   major_pent (5) fit 2/2 = 1 → should rank ahead of major (7) also 1.
    const hist = new Array(12).fill(0);
    hist[0] = 1; hist[7] = 1;
    const ranked = rankScales({ scales: SCALES, hist, root: 0 });
    assert(ranked[0].fit === 1, 'top fit 1');
    assert(ranked[0].size <= ranked[1].size, 'tighter first');
});

console.log('\nrankScales: edge cases');

test('negative intervals do not break root math', () => {
    // Defensive: nothing in the config has negative intervals, but the
    // modulo in rankScales should handle them anyway.
    const weird = [{ id: 'weird', name: 'Weird', intervals: [-1, 0, 1], tags: [] }];
    const hist = new Array(12).fill(0);
    hist[0] = 1; hist[1] = 1; hist[11] = 1;
    const ranked = rankScales({ scales: weird, hist, root: 0 });
    eq(ranked[0].fit, 1, 'covers all three pcs');
});

test('detected scale (natural_minor) appears in top 5 of Am pattern', () => {
    // More realistic: A minor pattern with scale tones and accents.
    // Verifies that the detected scale will visually surface in the
    // "Nearest to key" optgroup alongside pentatonic/dorian alternatives.
    const hist = new Array(12).fill(0);
    hist[9] = 4; // A
    hist[0] = 3; // C (minor 3rd)
    hist[4] = 3; // E (5th)
    hist[7] = 2; // G (b7)
    hist[2] = 1; // D (4th)
    hist[5] = 1; // F (b6)
    const ranked = rankScales({ scales: SCALES, hist, root: 9 });
    const topIds = ranked.slice(0, 5).map(r => r.scale.id);
    assert(topIds.includes('natural_minor'), `expected natural_minor in top 5, got ${topIds.join(', ')}`);
});

// --- Summary ----------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
