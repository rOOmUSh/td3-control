// Usage: node ui/js/multipattern/multipattern-export.test.js

import { buildRbsExportPayload, buildSingleFileExportPlan } from './multipattern-export.js';

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

const patterns = [
    { id: 'p1', steps: Array.from({ length: 16 }, () => ({})) },
    { id: 'p2', steps: Array.from({ length: 16 }, () => ({})) },
    { id: 'p3', steps: Array.from({ length: 16 }, () => ({})) },
];

console.log('multipattern-export tests:');

test('RBS export uses all patterns when none are checked', () => {
    const result = buildRbsExportPayload(patterns, [], 'SERIAL');
    assert(result.error === null, 'no error');
    assert(result.count === 3, 'all patterns counted');
    assert(result.payload.patterns.map(p => p.id).join(',') === 'p1,p2,p3', 'all patterns selected');
    assert(result.payload.pattern.id === 'p1', 'mandatory single pattern is first selected');
    assert(result.payload.rbs_mode === 'SERIAL', 'mode preserved');
});

test('RBS export uses checked patterns in index order', () => {
    const result = buildRbsExportPayload(patterns, [2, 0], 'ALTERNATE');
    assert(result.error === null, 'no error');
    assert(result.count === 2, 'checked count');
    assert(result.payload.patterns.map(p => p.id).join(',') === 'p1,p3', 'checked patterns sorted');
    assert(result.payload.rbs_mode === 'ALTERNATE', 'mode preserved');
});

test('RBS export rejects invalid mode', () => {
    const result = buildRbsExportPayload(patterns, [], 'BAD');
    assert(result.error === 'bad-mode', 'bad mode rejected');
});

test('RBS export rejects checked index outside the pattern list', () => {
    const result = buildRbsExportPayload(patterns, [3], 'SERIAL');
    assert(result.error === 'index-out-of-range', 'bad index rejected');
});

test('single-file export uses all patterns when none are checked', () => {
    const result = buildSingleFileExportPlan(
        patterns,
        [],
        'toml',
        { group: 2, pattern: 3, side: 'B' },
    );
    assert(result.error === null, 'no error');
    assert(result.count === 3, 'all patterns counted');
    assert(result.files.map(file => file.filename).join(',') === 'pattern_P001.toml,pattern_P002.toml,pattern_P003.toml',
        'all filenames are sequence indexed');
});

test('single-file export uses checked patterns when present', () => {
    const result = buildSingleFileExportPlan(
        patterns,
        [2, 0],
        'json',
        { group: 2, pattern: 3, side: 'B' },
    );
    assert(result.error === null, 'no error');
    assert(result.count === 2, 'checked count');
    assert(result.files.map(file => file.filename).join(',') === 'pattern_P001.json,pattern_P003.json',
        'checked filenames retain source indexes');
});

test('single-file export preserves the legacy filename for one selected pattern', () => {
    const result = buildSingleFileExportPlan(
        patterns,
        [1],
        'seq',
        { group: 2, pattern: 3, side: 'B' },
    );
    assert(result.error === null, 'no error');
    assert(result.files[0].filename === 'pattern_G2P3B.seq', 'single filename uses selected slot');
});

if (failed > 0) {
    console.error(`\nmultipattern-export: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}

console.log(`\nmultipattern-export: ${passed} passed`);
