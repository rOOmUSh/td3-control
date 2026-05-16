// Tests for key-detection.js - runs with Node.js
// Usage: node ui/js/key-detection.test.js
//
// Uses inline copies of the production logic (same pattern as the other
// progression tests). Known-key fixtures verify detection output; a
// generator sweep builds a canonical pattern for every (root × mode)
// pair and confirms the algorithm recovers the intended key.

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const MIN_ACTIVE_NOTES = 3;

// --- Temperley profile correlation (active - mirrors key-detection.js) ------

const MAJOR_PROFILE_TEMPERLEY = [0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400];
const MINOR_PROFILE_TEMPERLEY = [0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330];

function pearson(a, b) {
    const n = a.length;
    let ma = 0, mb = 0;
    for (let i = 0; i < n; i++) { ma += a[i]; mb += b[i]; }
    ma /= n; mb /= n;
    let num = 0, da = 0, db = 0;
    for (let i = 0; i < n; i++) {
        const xa = a[i] - ma, xb = b[i] - mb;
        num += xa * xb; da += xa * xa; db += xb * xb;
    }
    const denom = Math.sqrt(da * db);
    return denom === 0 ? 0 : num / denom;
}

function rotateProfile(profile, root) {
    const out = new Array(12);
    for (let i = 0; i < 12; i++) out[i] = profile[(i - root + 12) % 12];
    return out;
}

function buildPitchClassHistogram(pattern) {
    const hist = new Array(12).fill(0);
    if (!pattern || !Array.isArray(pattern.steps)) return hist;
    for (const step of pattern.steps) {
        if (!step || step.time === 'REST' || step.time === 'TIE_REST') continue;
        const idx = NOTE_NAMES.indexOf(step.note);
        if (idx < 0) continue;
        const pc = idx % 12;
        const weight = step.accent ? 1.5 : 1.0;
        hist[pc] += weight;
    }
    return hist;
}

function countActiveNotes(pattern) {
    return pattern?.steps?.filter(s =>
        s && s.time !== 'REST' && s.time !== 'TIE_REST'
    ).length || 0;
}

function detectKey(pattern) {
    const hist = buildPitchClassHistogram(pattern);
    const noteCount = countActiveNotes(pattern);
    if (noteCount < MIN_ACTIVE_NOTES) return null;

    let best = { score: -Infinity, root: 0, mode: 'major' };
    let second = -Infinity;
    for (let pc = 0; pc < 12; pc++) {
        const sMaj = pearson(hist, rotateProfile(MAJOR_PROFILE_TEMPERLEY, pc));
        const sMin = pearson(hist, rotateProfile(MINOR_PROFILE_TEMPERLEY, pc));
        if (sMaj > best.score) { second = best.score; best = { score: sMaj, root: pc, mode: 'major' }; }
        else if (sMaj > second) { second = sMaj; }
        if (sMin > best.score) { second = best.score; best = { score: sMin, root: pc, mode: 'minor' }; }
        else if (sMin > second) { second = sMin; }
    }

    return {
        root: best.root,
        scaleId: best.mode === 'major' ? 'major' : 'natural_minor',
        mode: best.mode,
        confidence: second === -Infinity ? 0 : (best.score - second),
        noteCount,
    };
}

// --- Pattern builders -------------------------------------------------------

function makeStep(note, opts = {}) {
    return {
        note,
        transpose: opts.transpose || 'NORMAL',
        accent: !!opts.accent,
        slide: !!opts.slide,
        time: opts.time || 'NORMAL',
    };
}

function makePattern(noteSequence) {
    const steps = [];
    for (let i = 0; i < 16; i++) {
        const n = noteSequence[i % noteSequence.length];
        if (n === null) steps.push(makeStep('C', { time: 'REST' }));
        else if (typeof n === 'string') steps.push(makeStep(n));
        else steps.push(makeStep(n.note, n));
    }
    return { active_steps: 16, triplet: false, steps };
}

// Canonical 16-step pattern that emphasizes a given scale's tonic/3rd/5th.
// The sequence of scale degrees (0-indexed into the mode's intervals) is a
// tonic-anchored arpeggio + step-wise motion:
//   1 3 5 1 | 2 4 3 5 | 1 3 5 6 | 1 3 5 1
// This gives 5× tonic, 5× third, 4× fifth - strongly tonal, unambiguous key.
function generateCanonicalPattern(root, intervals) {
    const degreeSeq = [0, 2, 4, 0,  1, 3, 2, 4,  0, 2, 4, 5,  0, 2, 4, 0];
    const noteSeq = degreeSeq.map(d => {
        const pc = (root + intervals[d % intervals.length]) % 12;
        return NOTE_NAMES[pc];
    });
    return makePattern(noteSeq);
}

const INTERVALS_MAJOR = [0, 2, 4, 5, 7, 9, 11];
const INTERVALS_MINOR = [0, 2, 3, 5, 7, 8, 10];

// --- Harness ----------------------------------------------------------------

let passed = 0, failed = 0;
function test(name, fn) {
    try { fn(); console.log(`  ok - ${name}`); passed++; }
    catch (e) { console.log(`  FAIL - ${name}\n    ${e.message}`); failed++; }
}
function assert(cond, msg) { if (!cond) throw new Error(msg || 'assertion failed'); }
function eq(a, b, msg) { if (a !== b) throw new Error(`${msg || 'mismatch'}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`); }

// --- Generator sweep: every (root × mode) round-trips ------------------------

console.log('Generator sweep (canonical patterns, all 24 keys):');

