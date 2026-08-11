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

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
