// Usage: node ui/js/shared/step-lanes.test.js

import {
    LANE_COLOR_MAX,
    LANE_COLOR_MID,
    LANE_COLOR_MIN,
    auditionLaneFields,
    defaultLaneValues,
    effectiveLane,
    effectiveLaneValue,
    effectiveLaneValues,
    isLaneDefault,
    laneColor,
    laneRequestFields,
    laneState,
    normalizeLaneValue,
    randomLaneValues,
    writeLaneState,
} from './step-lanes.js';

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

const rgb = ([r, g, b]) => `rgb(${r}, ${g}, ${b})`;

console.log('step-lanes tests:');

test('a pattern without lanes resolves to closed, off, and default values', () => {
    const lanes = laneState({ steps: [] });
    assert(lanes.open === false, 'closed');
    assert(lanes.cutoffOn === false && lanes.gateOn === false, 'both lanes off');
    assert(lanes.cutoff.length === 16 && lanes.cutoff.every((v) => v === 64), 'cutoff 64');
    assert(lanes.gate.length === 16 && lanes.gate.every((v) => v === 50), 'gate 50');
    assert(laneState(null).cutoff.length === 16, 'null pattern tolerated');
});

test('corrupt lane values are clamped or replaced per step', () => {
    const lanes = laneState({ lanes: { cutoff: [-5, 200, 'x', 63.6], gate: [0, 101, null] } });
    assert(lanes.cutoff.slice(0, 4).join(',') === '0,127,64,64', `cutoff got ${lanes.cutoff.slice(0, 4)}`);
    assert(lanes.gate.slice(0, 3).join(',') === '1,100,50', `gate got ${lanes.gate.slice(0, 3)}`);
    assert(lanes.cutoff.length === 16, 'short arrays are padded');
});

test('normalizeLaneValue clamps to the lane range and rejects unknown lanes', () => {
    assert(normalizeLaneValue('cutoff', 128) === 127, 'cutoff max');
    assert(normalizeLaneValue('gate', 0) === 1, 'gate min');
    assert(normalizeLaneValue('gate', 'junk') === 50, 'gate fallback');
    assert(normalizeLaneValue('nope', 1) === null, 'unknown lane');
});

test('writeLaneState stores a normalised copy on the pattern', () => {
    const pattern = { steps: [] };
    const written = writeLaneState(pattern, { open: 1, cutoffOn: 'yes', cutoff: [300] });
    assert(pattern.lanes === written, 'stored on the pattern');
    assert(written.open === true && written.cutoffOn === true, 'flags coerced');
    assert(written.cutoff[0] === 127 && written.cutoff[15] === 64, 'values normalised');
    assert(writeLaneState(null, {}) === null, 'null pattern is a no-op');
});

test('isLaneDefault and defaultLaneValues agree', () => {
    assert(isLaneDefault('cutoff', defaultLaneValues('cutoff')), 'default cutoff');
    assert(!isLaneDefault('cutoff', [65]), 'one changed step is not default');
    assert(isLaneDefault('gate', undefined), 'missing lane counts as default');
});

test('request fields follow the lane switches, the mode, and the morph state', () => {
    const pattern = {
        lanes: { cutoffOn: true, gateOn: true, cutoff: [1], gate: [2] },
    };
    const noLive = laneRequestFields(pattern, { noLive: true });
    assert(Array.isArray(noLive.stepCutoffs) && noLive.stepCutoffs[0] === 1, 'cutoff sent');
    assert(Array.isArray(noLive.stepGates) && noLive.stepGates[0] === 2, 'gate sent in NO-LIVE');
    const live = laneRequestFields(pattern, { noLive: false });
    assert(live.stepCutoffs && !live.stepGates, 'gate lane is NO-LIVE only');
    assert(auditionLaneFields(pattern).stepCutoffs?.[0] === 1, 'audition carries cutoff');
    assert(auditionLaneFields(pattern).stepGates?.[0] === 2, 'audition carries gate');
    const off = laneRequestFields({ lanes: { cutoff: [1], gate: [2] } });
    assert(Object.keys(off).length === 0, 'switched-off lanes are not sent');
    noLive.stepCutoffs[0] = 99;
    assert(pattern.lanes.cutoff[0] === 1, 'request arrays are copies');
});

