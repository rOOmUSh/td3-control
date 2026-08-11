// Usage: node ui/js/multipattern/multipattern-export.test.js

import {
    buildRbsExportFilename,
    buildRbsExportPayload,
    buildSingleFileExportPlan,
    formatPatternExportTimestamp,
    PATTERN_SET_NAME_MAX_BYTES,
    sanitizePatternSetName,
} from './multipattern-export.js';

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
        'Acid_Set',
    );
    assert(result.error === null, 'no error');
    assert(result.count === 3, 'all patterns counted');
    assert(result.files.map(file => file.filename).join(',') === 'P001_Acid_Set.toml,P002_Acid_Set.toml,P003_Acid_Set.toml',
        'all filenames are sequence indexed');
});

test('single-file export uses checked patterns when present', () => {
    const result = buildSingleFileExportPlan(
        patterns,
        [2, 0],
        'json',
        '2026-07-21-08-46-13',
    );
    assert(result.error === null, 'no error');
    assert(result.count === 2, 'checked count');
    assert(result.files.map(file => file.filename).join(',') === 'P001_2026-07-21-08-46-13.json,P003_2026-07-21-08-46-13.json',
        'checked filenames retain source indexes');
});

test('single-file export uses the source index for one selected pattern', () => {
    const result = buildSingleFileExportPlan(
        patterns,
        [1],
        'seq',
        'Named Set',
    );
    assert(result.error === null, 'no error');
    assert(result.files[0].filename === 'P002_Named Set.seq', 'single filename uses source index');
});

test('single-file export rejects an empty sanitized name', () => {
    const result = buildSingleFileExportPlan(patterns, [], 'mid', ' ... ');
    assert(result.error === 'empty-name', 'empty sanitized name rejected');
});

test('pattern set names are safe filename components', () => {
    assert(sanitizePatternSetName('  Acid Set  ') === 'Acid Set', 'outer whitespace trimmed');
    assert(sanitizePatternSetName('unsafe/name?') === 'unsafe_name_', 'forbidden characters replaced');
    assert(sanitizePatternSetName('Acid Set... ') === 'Acid Set', 'unsafe trailing dots removed');
    assert(sanitizePatternSetName('CON') === '_CON', 'Windows reserved name protected');
    assert(sanitizePatternSetName('lpt9.mix') === '_lpt9.mix', 'reserved stem with suffix protected');
    assert(sanitizePatternSetName(' ... ') === '', 'dot-only name becomes empty');
});

test('pattern set names are capped without splitting Unicode characters', () => {
    const ascii = sanitizePatternSetName('A'.repeat(PATTERN_SET_NAME_MAX_BYTES + 40));
    assert(ascii.length === PATTERN_SET_NAME_MAX_BYTES, 'ASCII name capped');

    const knob = '\u{1f39b}';
    const unicode = sanitizePatternSetName(knob.repeat(40));
    assert(new TextEncoder().encode(unicode).length <= PATTERN_SET_NAME_MAX_BYTES,
        'Unicode name stays inside UTF-8 byte cap');
    assert(Array.from(unicode).length === 30, 'Unicode name retains every complete character that fits');
    assert(Array.from(unicode).every(character => character === knob),
        'Unicode characters remain intact');

    const plan = buildSingleFileExportPlan(patterns, [0], 'steps.txt', 'B'.repeat(200));
    assert(plan.files[0].filename.length < 140, 'ordinary filename keeps extension headroom');
    assert(buildRbsExportFilename('C'.repeat(200)).length < 130,
        'RBS filename keeps extension headroom');
});

test('export timestamps use local date and time components', () => {
    const date = new Date(2026, 6, 21, 8, 46, 13);
    assert(formatPatternExportTimestamp(date) === '2026-07-21-08-46-13', 'timestamp format');
});

test('RBS export uses only the shared name component', () => {
    assert(buildRbsExportFilename('Acid/Set', 'rbs') === 'Acid_Set.rbs', 'named RBS filename');
    assert(buildRbsExportFilename('2026-07-21-08-46-13') === '2026-07-21-08-46-13.rbs',
        'timestamped RBS filename');
    assert(buildRbsExportFilename(' ... ') === null, 'empty RBS name rejected');
});

if (failed > 0) {
    console.error(`\nmultipattern-export: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}

console.log(`\nmultipattern-export: ${passed} passed`);
