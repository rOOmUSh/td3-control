// Usage: node ui/js/shared/triplet-morph-timing.test.js

import {
    morphCellOffsetInCardWidths,
    morphDisplayStep,
    morphDisplayStepForUniformTick,
    warpedOnsetFraction,
} from './triplet-morph-timing.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`  FAIL: ${message}`);
        failed += 1;
        return;
    }
    passed += 1;
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

function near(a, b, eps = 1e-9) {
    return Math.abs(a - b) < eps;
}

// Default all-equal plan: every beat keeps S2+S4; S3 collides at 2/3.
function defaultAssignments() {
    const out = [];
    for (let beat = 0; beat < 4; beat += 1) {
        out.push({
            step: beat * 4,
            survivor: true,
            sourceOffset: { num: 0, den: 1 },
            targetOffset: { num: 0, den: 1 },
        });
        out.push({
            step: beat * 4 + 1,
            survivor: true,
            sourceOffset: { num: 1, den: 4 },
            targetOffset: { num: 1, den: 3 },
        });
        out.push({
            step: beat * 4 + 2,
            survivor: false,
            sourceOffset: { num: 1, den: 2 },
            targetOffset: { num: 2, den: 3 },
        });
        out.push({
            step: beat * 4 + 3,
            survivor: true,
            sourceOffset: { num: 3, den: 4 },
            targetOffset: { num: 2, den: 3 },
        });
    }
    return out;
}

console.log('triplet-morph-timing tests:');

test('warped onsets interpolate the worked example fractions', () => {
    const assignments = defaultAssignments();
    assert(near(warpedOnsetFraction(assignments[0], 50), 0), 'anchor stays at zero');
    assert(near(warpedOnsetFraction(assignments[1], 0), 0.25 / 4), 'S2 at 0 percent');
    assert(near(warpedOnsetFraction(assignments[1], 50), (7 / 24) / 4), 'S2 at 50 percent');
    assert(near(warpedOnsetFraction(assignments[1], 100), (1 / 3) / 4), 'S2 at 100 percent');
    assert(near(warpedOnsetFraction(assignments[2], 50), (7 / 12) / 4), 'S3 at 50 percent');
    assert(near(warpedOnsetFraction(assignments[3], 50), (17 / 24) / 4), 'S4 at 50 percent');
    assert(near(warpedOnsetFraction(assignments[4], 25), 0.25), 'beat 1 anchor fixed');
});

test('cell offsets are expressed in card widths for CSS transforms', () => {
    const assignments = defaultAssignments();
    assert(near(morphCellOffsetInCardWidths(assignments[0], 80), 0), 'anchor never moves');
    assert(near(morphCellOffsetInCardWidths(assignments[1], 0), 0), 'no offset at zero');
    // S2 at 100 percent: 1/3 beat vs 1/4 beat = 1/12 beat = 1/3 card.
    assert(near(morphCellOffsetInCardWidths(assignments[1], 100), 1 / 3), 'S2 endpoint offset');
    // S4 moves backward toward 2/3.
    assert(morphCellOffsetInCardWidths(assignments[3], 100) < 0, 'S4 moves earlier');
});

test('display step follows warped onsets across the cycle', () => {
    const assignments = defaultAssignments();
    assert(morphDisplayStep(assignments, 50, 0) === 0, 'phase zero highlights the anchor');
    // At 50 percent S2 sits at 7/24 beat = 0.0729 of the cycle.
    assert(morphDisplayStep(assignments, 50, 0.07) === 0, 'before S2 the anchor holds');
    assert(morphDisplayStep(assignments, 50, 0.074) === 1, 'after S2 onset highlight S2');
    assert(morphDisplayStep(assignments, 50, 0.25) === 4, 'beat 1 anchor at quarter cycle');
    assert(morphDisplayStep(assignments, 50, 0.999) === 15, 'tail of the cycle');
});

test('the endpoint walks only the twelve surviving cells', () => {
    const assignments = defaultAssignments();
    const seen = new Set();
    for (let tick = 0; tick < 16; tick += 1) {
        seen.add(morphDisplayStepForUniformTick(assignments, 100, tick, 16));
    }
    assert(!seen.has(2), 'the losing cell is never highlighted at 100');
    assert(!seen.has(6) && !seen.has(10) && !seen.has(14), 'no loser in any beat');
    assert(seen.has(0) && seen.has(1) && seen.has(3), 'beat 0 targets all appear');
});

test('uniform tick translation is identity-like at zero amount', () => {
    const assignments = defaultAssignments();
    for (let tick = 0; tick < 16; tick += 1) {
        assert(
            morphDisplayStepForUniformTick(assignments, 0, tick, 16) === tick,
            `tick ${tick} maps to itself at amount 0`,
        );
    }
});

test('degenerate inputs stay safe', () => {
    assert(morphDisplayStep([], 50, 0.5) === 0, 'empty assignments yield step 0');
    assert(morphDisplayStep(null, 50, 0.5) === 0, 'null assignments yield step 0');
    assert(near(warpedOnsetFraction({ step: 5 }, 50), 0.25), 'missing offsets act as anchors');
    assert(
        morphDisplayStepForUniformTick(defaultAssignments(), 40, -1, 16) >= 0,
        'negative ticks wrap',
    );
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
