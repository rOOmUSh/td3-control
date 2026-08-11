// Usage: node ui/js/shared/triplet-morph-editing.test.js

import {
    MORPH_EDIT_ALL,
    MORPH_EDIT_LOCKED,
    MORPH_EDIT_SURVIVORS,
    editableSteps,
    isFullyLocked,
    isStepEditable,
    morphEditMode,
    morphEditNotice,
    survivingSteps,
} from './triplet-morph-editing.js';

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

// Default all-equal plan over `beats` beats: S2+S4 survive, S3 loses.
function planFor(beats) {
    const assignments = [];
    for (let beat = 0; beat < beats; beat += 1) {
        assignments.push(
            { step: beat * 4, survivor: true },
            { step: beat * 4 + 1, survivor: true },
            { step: beat * 4 + 2, survivor: false },
            { step: beat * 4 + 3, survivor: true },
        );
    }
    return { assignments };
}

console.log('triplet-morph-editing tests:');

test('edit mode is all at zero, locked mid-sweep, survivors at the endpoint', () => {
    assert(morphEditMode(0) === MORPH_EDIT_ALL, 'zero allows everything');
    for (const amount of [1, 25, 50, 75, 99]) {
        assert(morphEditMode(amount) === MORPH_EDIT_LOCKED, `${amount} is locked`);
    }
    assert(morphEditMode(100) === MORPH_EDIT_SURVIVORS, '100 allows survivors');
});

test('surviving steps come from the plan assignments', () => {
    const steps = survivingSteps(planFor(4));
    assert(steps.size === 12, 'twelve survivors over four beats');
    assert(steps.has(0) && steps.has(1) && steps.has(3), 'beat 0 survivors');
    assert(!steps.has(2) && !steps.has(6) && !steps.has(10) && !steps.has(14),
        'losers are excluded');
    assert(survivingSteps(null) === null, 'no plan yields null');
    assert(survivingSteps({ assignments: [] }) === null, 'empty plan yields null');
});

test('editable steps at zero mean every step', () => {
    assert(editableSteps(0, planFor(4)) === null, 'null means unrestricted');
    assert(editableSteps(0, null) === null, 'no plan needed at zero');
    assert(isStepEditable(0, null, 7), 'any step editable at zero');
    assert(!isFullyLocked(0, null), 'not locked at zero');
});

test('nothing is editable mid-sweep', () => {
    for (const amount of [1, 40, 99]) {
        const steps = editableSteps(amount, planFor(4));
        assert(steps instanceof Set && steps.size === 0, `${amount} yields an empty set`);
        assert(!isStepEditable(amount, planFor(4), 0), `${amount} blocks the anchor`);
        assert(isFullyLocked(amount, planFor(4)), `${amount} is fully locked`);
    }
});

test('only survivors are editable at the endpoint', () => {
    const plan = planFor(4);
    for (const step of [0, 1, 3, 4, 5, 7]) {
        assert(isStepEditable(100, plan, step), `survivor ${step} is editable`);
    }
    for (const step of [2, 6, 10, 14]) {
        assert(!isStepEditable(100, plan, step), `loser ${step} is not editable`);
    }
    assert(!isFullyLocked(100, plan), 'the endpoint is not fully locked');
});

test('a missing plan at the endpoint refuses rather than guessing', () => {
    const steps = editableSteps(100, null);
    assert(steps instanceof Set && steps.size === 0, 'empty set without a plan');
    assert(isFullyLocked(100, null), 'locked until the plan arrives');
});

test('shorter patterns expose only their own steps', () => {
    const oneBeat = planFor(1);
    assert(editableSteps(100, oneBeat).size === 3, 'three survivors in one beat');
    assert(isStepEditable(100, oneBeat, 3), 'step 3 survives');
    assert(!isStepEditable(100, oneBeat, 2), 'step 2 loses');
    assert(!isStepEditable(100, oneBeat, 7), 'a step past the pattern is not editable');

    const threeBeats = planFor(3);
    assert(editableSteps(100, threeBeats).size === 9, 'nine survivors in three beats');
    assert(!isStepEditable(100, threeBeats, 12), 'step past 12 steps is not editable');
});

test('the notice explains the current restriction', () => {
    assert(morphEditNotice(0) === null, 'no notice at zero');
    assert(morphEditNotice(50).includes('transition'), 'mid-sweep explains transition');
    assert(morphEditNotice(100).includes('triplet mode'), 'endpoint explains survivors');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
