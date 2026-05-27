// Usage: node ui/js/multipattern/multipattern-import.test.js

import { detectImportFormat } from './multipattern-import.js';

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

console.log('multipattern-import tests:');

test('detectImportFormat recognises text single-pattern formats', () => {
    assert(detectImportFormat('x.toml').format === 'toml', 'toml');
    assert(detectImportFormat('x.json').format === 'json', 'json');
    assert(detectImportFormat('x.steps.txt').format === 'steps', 'steps.txt');
    assert(detectImportFormat('x.txt').format === 'steps', 'txt');
    assert(detectImportFormat('x.pat').format === 'pat', 'pat');
});

test('detectImportFormat recognises binary single-pattern formats', () => {
    const seq = detectImportFormat('x.seq');
    const mid = detectImportFormat('x.mid');
    const midi = detectImportFormat('x.midi');
    assert(seq.format === 'seq' && seq.binary === true && seq.bank === false, 'seq');
    assert(mid.format === 'mid' && mid.binary === true && mid.bank === false, 'mid');
    assert(midi.format === 'mid' && midi.binary === true && midi.bank === false, 'midi');
});

test('detectImportFormat recognises bank formats', () => {
    const sqs = detectImportFormat('x.sqs');
    const rbs = detectImportFormat('x.rbs');
    assert(sqs.format === 'sqs' && sqs.binary === true && sqs.bank === true, 'sqs');
    assert(rbs.format === 'rbs' && rbs.binary === true && rbs.bank === true, 'rbs');
});

test('detectImportFormat rejects unknown suffixes', () => {
    const result = detectImportFormat('x.wav');
    assert(result.error === 'unsupported', 'unsupported suffix');
});

if (failed > 0) {
    console.error(`\nmultipattern-import: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}

console.log(`\nmultipattern-import: ${passed} passed`);
