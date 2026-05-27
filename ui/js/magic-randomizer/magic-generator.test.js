// Usage: node ui/js/magic-randomizer/magic-generator.test.js
//
// End-to-end smoke for the candidate generator: with a healthy candidate
// budget, the validator should accept most candidates and the best score
// should be solidly in the "good" range. Uses seeded RNG for repro.

import { analyzeScale } from './magic-scale-analysis.js';
import { createRng } from './magic-rng.js';
import {
    buildActiveMask, generatePitchSequence, generateCandidate, generateCandidates,
    encodeStepsFromPitches,
} from './magic-generator.js';
import { validateCandidate } from './magic-validator.js';
import { scoreCandidate } from './magic-scorer.js';
import { isPitchInRange, decodePitch } from './magic-pitch-encoding.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const SCALES = {
    major:        { id: 'major', name: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] },
    nat_minor:    { id: 'minor', name: 'Natural Minor', intervals: [0, 2, 3, 5, 7, 8, 10] },
    phrygian_dom: { id: 'phrygd', name: 'Phrygian Dominant', intervals: [0, 1, 4, 5, 7, 8, 10] },
    minor_pent:   { id: 'mpent', name: 'Minor Pentatonic', intervals: [0, 3, 5, 7, 10] },
    chromatic:    { id: 'chrom', name: 'Chromatic', intervals: [0,1,2,3,4,5,6,7,8,9,10,11] },
};

// --- Mask ---

test('buildActiveMask: percentage rounds correctly', () => {
    const rng = createRng(1);
    assert(buildActiveMask(0, rng).filter(Boolean).length === 0, '0% → 0');
    assert(buildActiveMask(1, createRng(2)).filter(Boolean).length === 16, '100% → 16');
    assert(buildActiveMask(0.5, createRng(3)).filter(Boolean).length === 8, '50% → 8');
});

// --- Pitch sequence ---

test('generatePitchSequence: every active pitch is in TD-3 range', () => {
    const a = analyzeScale(0, SCALES.major);
    const rng = createRng(42);
    const mask = buildActiveMask(1.0, rng);
    const { pitches } = generatePitchSequence({ analysis: a, mask, rng });
    for (let i = 0; i < 16; i++) {
        if (mask[i]) assert(isPitchInRange(pitches[i]), `pitch ${pitches[i]} in range`);
    }
});

test('generatePitchSequence: every active pitch is in scale', () => {
    const a = analyzeScale(0, SCALES.major);
    const rng = createRng(7);
    const mask = buildActiveMask(1.0, rng);
    const { pitches } = generatePitchSequence({ analysis: a, mask, rng });
    for (let i = 0; i < 16; i++) {
        if (mask[i]) {
            const pc = ((pitches[i] % 12) + 12) % 12;
            assert(a.pcs.has(pc), `pitch ${pitches[i]} pc ${pc} in scale ${[...a.pcs].join(',')}`);
        }
    }
});

// --- Encoding ---

test('encodeStepsFromPitches: round-trips every active pitch', () => {
    const a = analyzeScale(0, SCALES.major);
    const rng = createRng(11);
    const mask = buildActiveMask(1.0, rng);
    const { pitches } = generatePitchSequence({ analysis: a, mask, rng });
    const steps = encodeStepsFromPitches(pitches, mask, null);
    assert(steps.length === 16, '16 steps');
    for (let i = 0; i < 16; i++) {
        if (mask[i]) {
            const decoded = decodePitch(steps[i].note, steps[i].transpose);
            assert(decoded === pitches[i], `step ${i}: ${pitches[i]} → ${steps[i].note}/${steps[i].transpose} → ${decoded}`);
            assert(steps[i].time === 'NORMAL', 'active is NORMAL');
        } else {
            assert(steps[i].time === 'REST', 'inactive is REST');
        }
    }
});

// --- End-to-end generation pipeline ---

test('generateCandidates: seeded run produces validated, scored output', () => {
    const a = analyzeScale(0, SCALES.major);
    const rng = createRng(123);
    const cs = generateCandidates({
        analysis: a, notePercent: 1.0, count: 50, rng,
    });
    let validCount = 0;
    let bestScore = -1;
    for (const c of cs) {
        const v = validateCandidate(c, a);
        if (v.passed) {
            validCount++;
            const { score } = scoreCandidate(c, a, v.metrics);
            if (score > bestScore) bestScore = score;
        }
    }
    // We expect a healthy chunk of candidates to validate (the sparse
    // table is permissive enough that >40% should pass at 100% density).
    assert(validCount >= 20, `expected ≥20 validated, got ${validCount}/50`);
    assert(bestScore >= 75, `best score should be ≥75, got ${bestScore}`);
});

test('generateCandidates: across diverse scales, best score is ≥70', () => {
    for (const [name, scale] of Object.entries(SCALES)) {
        const a = analyzeScale(0, scale);
        const rng = createRng(1000 + name.length);
        const cs = generateCandidates({
            analysis: a, notePercent: 1.0, count: 50, rng,
        });
        let bestScore = -1;
        for (const c of cs) {
            const v = validateCandidate(c, a);
            if (v.passed) {
                const { score } = scoreCandidate(c, a, v.metrics);
                if (score > bestScore) bestScore = score;
            }
        }
        assert(bestScore >= 70, `${name}: best=${bestScore}`);
    }
});

test('generateCandidates: respects centerPc != rootPc (progression mode)', () => {
    // C major scale, generate around the V degree (G, pc=7). Many active
    // pitches should land near G.
    const a = analyzeScale(0, SCALES.major);
    const rng = createRng(31);
    const cs = generateCandidates({
        analysis: a, notePercent: 1.0, count: 30, rng,
        centerPc: 7,        // G as center
        registerCenter: 7,  // pull register toward G
    });
    let total = 0, neighborhood = 0;
    for (const c of cs) {
        for (let i = 0; i < 16; i++) {
            if (c.mask[i]) {
                total++;
                if (Math.abs(c.pitches[i] - 7) <= 5) neighborhood++;
            }
        }
    }
    assert(total > 0, 'has active pitches');
    // Expect majority of pitches within ±5 semitones of G.
    const pct = neighborhood / total;
    assert(pct >= 0.5, `neighborhood pct=${pct.toFixed(2)}`);
});

// --- Sparse density ---

test('generateCandidate: sparse 4 active still encodes round-trippable steps', () => {
    const a = analyzeScale(0, SCALES.major);
    const rng = createRng(9);
    const c = generateCandidate({ analysis: a, notePercent: 0.25, rng });
    const activeCount = c.mask.filter(Boolean).length;
    assert(activeCount === 4, `4 active, got ${activeCount}`);
    for (let i = 0; i < 16; i++) {
        const dec = decodePitch(c.steps[i].note, c.steps[i].transpose);
        assert(dec !== null, `step ${i} decodes`);
    }
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
