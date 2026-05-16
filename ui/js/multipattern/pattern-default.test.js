// Usage: node ui/js/multipattern/pattern-default.test.js
//
// Verifies the pure pattern-default helpers. These are load-bearing for the
// LOAD ALL overwrite-confirm guard - a false "is default" would let
// LOAD ALL silently overwrite user work.

import {
    defaultStep,
    defaultPattern,
    isStepDefault,
    isPatternDefault,
    clonePattern,
} from './pattern-default.js';

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; return; }
    passed++;
}
function test(name, fn) {
    try { fn(); console.log(`  ok: ${name}`); }
    catch (e) { console.error(`  FAIL: ${name}: ${e.stack || e.message}`); failed++; }
}

console.log('pattern-default tests:');

test('defaultStep has expected shape', () => {
    const s = defaultStep();
    assert(s.note === 'C', 'note C');
    assert(s.transpose === 'NORMAL', 'transpose NORMAL');
    assert(s.time === 'NORMAL', 'time NORMAL');
    assert(s.accent === false, 'accent false');
    assert(s.slide === false, 'slide false');
});

test('defaultPattern is 16 default steps, triplet off, active_steps=16', () => {
    const p = defaultPattern();
    assert(p.active_steps === 16, 'active_steps');
    assert(p.triplet === false, 'triplet');
    assert(p.steps.length === 16, 'length 16');
    for (const s of p.steps) assert(isStepDefault(s), 'every step default');
});

test('isStepDefault distinguishes non-defaults', () => {
    assert(isStepDefault(defaultStep()) === true, 'default is default');
    assert(isStepDefault({ ...defaultStep(), note: 'D' }) === false, 'note D ≠ default');
    assert(isStepDefault({ ...defaultStep(), accent: true }) === false, 'accent true ≠ default');
    assert(isStepDefault({ ...defaultStep(), slide: true }) === false, 'slide true ≠ default');
    assert(isStepDefault({ ...defaultStep(), time: 'REST' }) === false, 'REST ≠ default');
    assert(isStepDefault({ ...defaultStep(), transpose: 'UP' }) === false, 'UP ≠ default');
    assert(isStepDefault(null) === false, 'null ≠ default');
    assert(isStepDefault(undefined) === false, 'undefined ≠ default');
});

test('isPatternDefault rejects any edited pattern', () => {
    assert(isPatternDefault(defaultPattern()) === true, 'fresh default');

    const edited1 = defaultPattern();
    edited1.steps[3].note = 'F';
    assert(isPatternDefault(edited1) === false, 'note change flips default');

    const edited2 = defaultPattern();
    edited2.triplet = true;
    assert(isPatternDefault(edited2) === false, 'triplet change flips default');

    const edited3 = defaultPattern();
    edited3.active_steps = 8;
    assert(isPatternDefault(edited3) === false, 'active_steps change flips default');

    const edited4 = defaultPattern();
    edited4.steps[15].time = 'REST';
    assert(isPatternDefault(edited4) === false, 'REST step flips default');
});

test('isPatternDefault handles malformed input safely', () => {
    assert(isPatternDefault(null) === false, 'null');
    assert(isPatternDefault(undefined) === false, 'undefined');
    assert(isPatternDefault({}) === false, 'empty object');
    assert(isPatternDefault({ steps: [] }) === false, 'empty steps');
    assert(isPatternDefault({ steps: Array.from({ length: 15 }, defaultStep), active_steps: 16, triplet: false }) === false, '15 steps ≠ 16');
});

test('clonePattern produces an independent deep copy', () => {
    const p = defaultPattern();
    p.steps[0].note = 'E';
    const c = clonePattern(p);
    c.steps[0].note = 'G';
    assert(p.steps[0].note === 'E', 'original unchanged');
    assert(c.steps[0].note === 'G', 'copy modified');
    assert(c !== p, 'different refs');
    assert(c.steps !== p.steps, 'different steps array ref');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
