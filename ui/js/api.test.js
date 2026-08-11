// Usage: node ui/js/api.test.js

const requests = [];
globalThis.fetch = async (url, options) => {
    requests.push({
        url,
        method: options.method,
        body: options.body === undefined ? undefined : JSON.parse(options.body),
    });
    return {
        ok: true,
        async json() { return { ok: true }; },
    };
};

const { api } = await import('./api.js');

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

async function test(name, fn) {
    try {
        requests.length = 0;
        await fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

console.log('api audition wrapper tests:');

await test('legacy audition arguments omit gate', async () => {
    await api.auditionPattern({ id: 'start' }, 120, true, 123456);
    await api.auditionUpdate({ id: 'update' }, 121, true, 7);
    await api.auditionQueueNextCycle({ id: 'queue' }, 122, true, 8);
    assert(requests.every(({ body }) => body.gatePercent === undefined),
        'legacy calls do not gain a gate property');
    assert(requests[0].body.targetEpochMicros === 123456, 'start target position is preserved');
    assert(requests[1].body.expectedScheduleGeneration === 7,
        'update generation position is preserved');
    assert(requests[2].body.expectedScheduleGeneration === 8,
        'queue generation position is preserved');
});

await test('final optional argument serializes camelCase gate on every endpoint', async () => {
    await api.auditionPattern({ id: 'start' }, 120, true, null, 25);
    await api.auditionUpdate({ id: 'update' }, 121, true, null, 50);
    await api.auditionQueueNextCycle({ id: 'queue' }, 122, true, 9, 100);
    assert(requests.map(({ body }) => body.gatePercent).join(',') === '25,50,100',
        'all gate values serialize');
    assert(requests.every(({ body }) => body.gate_percent === undefined),
        'snake case is not emitted by browser wrappers');
    assert(requests.map(({ url }) => url).join(',')
        === '/api/pattern/audition,/api/pattern/audition/update,/api/pattern/audition/queue-next-cycle',
    'all endpoint paths are unchanged');
});

await test('legacy audition arguments omit the triplet morph amount', async () => {
    await api.auditionPattern({ id: 'start' }, 120, true, 123456, 50);
    await api.auditionUpdate({ id: 'update' }, 121, true, 7, 50);
    await api.auditionQueueNextCycle({ id: 'queue' }, 122, true, 8, 50);
    assert(requests.every(({ body }) => body.tripletMorphPercent === undefined),
        'legacy calls do not gain a morph property');
});

await test('an explicit morph amount serializes camelCase on every endpoint', async () => {
    await api.auditionPattern({ id: 'start' }, 120, true, null, 25, 0);
    await api.auditionUpdate({ id: 'update' }, 121, true, null, 50, 40);
    await api.auditionQueueNextCycle({ id: 'queue' }, 122, true, 9, 100, 100);
    assert(requests.map(({ body }) => body.tripletMorphPercent).join(',') === '0,40,100',
        'explicit zero and positive amounts all serialize');
    assert(requests.every(({ body }) => body.triplet_morph_percent === undefined),
        'snake case is not emitted by browser wrappers');
});

await test('the device MIDI channel serializes camelCase on every audition endpoint', async () => {
    await api.auditionPattern({ id: 'start' }, 120, true, null, 25, 0, 3);
    await api.auditionUpdate({ id: 'update' }, 121, true, null, 50, 40, 16);
    await api.auditionQueueNextCycle({ id: 'queue' }, 122, true, 9, 100, 100, 1);
    assert(requests.map(({ body }) => body.midiChannel).join(',') === '3,16,1',
        'the channel reaches every audition endpoint');
    assert(requests.every(({ body }) => body.midi_channel === undefined),
        'snake case is not emitted by browser wrappers');
});

await test('an omitted channel leaves the server default in charge', async () => {
    await api.auditionPattern({ id: 'start' }, 120, true, null, 25, 0);
    await api.notePreview('C', 'NORMAL', false);
    assert(requests.every(({ body }) => body.midiChannel === undefined),
        'no channel property is invented when the caller supplies none');
});

await test('note preview carries the device channel', async () => {
    await api.notePreview('C', 'NORMAL', true, 3);
    assert(requests[0].url === '/api/note/preview', 'preview endpoint path');
    assert(requests[0].body.midiChannel === 3, 'the channel travels with the preview');
    assert(requests[0].body.accent === true, 'the existing fields are untouched');
});

await test('triplet morph plan endpoint posts the canonical pattern', async () => {
    await api.tripletMorphPlan({ id: 'plan-source' });
    assert(requests.length === 1, 'one request');
    assert(requests[0].url === '/api/pattern/triplet-morph/plan', 'plan endpoint path');
    assert(requests[0].method === 'POST', 'plan endpoint method');
    assert(requests[0].body.pattern.id === 'plan-source', 'pattern travels in the body');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
