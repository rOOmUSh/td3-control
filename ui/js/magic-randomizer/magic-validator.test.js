// Usage: node ui/js/magic-randomizer/magic-validator.test.js

import { analyzeScale } from './magic-scale-analysis.js';
import { computeMetrics, validateCandidate, sparseTier } from './magic-validator.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const C_MAJOR = { id: 'major', name: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] };

// Build a candidate from a 16-pitch (or null for rest) array.
function cand(pitches) {
    return { pitches, mask: pitches.map(p => p !== null) };
}

// --- sparseTier table ---

test('sparseTier table', () => {
    assert(sparseTier(16).label === 'dense');
    assert(sparseTier(12).label === 'dense');
    assert(sparseTier(11).label === 'medium');
    assert(sparseTier(8).label === 'medium');
    assert(sparseTier(7).label === 'sparse');
    assert(sparseTier(4).label === 'sparse');
    assert(sparseTier(3) === 'micro');
    assert(sparseTier(0) === 'micro');
});

// --- Metrics on a hand-crafted candidate ---

test('metrics: balanced 16-step C major candidate', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Pitches: C E G E  C D E G  A G E D  C E G C
    const pitches = [0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0];
    const c = cand(pitches);
    const m = computeMetrics(c, a);
    assert(m.activeCount === 16, 'all active');
    assert(m.rootCount === 4, `roots=${m.rootCount}`);
    assert(m.distinctPcs >= 4, 'enough variety');
    assert(m.rootInFirstHalf, 'root in first half');
    assert(m.rootInLastQuarter, 'root in last quarter');
    assert(!m.allStrongBeatsSamePitch, 'strong beats vary');
    assert(m.strongStableCount >= 3, 'most strong beats stable');
    assert(m.maxRunLen <= 1, `no run, got ${m.maxRunLen}`);
});

// --- Validator: hard rejects ---

test('validator rejects 4-in-a-row identical pitches', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [0, 0, 0, 0,  4, 7, 0, 4,  2, 4, 7, 0,  4, 7, 0, 4];
    const r = validateCandidate(cand(pitches), a);
    assert(!r.passed, 'should reject');
    assert(r.reasons.some(x => x.includes('run-')), 'reason mentions run');
});

test('validator rejects no-root in dense pattern', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Use scale pitches but never the root
    const pitches = [4, 7, 9, 4,  2, 7, 9, 4,  7, 11, 4, 7,  9, 2, 4, 7];
    const r = validateCandidate(cand(pitches), a);
    assert(!r.passed, 'should reject');
    assert(r.reasons.some(x => x.includes('root-count')), 'reason mentions root count');
});

test('validator rejects too-few distinct pcs in dense pattern', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Only C and G - distinct pcs = 2, well below the 4 threshold
    const pitches = [0, 7, 0, 7,  0, 7, 0, 7,  0, 7, 0, 7,  0, 7, 0, 7];
    const r = validateCandidate(cand(pitches), a);
    assert(!r.passed, 'should reject');
    assert(r.reasons.some(x => x.includes('distinct-pcs')), 'reason mentions distinct pcs');
});

test('validator rejects pc domination', () => {
    const a = analyzeScale(0, C_MAJOR);
    // C used 9 times - over the 7-cap for dense
    const pitches = [0, 0, 0, 4,  0, 0, 0, 7,  0, 0, 0, 4,  0, 2, 9, 11];
    const r = validateCandidate(cand(pitches), a);
    assert(!r.passed, 'should reject');
    assert(r.reasons.some(x => x.includes('domination')), `reason: ${r.reasons.join(',')}`);
});

test('validator accepts a balanced candidate', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0];
    const r = validateCandidate(cand(pitches), a);
    assert(r.passed, `should pass, reasons: ${r.reasons.join(',')}`);
});

// --- Sparse tier behaviour ---

test('validator: sparse pattern with single root passes', () => {
    const a = analyzeScale(0, C_MAJOR);
    // 5 active steps (sparse tier), root used once
    const pitches = [0, null, null, null,  4, null, null, null,
                     7, null, null, null,  9, null, null, 4];
    const r = validateCandidate(cand(pitches), a);
    assert(r.passed, `should pass, got reasons: ${r.reasons.join(',')}`);
});

test('validator: micro pattern (1-3 active) waives content rules', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [4, null, null, null,  null, null, null, null,
                     null, null, null, null,  null, null, null, null];
    const r = validateCandidate(cand(pitches), a);
    assert(r.passed, `1 active should pass, reasons: ${r.reasons.join(',')}`);
});

// --- Final-loop check ---

test('validator: enormous final-leap to unstable rejected', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Last note far from first, first not stable
    const pitches = [
        2, 4, 5, 7,  9, 11, 9, 7,
        5, 4, 2, 4,  5, 7, 9, -7,  // last note -7 is below; first is 2 (D, color, not stable)
    ];
    // Loop: |-7 - 2| = 9 > 12? It's 9 - under 12. We need >12 leap.
    // Replace last with -10 → |2 - -10| = 12 → still not >12. With 24:
    const pitches2 = [
        2, 4, 5, 7,  9, 11, 9, 7,
        5, 4, 2, 4,  5, 7, 9, 24,
    ]; // |2 - 24| = 22 > 12, first=2 is not stable
    const r = validateCandidate(cand(pitches2), a);
    assert(!r.passed, 'should reject big-leap-to-unstable');
    assert(r.reasons.some(x => x.includes('loop')), `reason mentions loop, got: ${r.reasons.join(',')}`);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