for (let root = 0; root < 12; root++) {
    test(`generated ${NOTE_NAMES[root]} major → detected ${NOTE_NAMES[root]} major`, () => {
        const p = generateCanonicalPattern(root, INTERVALS_MAJOR);
        const d = detectKey(p);
        assert(d !== null, 'should detect');
        eq(d.root, root, 'root');
        eq(d.scaleId, 'major', 'scale');
    });
    test(`generated ${NOTE_NAMES[root]} minor → detected ${NOTE_NAMES[root]} natural_minor`, () => {
        const p = generateCanonicalPattern(root, INTERVALS_MINOR);
        const d = detectKey(p);
        assert(d !== null, 'should detect');
        eq(d.root, root, 'root');
        eq(d.scaleId, 'natural_minor', 'scale');
    });
}

// --- Hand-authored fixtures -------------------------------------------------

console.log('\ndetectKey on hand-authored fixtures:');

test('C major triad + scale tones → C major', () => {
    const p = makePattern(['C', 'E', 'G', 'C', 'D', 'F', 'E', 'G', 'C', 'E', 'G', 'A', 'C', 'E', 'G', 'C']);
    const d = detectKey(p);
    eq(d.root, 0, 'root');
    eq(d.scaleId, 'major', 'scale');
    assert(d.confidence > 0, 'confidence > 0');
});

test('A minor with typical acid-line emphasis → A minor', () => {
    const p = makePattern(['A', 'C', 'E', 'A', 'G', 'E', 'C', 'A', 'A', 'B', 'C', 'D', 'E', 'D', 'C', 'A']);
    const d = detectKey(p);
    eq(d.root, 9, 'root A');
    eq(d.scaleId, 'natural_minor', 'scale');
});

test('G major bassline → G major', () => {
    const p = makePattern(['G', 'B', 'D', 'G', 'A', 'G', 'D', 'B', 'G', 'B', 'D', 'G', 'F#', 'G', 'D', 'G']);
    const d = detectKey(p);
    eq(d.root, 7, 'root G');
    eq(d.scaleId, 'major', 'scale');
});

test('E minor pattern → E minor', () => {
    const p = makePattern(['E', 'G', 'B', 'E', 'D', 'B', 'G', 'E', 'E', 'F#', 'G', 'A', 'B', 'A', 'G', 'E']);
    const d = detectKey(p);
    eq(d.root, 4, 'root E');
    eq(d.scaleId, 'natural_minor', 'scale');
});

test('F# minor (sharp key) → F# minor', () => {
    const p = makePattern(['F#', 'A', 'C#', 'F#', 'E', 'C#', 'A', 'F#', 'F#', 'G#', 'A', 'B', 'C#', 'B', 'A', 'F#']);
    const d = detectKey(p);
    eq(d.root, 6, 'root F#');
    eq(d.scaleId, 'natural_minor', 'scale');
});

test('Bb major (flat key stored as A#) → A# root, major', () => {
    const p = makePattern(['A#', 'D', 'F', 'A#', 'C', 'A#', 'F', 'D', 'A#', 'D', 'F', 'A#', 'G', 'A#', 'F', 'A#']);
    const d = detectKey(p);
    eq(d.root, 10, 'root A#');
    eq(d.scaleId, 'major', 'scale');
});

test('C^ (octave C) counts as C pitch class', () => {
    const p = makePattern(['C^', 'E', 'G', 'C^', 'C^', 'E', 'G', 'C', 'D', 'F', 'E', 'G', 'C^', 'E', 'G', 'C^']);
    const d = detectKey(p);
    eq(d.root, 0, 'root C from C^');
});

console.log('\nEdge cases:');

test('all REST → null', () => {
    const p = makePattern([null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null]);
    eq(detectKey(p), null);
});

test('2 active notes → null (below MIN_ACTIVE_NOTES)', () => {
    const p = makePattern([null, 'C', null, null, null, null, null, null, 'G', null, null, null, null, null, null, null]);
    eq(detectKey(p), null);
});

test('exactly 3 active notes → returns detection', () => {
    const p = makePattern([null, 'C', null, null, 'E', null, null, null, 'G', null, null, null, null, null, null, null]);
    const d = detectKey(p);
    assert(d !== null, 'should detect');
    eq(d.noteCount, 3);
});

test('TIE_REST treated same as REST', () => {
    const steps = [];
    for (let i = 0; i < 16; i++) steps.push({ note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'TIE_REST' });
    const d = detectKey({ steps });
    eq(d, null, 'tie_rests contribute nothing');
});

test('null pattern → null', () => {
    eq(detectKey(null), null);
    eq(detectKey({}), null);
    eq(detectKey({ steps: [] }), null);
});

console.log('\nAmbiguity guard:');

test('uniform chromatic run → low confidence', () => {
    // Truly flat histogram: 12 distinct pc + 4 rests. Under SA, a flat
    // histogram puts the CoE near the geometric center of the helix,
    // roughly equidistant from all key centroids.
    const p = makePattern(['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', null, null, null, null]);
    const d = detectKey(p);
    assert(d !== null, 'should still return a best guess');
    assert(d.confidence < 0.05, `chromatic should be low-confidence, got ${d.confidence}`);
});

test('pure triad (strong tonic) → higher confidence than diffuse scale', () => {
    const triad = makePattern(['C', 'E', 'G', 'C', 'C', 'E', 'G', 'C', 'C', 'E', 'G', 'C', 'C', 'E', 'G', 'C']);
    const diffuse = makePattern(['C', 'D', 'E', 'F', 'G', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'A', 'B', 'C', 'D']);
    const dTriad = detectKey(triad);
    const dDiffuse = detectKey(diffuse);
    assert(dTriad.confidence > dDiffuse.confidence,
        `triad ${dTriad.confidence} should exceed diffuse ${dDiffuse.confidence}`);
});

// --- Summary ----------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