test('ratio knob scales per-step values toward the top or bottom and restores at centre', () => {
    const base = [0, 127, 50, 100];
    assert(effectiveLaneValues('cutoff', base, 64).slice(0, 4).join(',') === '0,127,50,100',
        'centre is the identity');
    assert(effectiveLaneValues('cutoff', base, 96).slice(0, 4).join(',') === '64,127,89,114',
        `half way up: ${effectiveLaneValues('cutoff', base, 96).slice(0, 4)}`);
    assert(effectiveLaneValues('cutoff', base, 127).slice(0, 4).join(',') === '127,127,127,127',
        'top pins every step to the maximum');
    assert(effectiveLaneValues('cutoff', base, 32).slice(0, 4).join(',') === '0,64,25,50',
        `half way down: ${effectiveLaneValues('cutoff', base, 32).slice(0, 4)}`);
    assert(effectiveLaneValues('cutoff', base, 0).slice(0, 4).join(',') === '0,0,0,0',
        'bottom pins every step to the minimum');
    assert(effectiveLaneValue('gate', 50, 100) === 100 && effectiveLaneValue('gate', 50, 1) === 1,
        'gate uses its own 1..100 range');
    assert(effectiveLaneValue('gate', 20, 75) === 60, `gate half way up from 20: ${effectiveLaneValue('gate', 20, 75)}`);
    assert(effectiveLaneValue('nope', 1, 1) === null, 'unknown lane');
});

test('lane state carries ratios and request fields send effective values', () => {
    const lanes = laneState({ lanes: { cutoffOn: true, gateOn: true, cutoff: [0, 127, 50, 100], cutoffRatio: 96, gate: [50], gateRatio: 100 } });
    assert(lanes.cutoffRatio === 96 && lanes.gateRatio === 100, 'ratios kept');
    assert(laneState({}).cutoffRatio === 64 && laneState({}).gateRatio === 50, 'ratio defaults to centre');
    assert(laneState({ lanes: { cutoffRatio: 999 } }).cutoffRatio === 127, 'ratio clamped');
    assert(effectiveLane(lanes, 'cutoff').slice(0, 4).join(',') === '64,127,89,114', 'effective cutoff');
    const fields = laneRequestFields({ lanes }, { noLive: true });
    assert(fields.stepCutoffs.slice(0, 4).join(',') === '64,127,89,114', 'request carries effective cutoff');
    assert(fields.stepGates.every((v) => v === 100), 'request carries effective gate');
    assert(lanes.cutoff[0] === 0, 'base values untouched');
});

test('random lane values stay in range and cover both ends', () => {
    const cutoff = randomLaneValues('cutoff');
    assert(cutoff.length === 16, 'sixteen values');
    assert(cutoff.every((v) => Number.isInteger(v) && v >= 0 && v <= 127), 'cutoff in range');
    const gate = randomLaneValues('gate');
    assert(gate.every((v) => Number.isInteger(v) && v >= 1 && v <= 100), 'gate in range');
    assert(randomLaneValues('cutoff', () => 0).every((v) => v === 0), 'draw 0 hits the minimum');
    assert(randomLaneValues('cutoff', () => 0.999999).every((v) => v === 127), 'draw near 1 hits the maximum');
    assert(randomLaneValues('gate', () => 0).every((v) => v === 1), 'gate minimum is 1');
    assert(randomLaneValues('gate', () => 0.999999).every((v) => v === 100), 'gate maximum is 100');
    assert(randomLaneValues('nope') === null, 'unknown lane');
});

test('readout colour anchors: red at min, dimmed white at centre, green at max', () => {
    assert(laneColor('cutoff', 0) === rgb(LANE_COLOR_MIN), 'cutoff 0 is red');
    assert(laneColor('cutoff', 64) === rgb(LANE_COLOR_MID), 'cutoff 64 is dimmed white');
    assert(laneColor('cutoff', 127) === rgb(LANE_COLOR_MAX), 'cutoff 127 is green');
    assert(laneColor('gate', 1) === rgb(LANE_COLOR_MIN), 'gate 1 is red');
    assert(laneColor('gate', 50) === rgb(LANE_COLOR_MID), 'gate 50 is dimmed white');
    assert(laneColor('gate', 100) === rgb(LANE_COLOR_MAX), 'gate 100 is green');
});

test('readout colour blends linearly between anchors', () => {
    const quarter = laneColor('cutoff', 32);
    const [r, g, b] = quarter.match(/\d+/g).map(Number);
    assert(r < LANE_COLOR_MIN[0] && r > LANE_COLOR_MID[0], `red channel eases: ${quarter}`);
    assert(g > LANE_COLOR_MIN[1] && g < LANE_COLOR_MID[1], `green rises: ${quarter}`);
    assert(b > LANE_COLOR_MIN[2] && b < LANE_COLOR_MID[2], `blue rises: ${quarter}`);
    const upper = laneColor('cutoff', 96);
    const [, g2] = upper.match(/\d+/g).map(Number);
    assert(g2 > LANE_COLOR_MID[1] && g2 < LANE_COLOR_MAX[1], `upper half heads to green: ${upper}`);
    assert(laneColor('cutoff', 500) === rgb(LANE_COLOR_MAX), 'over-range clamps');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
