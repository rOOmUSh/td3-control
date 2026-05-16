import { buildStepCardViewModel } from './step-card-view.js';

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) {
        console.error(`  FAIL: ${msg}`);
        failed++;
        return;
    }
    passed++;
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (err) {
        console.error(`  FAIL: ${name}: ${err.stack || err.message}`);
        failed++;
    }
}

function makeStep(overrides = {}) {
    return {
        note: 'C',
        transpose: 'NORMAL',
        accent: false,
        slide: false,
        time: 'NORMAL',
        ...overrides,
    };
}

console.log('step-card-view tests:');

test('normal downbeat view model matches progression card sizing and downbeat styling', () => {
    const view = buildStepCardViewModel({
        step: makeStep(),
        index: 0,
        activeSteps: 16,
    });

    assert(view.cardClassName.includes('h-16'), 'uses progression card height');
    assert(view.cardClassName.includes('step-downbeat'), 'downbeat class applied');
    assert(view.cardClassName.includes('bg-surface-container-highest'), 'downbeat background uses brighter surface');
    assert(view.numberClassName.includes('text-[0.7rem]'), 'step number uses progression sizing');
    assert(view.controlsClassName.includes('p-0.5'), 'controls padding matches progression');
});

test('selected main-page card adds keyboard cursor without changing shared card sizing', () => {
    const view = buildStepCardViewModel({
        step: makeStep(),
        index: 3,
        activeSteps: 16,
        selected: true,
    });

    assert(view.cardClassName.includes('step-kb-selected'), 'keyboard-selected class applied');
    assert(view.cardClassName.includes('h-16'), 'selected card still uses shared progression height');
});

test('rest and tie states use compact progression labels and disable indicators', () => {
    const restView = buildStepCardViewModel({
        step: makeStep({ time: 'REST' }),
        index: 1,
        activeSteps: 16,
    });
    const tieView = buildStepCardViewModel({
        step: makeStep({ time: 'TIE' }),
        index: 2,
        activeSteps: 16,
    });

    assert(restView.noteLabelText === 'REST', 'rest label text preserved');
    assert(restView.noteLabelClassName.includes('text-xs'), 'rest uses compact progression text size');
    assert(!restView.showIndicators, 'rest hides accent/slide indicators');
    assert(tieView.noteLabelText === 'TIE', 'tie label text preserved');
    assert(tieView.noteLabelClassName.includes('text-xs'), 'tie uses compact progression text size');
});

test('transpose indicator uses progression sizing and correct text', () => {
    const upView = buildStepCardViewModel({
        step: makeStep({ transpose: 'UP' }),
        index: 4,
        activeSteps: 16,
    });
    const downView = buildStepCardViewModel({
        step: makeStep({ transpose: 'DOWN' }),
        index: 5,
        activeSteps: 16,
    });

    assert(upView.showTranspose, 'transpose shown for UP');
    assert(upView.transposeClassName.includes('text-[0.7rem]'), 'transpose uses progression sizing');
    assert(upView.transposeText === 'UP', 'transpose text UP preserved');
    assert(downView.transposeText === 'DN', 'transpose text DN preserved');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
