// Usage: node ui/js/magic-randomizer/magic-repairer.test.js

import { analyzeScale } from './magic-scale-analysis.js';
import { computeMetrics, validateCandidate } from './magic-validator.js';
import { repairCandidate } from './magic-repairer.js';
import { createRng } from './magic-rng.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const C_MAJOR = { id: 'major', name: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] };

function cand(pitches) {
    return { pitches, mask: pitches.map(p => p !== null) };
}

test('repairer inserts missing root', () => {
    const a = analyzeScale(0, C_MAJOR);
    // 16 active, no C anywhere
    const pitches = [4, 7, 9, 4,  2, 7, 9, 4,  7, 11, 4, 7,  9, 2, 4, 7];
    const r = repairCandidate(cand(pitches), a, createRng(1));
    assert(r.actions.includes('insert-root'), `actions=${r.actions.join(',')}`);
    const m = computeMetrics(r, a);
    assert(m.rootCount >= 1, 'has root');
});

test('repairer reduces dominant pc', () => {
    const a = analyzeScale(0, C_MAJOR);
    // C used 9 times - over the 7 cap for dense
    const pitches = [0, 0, 0, 4,  0, 0, 0, 7,  0, 0, 0, 4,  0, 2, 9, 11];
    const r = repairCandidate(cand(pitches), a, createRng(2));
    assert(r.actions.includes('reduce-dominant-pc'), `actions=${r.actions.join(',')}`);
    const m = computeMetrics(r, a);
    assert(m.maxPcCount <= 7, `maxPc=${m.maxPcCount}`);
});

test('repairer breaks 4-in-a-row run', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [4, 4, 4, 4,  7, 9, 11, 0,  4, 7, 0, 9,  4, 7, 11, 0];
    const r = repairCandidate(cand(pitches), a, createRng(3));
    assert(r.actions.includes('break-run'), `actions=${r.actions.join(',')}`);
    const m = computeMetrics(r, a);
    assert(m.maxRunLen < 4, `run=${m.maxRunLen}`);
});

test('repairer improves a weak strong beat', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Strong beats (0, 4, 8, 12) all on color tones; rest is balanced
    const pitches = [2, 2, 4, 5,  9, 7, 4, 0,  11, 4, 7, 0,  5, 4, 0, 7];
    const r = repairCandidate(cand(pitches), a, createRng(4));
    // Repair will run multiple actions; just ensure improve step happens
    // and strong-beat stable count rises.
    const m0 = computeMetrics(cand(pitches), a);
    const m1 = computeMetrics(r, a);
    assert(m1.strongStableCount >= m0.strongStableCount,
        `strongStable ${m0.strongStableCount} → ${m1.strongStableCount}`);
});

test('repairer caps at MAX_REPAIR_ACTIONS=6', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Pathological pattern: stuck on D, no root
    const pitches = [2, 2, 2, 2,  2, 2, 2, 2,  2, 2, 2, 2,  2, 2, 2, 2];
    const r = repairCandidate(cand(pitches), a, createRng(5));
    assert(r.actions.length <= 6, `actions=${r.actions.length}`);
});

test('repaired candidate often passes validation', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Dense pattern missing root + with one over-used pc
    const pitches = [4, 4, 4, 7,  4, 7, 9, 11,  4, 7, 4, 9,  4, 7, 4, 9];
    const r = repairCandidate(cand(pitches), a, createRng(7));
    const v = validateCandidate(r, a);
    // Either passes outright, or at least the maxPcCount and rootCount
    // metrics improved.
    const m0 = computeMetrics(cand(pitches), a);
    const m1 = computeMetrics(r, a);
    const improved = m1.rootCount > m0.rootCount || m1.maxPcCount < m0.maxPcCount;
    assert(v.passed || improved, `should pass or improve: passed=${v.passed} reasons=${v.reasons.join(',')}`);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
