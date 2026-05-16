// Usage: node ui/js/magic-randomizer/magic-scorer.test.js

import { analyzeScale } from './magic-scale-analysis.js';
import { computeMetrics } from './magic-validator.js';
import { scoreCandidate, SCORE_WEIGHTS } from './magic-scorer.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const C_MAJOR = { id: 'major', name: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] };

function cand(pitches) {
    return { pitches, mask: pitches.map(p => p !== null) };
}
function score(c, a) {
    const m = computeMetrics(c, a);
    return { ...scoreCandidate(c, a, m), metrics: m };
}

test('weights sum to 100', () => {
    const total = Object.values(SCORE_WEIGHTS).reduce((a, b) => a + b, 0);
    assert(total === 100, `sum=${total}`);
});

test('balanced melody scores well', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0];
    const r = score(cand(pitches), a);
    assert(r.score >= 75, `expected >=75, got ${r.score} breakdown=${JSON.stringify(r.breakdown)}`);
});

test('two-note stuck pattern scores poorly', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [0, 7, 0, 7,  0, 7, 0, 7,  0, 7, 0, 7,  0, 7, 0, 7];
    const r = score(cand(pitches), a);
    assert(r.score < 70, `expected <70 for two-note loop, got ${r.score}`);
});

test('all-leap chaos scores poorly', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Maximize jumps; remove root anchors so the chaos isn't masked by
    // strong-beat stability
    const pitches = [4, 16, -7, 11,  -10, 16, -5, 19,  -12, 14, -3, 21,  9, 14, 7, -4];
    const r = score(cand(pitches), a);
    assert(r.score < 70, `expected <70 for chaos, got ${r.score} breakdown=${JSON.stringify(r.breakdown)}`);
});

test('balanced melody scores higher than stuck or chaotic', () => {
    const a = analyzeScale(0, C_MAJOR);
    const balanced = [0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0];
    const stuck    = [0, 7, 0, 7,  0, 7, 0, 7,  0, 7, 0, 7,  0, 7, 0, 7];
    const chaotic  = [0, 12, -7, 11,  -10, 16, -5, 19,  -12, 14, -3, 21,  0, 14, 7, -4];
    const sa = score(cand(balanced), a).score;
    const sb = score(cand(stuck), a).score;
    const sc = score(cand(chaotic), a).score;
    assert(sa > sb, `balanced(${sa}) > stuck(${sb})`);
    assert(sa > sc, `balanced(${sa}) > chaotic(${sc})`);
});

test('motif: 1-4 mirroring 9-12 scores higher than no-mirror', () => {
    const a = analyzeScale(0, C_MAJOR);
    const withMotif    = [0, 2, 4, 5,  7, 9, 7, 5,  0, 2, 4, 5,  4, 2, 0, 0];
    const withoutMotif = [0, 4, 2, 7,  9, 5, 11, 4,  2, 9, 7, 0,  4, 2, 0, 7];
    const m1 = computeMetrics(cand(withMotif), a);
    const m2 = computeMetrics(cand(withoutMotif), a);
    const s1 = scoreCandidate(cand(withMotif), a, m1);
    const s2 = scoreCandidate(cand(withoutMotif), a, m2);
    assert(s1.breakdown.motif >= s2.breakdown.motif,
        `motif: ${s1.breakdown.motif.toFixed(2)} vs ${s2.breakdown.motif.toFixed(2)}`);
});

test('phrase shape: ends-on-stable beats ends-on-tension', () => {
    const a = analyzeScale(0, C_MAJOR);
    const goodEnding  = [0, 4, 7, 9,  4, 2, 7, 9,  5, 11, 7, 4,  2, 4, 7, 0];
    // Same melody but ends on F (not stable) instead of C
    const weakEnding  = [0, 4, 7, 9,  4, 2, 7, 9,  5, 11, 7, 4,  2, 4, 7, 5];
    const m1 = computeMetrics(cand(goodEnding), a);
    const m2 = computeMetrics(cand(weakEnding), a);
    const s1 = scoreCandidate(cand(goodEnding), a, m1);
    const s2 = scoreCandidate(cand(weakEnding), a, m2);
    assert(s1.breakdown.phraseShape >= s2.breakdown.phraseShape,
        `phrase: ${s1.breakdown.phraseShape.toFixed(2)} vs ${s2.breakdown.phraseShape.toFixed(2)}`);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
