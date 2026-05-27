// Usage: node ui/js/multipattern/multipattern-reset.test.js

import {
    resetCheckedOrAll,
    resetToolbarLabel,
    resetToolbarTitle,
} from './multipattern-reset.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`  FAIL: ${message}`);
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

function fakeState(checked, patternCount = 3) {
    const calls = [];
    return {
        calls,
        getCheckedArray: () => [...checked],
        getPatternCount: () => patternCount,
        resetAllPatterns: () => calls.push(['all']),
        resetPattern: index => calls.push(['pattern', index]),
    };
}

console.log('multipattern-reset tests:');

test('resetToolbarLabel shows all when nothing is checked', () => {
    assert(resetToolbarLabel(0) === 'RESET ALL PATTERNS', 'zero checked label');
});

test('resetToolbarLabel shows checked count when patterns are checked', () => {
    assert(resetToolbarLabel(1) === 'RESET PATTERN (1)', 'one checked label');
    assert(resetToolbarLabel(2) === 'RESET PATTERNS (2)', 'two checked label');
});

test('resetToolbarTitle follows checked count', () => {
    assert(resetToolbarTitle(0) === 'Reset every pattern to a blank pattern', 'all title');
    assert(resetToolbarTitle(1) === 'Reset the checked pattern to a blank pattern', 'one title');
    assert(resetToolbarTitle(3) === 'Reset 3 checked patterns to blank patterns', 'many title');
});

test('resetCheckedOrAll resets all patterns when no checks are active', () => {
    const state = fakeState([], 4);
    const result = resetCheckedOrAll(state);
    assert(result.mode === 'all', 'all mode');
    assert(result.count === 4, 'all count');
    assert(JSON.stringify(state.calls) === JSON.stringify([['all']]), 'all reset called once');
});

test('resetCheckedOrAll resets only checked patterns when checks are active', () => {
    const state = fakeState([2, 0], 4);
    const result = resetCheckedOrAll(state);
    assert(result.mode === 'checked', 'checked mode');
    assert(result.count === 2, 'checked count');
    assert(JSON.stringify(state.calls) === JSON.stringify([['pattern', 2], ['pattern', 0]]),
        'checked reset calls preserve checked order');
});

if (failed > 0) {
    console.error(`\nmultipattern-reset: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}

console.log(`\nmultipattern-reset: ${passed} passed`);
