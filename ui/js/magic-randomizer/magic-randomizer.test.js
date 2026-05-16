// Usage: node ui/js/magic-randomizer/magic-randomizer.test.js
//
// End-to-end orchestrator: full + slice + progression-mode. The pipeline
// must always return a 16-step pattern, never throw, and respect the
// 65-acceptance / fall-back-to-best contract.

import {
    runMagicFull, runMagicSlice, runMagicProgression,
    BUDGET_SINGLE, BUDGET_BULK,
} from './magic-randomizer.js';
import { decodePitch, isPitchInRange } from './magic-pitch-encoding.js';
import { createRng } from './magic-rng.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const SCALES = {
    major:        { id: 'major', name: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] },
    minor_pent:   { id: 'mpent', name: 'Minor Pentatonic', intervals: [0, 3, 5, 7, 10] },
    phrygian_dom: { id: 'phrygd', name: 'Phrygian Dominant', intervals: [0, 1, 4, 5, 7, 8, 10] },
    chromatic:    { id: 'chrom', name: 'Chromatic', intervals: [0,1,2,3,4,5,6,7,8,9,10,11] },
};

// --- Budget constants ---

test('budget constants', () => {
    assert(BUDGET_SINGLE === 50, `single=${BUDGET_SINGLE}`);
    assert(BUDGET_BULK === 15, `bulk=${BUDGET_BULK}`);
});

// --- Full magic ---

test('runMagicFull: returns 16-step pattern with all decodable notes', () => {
    const result = runMagicFull({
        root: 0, scale: SCALES.major,
        notePercent: 1.0, slidePercent: 0.25, accPercent: 0.4,
        attempts: 25, rng: createRng(101),
    });
    assert(Array.isArray(result.steps) && result.steps.length === 16, '16 steps');
    for (const s of result.steps) {
        const dec = decodePitch(s.note, s.transpose);
        assert(dec !== null && isPitchInRange(dec), `decode ${s.note}/${s.transpose}`);
    }
});

test('runMagicFull: slider density preserved on output', () => {
    const result = runMagicFull({
        root: 0, scale: SCALES.major,
        notePercent: 1.0, slidePercent: 0.5, accPercent: 0.25,
        attempts: 20, rng: createRng(202),
    });
    const slides = result.steps.filter(s => s.slide).length;
    const accents = result.steps.filter(s => s.accent).length;
    // 16 active * 0.5 slide = 8; * 0.25 accent = 4
    assert(slides === 8, `slides=${slides}`);
    assert(accents === 4, `accents=${accents}`);
});

test('runMagicFull: works across diverse scales without throwing', () => {
    for (const [name, scale] of Object.entries(SCALES)) {
        for (const root of [0, 5, 9]) {
            const result = runMagicFull({
                root, scale,
                notePercent: 0.875, slidePercent: 0.2, accPercent: 0.3,
                attempts: 10, rng: createRng(name.length * 7 + root),
            });
            assert(result.steps.length === 16, `${name} root=${root}`);
            assert(result.magic.score >= 0, `score nonneg for ${name}`);
        }
    }
});

test('runMagicFull: sparse pattern (4 active) succeeds', () => {
    const result = runMagicFull({
        root: 0, scale: SCALES.major,
        notePercent: 0.25, slidePercent: 0.5, accPercent: 0.5,
        attempts: 20, rng: createRng(303),
    });
    const active = result.steps.filter(s => s.time !== 'REST' && s.time !== 'TIE_REST').length;
    assert(active === 4, `4 active, got ${active}`);
});

test('runMagicFull: 0% density returns all-REST pattern', () => {
    const result = runMagicFull({
        root: 0, scale: SCALES.major,
        notePercent: 0, slidePercent: 0, accPercent: 0,
        attempts: 5, rng: createRng(1),
    });
    const rests = result.steps.filter(s => s.time === 'REST').length;
    assert(rests === 16, `all rest, got ${rests}`);
});

// --- Slice ---

function makePrev() {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, (_, i) => ({
            note: i % 2 === 0 ? 'C' : 'E',
            transpose: 'NORMAL',
            accent: false, slide: false,
            time: 'NORMAL',
        })),
    };
}

test('runMagicSlice: only writes inside slice indices', () => {
    const prev = makePrev();
    const sliceIndices = [4, 5, 6, 7];
    const result = runMagicSlice({
        root: 0, scale: SCALES.major,
        prevPattern: prev, sliceIndices,
        notePercent: 1.0, slidePercent: 0, accPercent: 0,
        attempts: 15, rng: createRng(11),
    });
    // Outside slice: matches prev exactly
    for (let i = 0; i < 16; i++) {
        if (sliceIndices.includes(i)) continue;
        assert(result.steps[i].note === prev.steps[i].note
            && result.steps[i].transpose === prev.steps[i].transpose
            && result.steps[i].time === prev.steps[i].time,
            `step ${i} preserved outside slice`);
    }
});

test('runMagicSlice: every slice step decodes to a valid pitch', () => {
    const prev = makePrev();
    const sliceIndices = [12, 13, 14, 15];
    const result = runMagicSlice({
        root: 0, scale: SCALES.major,
        prevPattern: prev, sliceIndices,
        notePercent: 1.0, slidePercent: 0.25, accPercent: 0.5,
        attempts: 20, rng: createRng(44),
    });
    for (const i of sliceIndices) {
        const dec = decodePitch(result.steps[i].note, result.steps[i].transpose);
        assert(dec !== null && isPitchInRange(dec), `slice step ${i}`);
    }
});

test('runMagicSlice: empty sliceIndices returns prev untouched', () => {
    const prev = makePrev();
    const result = runMagicSlice({
        root: 0, scale: SCALES.major,
        prevPattern: prev, sliceIndices: [],
        notePercent: 1.0, slidePercent: 0, accPercent: 0,
        attempts: 5, rng: createRng(1),
    });
    assert(result.steps === prev.steps, 'returned same steps array');
});

// --- Progression mode ---

test('runMagicProgression: centerPc shifts the candidate cluster', () => {
    const result = runMagicProgression({
        root: 0, scale: SCALES.major,
        centerPc: 7,           // G
        registerCenter: 7,
        notePercent: 1.0, slidePercent: 0, accPercent: 0,
        attempts: 20, rng: createRng(77),
    });
    // Loose check: most active pitches near G (within ±5 semitones).
    let near = 0, total = 0;
    for (const s of result.steps) {
        if (s.time === 'REST') continue;
        const p = decodePitch(s.note, s.transpose);
        total++;
        if (Math.abs(p - 7) <= 5) near++;
    }
    assert(total > 0, 'has active');
    const ratio = near / total;
    assert(ratio >= 0.50, `≥50% near G, got ${(ratio * 100).toFixed(0)}%`);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
