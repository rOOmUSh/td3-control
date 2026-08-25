// Usage: node ui/js/progression/progression-state.test.js

const values = new Map([['td3_gate_percent', 'corrupt']]);
globalThis.sessionStorage = {
    getItem(key) { return values.has(key) ? values.get(key) : null; },
    setItem(key, value) { values.set(key, String(value)); },
    removeItem(key) { values.delete(key); },
};

const state = await import('./progression-state.js');

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

console.log('progression-state gate tests:');

assert(state.getGatePercent() === 50, 'corrupt storage falls back to 50');
state.setGatePercent(25);
assert(state.getGatePercent() === 25, 'setter updates progression state');
assert(sessionStorage.getItem('td3_gate_percent') === '25',
    'setter uses the shared session key');
state.setGatePercent(-10);
assert(state.getGatePercent() === 1, 'setter clamps the lower bound');
state.setGatePercent(500);
assert(state.getGatePercent() === 100, 'setter clamps the upper bound');

console.log('progression-state step lane tests:');

function progPattern(note) {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, () => ({
            note, transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL',
        })),
    };
}

const progLanes = {
    open: true,
    cutoffOn: true,
    cutoff: Array.from({ length: 16 }, (_, i) => i * 8),
    gate: Array.from({ length: 16 }, () => 70),
    cutoffRatio: 96,
};

state.setLanes(1, progLanes);
state.setPattern(1, progPattern('D'));
assert(state.getLanes(1).open === true, 'setPattern keeps the row drawer open');
assert(state.getLanes(1).cutoff[3] === 24, 'setPattern keeps the per-step values');
assert(state.getLanes(1).cutoffRatio === 96, 'setPattern keeps the ratio');
assert(state.getPattern(1).steps[0].note === 'D', 'the new content did land');

state.setPatterns([progPattern('E'), progPattern('F'), progPattern('G'), progPattern('A')]);
assert(state.getLanes(1).open === true, 'a generated set leaves the drawer open');
assert(state.getLanes(1).cutoffOn === false, 'a generated set switches the lane off');
assert(state.getLanes(1).cutoff.every(v => v === 64), 'a generated set blanks the values');
assert(state.getLanes(1).cutoffRatio === 64, 'a generated set centres the ratio');
assert(state.getPattern(1).steps[0].note === 'F', 'the generated content did land');

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
