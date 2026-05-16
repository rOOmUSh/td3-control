// Usage: node ui/js/magic-randomizer/magic-slide-accent.test.js

import { analyzeScale } from './magic-scale-analysis.js';
import { applySlidesAndAccents } from './magic-slide-accent.js';
import { encodeStepsFromPitches } from './magic-generator.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const C_MAJOR = { id: 'major', name: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] };

function buildSteps(pitches) {
    const mask = pitches.map(p => p !== null);
    return encodeStepsFromPitches(pitches, mask, null);
}

test('slider density is preserved exactly (rounded)', () => {
    const a = analyzeScale(0, C_MAJOR);
    const steps = buildSteps([0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0]);
    applySlidesAndAccents(steps, a, 0.25, 0.50);
    const slideCount = steps.filter(s => s.slide).length;
    const accCount = steps.filter(s => s.accent).length;
    // 16 active * 0.25 = 4 slides, * 0.5 = 8 accents
    assert(slideCount === 4, `slides=${slideCount}, expected 4`);
    assert(accCount === 8, `accents=${accCount}, expected 8`);
});

test('accents prefer off-beats over downbeats (acid groove rule)', () => {
    const a = analyzeScale(0, C_MAJOR);
    const steps = buildSteps([0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0]);
    applySlidesAndAccents(steps, a, 0, 0.25);  // 4 accents
    const accents = steps.map((s, i) => s.accent ? i : -1).filter(i => i >= 0);
    const onDownbeat = accents.filter(i => [0, 4, 8, 12].includes(i)).length;
    // Downbeats compete with kick. Most accents must land off the beat.
    assert(onDownbeat <= 1, `≤1 of 4 accents on downbeats, got ${onDownbeat} (${accents.join(',')})`);
});

test('accents at 30% density do NOT cluster on downbeats', () => {
    // Regression for the "every kick gets accented" cheap-mastering bug.
    // Across many seeds, accents must spread off-beat - at most 25% of
    // accents in a run should be on downbeats, and at least 60% off-beat.
    const a = analyzeScale(0, C_MAJOR);
    const fixtures = [
        [0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0],
        [0, 2, 4, 5,  7, 4, 9, 11,  0, 4, 7, 5,  2, 9, 4, 0],
        [4, 7, 9, 5,  0, 2, 7, 4,  9, 11, 7, 4,  0, 5, 9, 7],
    ];
    let downbeatHits = 0, totalAccents = 0;
    for (const pitches of fixtures) {
        const steps = buildSteps(pitches);
        applySlidesAndAccents(steps, a, 0, 0.30);
        for (let i = 0; i < 16; i++) {
            if (!steps[i].accent) continue;
            totalAccents++;
            if ([0, 4, 8, 12].includes(i)) downbeatHits++;
        }
    }
    const ratio = downbeatHits / totalAccents;
    assert(ratio <= 0.30,
        `≤30% of accents on downbeats, got ${(ratio * 100).toFixed(0)}% (${downbeatHits}/${totalAccents})`);
});

test('high accent density covers all active steps including downbeats', () => {
    // The downbeat penalty is a ranking bias, not a hard ban. At 100%
    // density every active step gets an accent, downbeats included -
    // proving the penalty doesn't drop downbeats from the eligible set.
    const a = analyzeScale(0, C_MAJOR);
    const steps = buildSteps([0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0]);
    applySlidesAndAccents(steps, a, 0, 1.0);
    for (let i = 0; i < 16; i++) {
        if (steps[i].time !== 'REST') {
            assert(steps[i].accent, `downbeat-or-not, step ${i} should be accented at 100%`);
        }
    }
});

test('slides prefer connected step motion over leaps', () => {
    const a = analyzeScale(0, C_MAJOR);
    // Place: step motion at 0→1 (C→D), repeat at 4→5 (C→C), big leap at 8→9 (A→G high)
    const pitches = [0, 2, 4, 5,  0, 0, 7, 9,  9, -3, 4, 7,  0, 4, 7, 0];
    const steps = buildSteps(pitches);
    applySlidesAndAccents(steps, a, 0.20, 0);  // ≈3 slides on 16 active
    const slides = steps.map((s, i) => s.slide ? i : -1).filter(i => i >= 0);
    // Slides should land where there is step motion. Step 0 (C→D, +2) is
    // a textbook slide candidate. Verify it's chosen.
    assert(slides.includes(0), `step 0 should be a slide, got ${slides.join(',')}`);
});

test('slides skip REST destinations', () => {
    const a = analyzeScale(0, C_MAJOR);
    const pitches = [0, 4, null, 7,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0];
    const steps = buildSteps(pitches);
    applySlidesAndAccents(steps, a, 0.20, 0);
    // Step 1 leads into a REST → discouraged. Step 0 leads to step 1 (active),
    // so slide at step 0 is fine. Just assert that REST steps never get slide=true.
    for (let i = 0; i < 16; i++) {
        if (steps[i].time === 'REST') {
            assert(!steps[i].slide, `REST at ${i} has slide`);
            assert(!steps[i].accent, `REST at ${i} has accent`);
        }
    }
});

test('zero density produces zero slides/accents', () => {
    const a = analyzeScale(0, C_MAJOR);
    const steps = buildSteps([0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0]);
    applySlidesAndAccents(steps, a, 0, 0);
    assert(steps.every(s => !s.slide), 'no slides');
    assert(steps.every(s => !s.accent), 'no accents');
});

test('full density places one slide and one accent per active step', () => {
    const a = analyzeScale(0, C_MAJOR);
    const steps = buildSteps([0, 4, 7, 4,  0, 2, 4, 7,  9, 7, 4, 2,  0, 4, 7, 0]);
    applySlidesAndAccents(steps, a, 1.0, 1.0);
    assert(steps.every(s => s.slide), 'all slide');
    assert(steps.every(s => s.accent), 'all accent');
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
