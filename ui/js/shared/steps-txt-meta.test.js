// Usage: node ui/js/shared/steps-txt-meta.test.js

import { applyImportedStepsMeta, stepsMetaForExport } from './steps-txt-meta.js';

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

function pattern(lanes, activeSteps = 16, triplet = false) {
    return {
        active_steps: activeSteps,
        triplet,
        steps: Array.from({ length: 16 }, () => ({ note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' })),
        lanes,
    };
}

console.log('steps-txt-meta tests:');

test('untouched lanes export the transport-bar values on every step, switches off', () => {
    const meta = stepsMetaForExport(pattern(undefined), { globalCutoff: 90, globalGate: 33 });
    assert(meta.stepCutoffs.every((v) => v === 90), 'cutoff takes the bar value');
    assert(meta.stepGates.every((v) => v === 33), 'gate takes the bar value');
    assert(meta.cutoffLaneOn === false && meta.gateLaneOn === false, 'switches off');
    assert(meta.tripletMorphPercent === 0 && meta.liveUpdate === false, 'morph and live defaults');
});

test('a lane that is on exports its stored base values, ratio not applied', () => {
    const cutoff = Array.from({ length: 16 }, (_, i) => i * 8);
    const meta = stepsMetaForExport(pattern({ cutoffOn: true, cutoff, cutoffRatio: 127 }), { globalCutoff: 90 });
    assert(meta.stepCutoffs.join() === cutoff.join(), 'base values exported');
    assert(meta.cutoffLaneOn === true, 'switch on');
});

test('a lane that is off but edited keeps its values with the switch off', () => {
    const gate = Array.from({ length: 16 }, () => 50);
    gate[3] = 90;
    const meta = stepsMetaForExport(pattern({ gateOn: false, gate }), { globalGate: 20 });
    assert(meta.stepGates[3] === 90 && meta.stepGates[0] === 50, 'edited values kept');
    assert(meta.gateLaneOn === false, 'switch off');
});

test('morph is exported only above zero and for an eligible pattern; live mirrors the button', () => {
    const on = stepsMetaForExport(pattern(undefined, 16), { tripletMorphPercent: 69, liveUpdate: true });
    assert(on.tripletMorphPercent === 69 && on.liveUpdate === true, 'morph and live on');
    const ineligible = stepsMetaForExport(pattern(undefined, 7), { tripletMorphPercent: 69 });
    assert(ineligible.tripletMorphPercent === 0, 'seven steps is not eligible');
    const tripletTime = stepsMetaForExport(pattern(undefined, 16, true), { tripletMorphPercent: 69 });
    assert(tripletTime.tripletMorphPercent === 0, 'triplet time is not eligible');
});

test('import stores lanes with ratios at centre and honours the switches', () => {
    const result = applyImportedStepsMeta({
        meta: {
            stepCutoffs: Array.from({ length: 16 }, (_, i) => i),
            stepGates: Array.from({ length: 16 }, () => 70),
            cutoffLaneOn: true,
            gateLaneOn: false,
        },
        pattern: pattern(undefined),
        deviceControlsSupported: true,
    });
    assert(result.lanes.cutoff[5] === 5 && result.lanes.cutoffOn === true, 'cutoff lane applied');
    assert(result.lanes.gate[0] === 70 && result.lanes.gateOn === false, 'gate lane applied, off');
    assert(result.lanes.cutoffRatio === 64 && result.lanes.gateRatio === 50, 'ratios centred');
    assert(result.morphPercent === null && result.liveUpdate === null, 'nothing else to apply');
});

test('import drops the cutoff lane on a device that cannot be controlled', () => {
    const result = applyImportedStepsMeta({
        meta: { stepCutoffs: Array.from({ length: 16 }, () => 5), cutoffLaneOn: true },
        pattern: pattern(undefined),
        deviceControlsSupported: false,
    });
    assert(result.lanes.cutoff.every((v) => v === 64), 'cutoff stays default');
    assert(result.lanes.cutoffOn === false, 'cutoff lane off');
});

test('import applies morph only for a multiple of four straight steps, and live when present', () => {
    const ok = applyImportedStepsMeta({ meta: { tripletMorphPercent: 40, liveUpdate: true }, pattern: pattern(undefined, 12) });
    assert(ok.morphPercent === 40 && ok.liveUpdate === true, 'applied');
    const odd = applyImportedStepsMeta({ meta: { tripletMorphPercent: 40 }, pattern: pattern(undefined, 7) });
    assert(odd.morphPercent === null, 'seven steps ignored');
    const trip = applyImportedStepsMeta({ meta: { tripletMorphPercent: 40 }, pattern: pattern(undefined, 16, true) });
    assert(trip.morphPercent === null, 'triplet time ignored');
    const none = applyImportedStepsMeta({ meta: {}, pattern: pattern(undefined) });
    assert(none.liveUpdate === null && none.lanes.gateOn === false, 'empty meta is inert');
    const v1 = applyImportedStepsMeta({ meta: undefined, pattern: pattern(undefined) });
    assert(v1.lanes.cutoff.length === 16, 'absent meta still yields lanes');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
