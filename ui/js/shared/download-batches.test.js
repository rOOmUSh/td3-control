// Usage: node ui/js/shared/download-batches.test.js

import { runDownloadBatches } from './download-batches.js';

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

async function test(name, fn) {
    try {
        await fn();
        console.log(`  ok: ${name}`);
    } catch (err) {
        console.error(`  FAIL: ${name}: ${err.stack || err.message}`);
        failed++;
    }
}

async function recordRun(count, delayMs = 2000) {
    const timeline = [];
    const items = Array.from({ length: count }, (_item, index) => index);
    await runDownloadBatches(
        items,
        async item => { timeline.push(`download:${item}`); },
        delayMs,
        async ms => { timeline.push(`wait:${ms}`); },
    );
    return timeline;
}

console.log('download-batches tests:');

await test('ten files need no pause', async () => {
    const timeline = await recordRun(10);
    assert(timeline.length === 10, 'only download events recorded');
    assert(!timeline.some(event => event.startsWith('wait:')), 'no pause');
});

await test('eleven files pause after the first ten', async () => {
    const timeline = await recordRun(11);
    assert(timeline[10] === 'wait:2000', 'pause follows the tenth download');
    assert(timeline[11] === 'download:10', 'last download follows pause');
});

await test('sixteen files pause once', async () => {
    const timeline = await recordRun(16);
    assert(timeline.filter(event => event === 'wait:2000').length === 1, 'one pause');
    assert(timeline.at(-1) === 'download:15', 'all sixteen files processed');
});

await test('twenty files do not pause after the final batch', async () => {
    const timeline = await recordRun(20);
    assert(timeline.filter(event => event === 'wait:2000').length === 1, 'only inter-batch pause');
    assert(timeline.at(-1) === 'download:19', 'final event is a download');
});

await test('twenty-one files pause twice', async () => {
    const timeline = await recordRun(21);
    assert(timeline.filter(event => event === 'wait:2000').length === 2, 'two pauses');
    assert(timeline.at(-1) === 'download:20', 'all twenty-one files processed');
});

await test('zero delay disables pauses', async () => {
    const timeline = await recordRun(21, 0);
    assert(timeline.length === 21, 'only download events recorded');
    assert(!timeline.some(event => event.startsWith('wait:')), 'no pauses');
});

await test('download failure stops remaining files', async () => {
    const processed = [];
    let message = '';
    try {
        await runDownloadBatches(
            Array.from({ length: 16 }, (_item, index) => index),
            async item => {
                processed.push(item);
                if (item === 4) throw new Error('download failed');
            },
            2000,
            async () => {},
        );
    } catch (err) {
        message = err.message;
    }
    assert(message === 'download failed', 'failure propagated');
    assert(processed.join(',') === '0,1,2,3,4', 'remaining files skipped');
});

if (failed > 0) {
    console.error(`\ndownload-batches: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}

console.log(`\ndownload-batches: ${passed} passed`);
