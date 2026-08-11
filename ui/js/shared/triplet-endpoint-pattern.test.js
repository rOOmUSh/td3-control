// Usage: node ui/js/shared/triplet-endpoint-pattern.test.js
//
// The endpoint projection is reshaped from the planner's own
// `endpointCells`, never recomputed here. These cover the reshaping and
// every way a plan can fail to carry a usable projection.

import {
    endpointCellsInOrder,
    endpointPatternFromPlan,
} from './triplet-endpoint-pattern.js';

function step(note, extra = {}) {
    return {
        note, transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL', ...extra,
    };
}

function planWith(cells) {
    return { planVersion: 1, endpointCells: cells };
}

function twelveCells() {
    return Array.from({ length: 12 }, (_, index) => ({
        targetIndex: index,
        sourceStep: index,
        role: 'survivor',
        step: step(String(index)),
    }));
}

let passed = 0;
let failed = 0;
function check(condition, message) {
    if (condition) { passed += 1; return; }
    failed += 1;
    console.error(`  FAIL: ${message}`);
}

// --- Shape ---

const twelve = endpointPatternFromPlan(planWith(twelveCells()));
check(twelve.active_steps === 12, 'twelve target cells become twelve active steps');
check(twelve.triplet === true, 'the projection is a triplet pattern');
check(twelve.steps.length === 16, 'the pattern keeps sixteen cells');
check(twelve.steps[0].note === '0' && twelve.steps[11].note === '11',
    'cell content lands at its target index');
check(twelve.steps[12].time === 'REST' && twelve.steps[15].time === 'REST',
    'cells past the projection are rests');

// --- Ordering ---

const shuffled = twelveCells();
shuffled.reverse();
const reordered = endpointPatternFromPlan(planWith(shuffled));
check(reordered.steps[0].note === '0' && reordered.steps[11].note === '11',
    'cells are placed by targetIndex, not by array order');

// --- Content is carried verbatim ---

const flagged = twelveCells();
flagged[3].step = step('G', { accent: true, slide: true, time: 'TIE' });
const carried = endpointPatternFromPlan(planWith(flagged));
check(carried.steps[3].note === 'G', 'note carried');
check(carried.steps[3].accent === true && carried.steps[3].slide === true,
    'accent and slide carried');
check(carried.steps[3].time === 'TIE', 'time carried');

// --- Shorter sources ---

const threeCells = twelveCells().slice(0, 3);
const short = endpointPatternFromPlan(planWith(threeCells));
check(short.active_steps === 3, 'a one-beat source projects three cells');

// --- Rejections ---

check(endpointPatternFromPlan(null) === null, 'no plan');
check(endpointPatternFromPlan({}) === null, 'no endpointCells');
check(endpointPatternFromPlan(planWith([])) === null, 'empty projection');
check(endpointCellsInOrder(planWith(twelveCells().concat(twelveCells()))) === null,
    'more cells than the pattern can hold');

const gapped = twelveCells();
gapped[5].targetIndex = 99;
check(endpointPatternFromPlan(planWith(gapped)) === null,
    'a gap in the target indexes is refused rather than silently shifted');

const missingStep = twelveCells();
delete missingStep[2].step;
check(endpointPatternFromPlan(planWith(missingStep)) === null,
    'a cell without step content is refused');

// --- The plan is never mutated ---

const original = planWith(twelveCells());
const snapshot = JSON.stringify(original);
endpointPatternFromPlan(original);
check(JSON.stringify(original) === snapshot, 'the plan is left untouched');

console.log(`triplet-endpoint-pattern tests: ${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
