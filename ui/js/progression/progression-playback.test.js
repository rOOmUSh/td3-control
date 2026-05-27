// Tests for progression-playback.js - runs with Node.js
// Usage: node ui/js/progression/progression-playback.test.js

import {
    firstTimelinePatternIndex,
    startPlayback,
    stopPlayback,
} from './progression-playback.js';

let passed = 0, failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; }
    else passed++;
}

async function test(name, fn) {
    try {
        await fn();
        console.log(`  ok: ${name}`);
    } catch (e) {
        console.error(`  FAIL: ${name}: ${e.stack || e.message}`);
        failed++;
    }
}

await test('firstTimelinePatternIndex defaults to P1 when timeline missing', () => {
    assert(firstTimelinePatternIndex(null) === 0, 'null timeline -> P1');
    assert(firstTimelinePatternIndex([]) === 0, 'empty timeline -> P1');
});

await test('firstTimelinePatternIndex uses first timeline slot', () => {
    assert(firstTimelinePatternIndex([3, 1, 2, 4]) === 2, 'slot 3 -> pattern index 2');
    assert(firstTimelinePatternIndex([4]) === 3, 'slot 4 -> pattern index 3');
});

await test('firstTimelinePatternIndex clamps out-of-range values', () => {
    assert(firstTimelinePatternIndex([0]) === 0, 'slot 0 clamps to P1');
    assert(firstTimelinePatternIndex([9]) === 3, 'slot 9 clamps to P4');
    assert(firstTimelinePatternIndex(['x']) === 0, 'non-numeric clamps to P1');
});

await test('startPlayback preloads first pattern before transportStart', async () => {
    const calls = [];
    const patterns = [{ name: 'P1' }, { name: 'P2' }, { name: 'P3' }, { name: 'P4' }];
    let playing = false;
    let status = '';

    const api = {
        savePattern: async (g, p, s, data) => {
            calls.push(`save:${data.name}:${g}${p}${s}`);
        },
        transportStart: async (bpm) => {
            calls.push(`start:${bpm}`);
        },
    };
    const transport = {
        start: async () => { calls.push('transport.start'); },
    };

    const firstPatIdx = await startPlayback({
        api,
        timeline: [1, 1, 2, 2],
        getPattern: (idx) => patterns[idx],
        scratch: { group: 1, pattern: 2, side: 'A', label: 'G1-P2A' },
        bpm: 123,
        transport,
        stopAllPreviews: async () => { calls.push('stopPreviews'); },
        setPlaying: (v) => { playing = v; calls.push(`playing:${v}`); },
        setStatus: (msg) => { status = msg; calls.push(`status:${msg}`); },
    });

    assert(firstPatIdx === 0, 'returns first pattern index');
    assert(calls[0] === 'stopPreviews', 'previews stop first');
    assert(calls[1] === 'save:P1:12A', 'savePattern is first network action');
    assert(calls[2] === 'start:123', 'transportStart happens after savePattern');
    assert(calls[3] === 'playing:true', 'playing flips true after transportStart');
    assert(calls[4] === 'transport.start', 'transport animation starts after playing=true');
    assert(status === 'Playing - P1 → G1-P2A', 'status references P1 + scratch label');
    assert(playing === true, 'playing flag set');
});

await test('startPlayback honors a non-default first timeline entry', async () => {
    const calls = [];
    const patterns = [{ name: 'P1' }, { name: 'P2' }, { name: 'P3' }, { name: 'P4' }];

    await startPlayback({
        api: {
            savePattern: async (_g, _p, _s, data) => { calls.push(`save:${data.name}`); },
            transportStart: async () => { calls.push('start'); },
        },
        timeline: [3, 3, 1, 1],
        getPattern: (idx) => patterns[idx],
        scratch: { group: 1, pattern: 1, side: 'A', label: 'G1-P1A' },
        bpm: 120,
        transport: { start: async () => { calls.push('transport.start'); } },
        stopAllPreviews: async () => {},
        setPlaying: () => {},
        setStatus: () => {},
    });

    assert(calls[0] === 'save:P3', 'timeline first slot 3 preloads P3');
});

await test('startPlayback uses host audition when live update is off', async () => {
    const calls = [];
    const patterns = [{ name: 'P1' }, { name: 'P2' }, { name: 'P3' }, { name: 'P4' }];
    let playing = false;
    let status = '';

    const firstPatIdx = await startPlayback({
        api: {
            auditionPattern: async (data, bpm, looping) => {
                calls.push(`audition:${data.name}:${bpm}:${looping}`);
            },
            savePattern: async () => { calls.push('save'); },
            transportStart: async () => { calls.push('start'); },
        },
        timeline: [2, 1, 1, 1],
        getPattern: (idx) => patterns[idx],
        scratch: { group: 1, pattern: 1, side: 'A', label: 'G1-P1A' },
        bpm: 119.5,
        transport: { start: async (sync) => { calls.push(`transport.start:${sync === null}`); } },
        stopAllPreviews: async () => { calls.push('stopPreviews'); },
        liveUpdate: false,
        setPlaying: (v) => { playing = v; calls.push(`playing:${v}`); },
        setStatus: (msg) => { status = msg; calls.push(`status:${msg}`); },
    });

    assert(firstPatIdx === 1, 'returns first pattern index');
    assert(calls[0] === 'stopPreviews', 'previews stop first');
    assert(calls[1] === 'audition:P2:119.5:true', 'auditionPattern starts selected pattern');
    assert(!calls.includes('save'), 'savePattern not called');
    assert(!calls.includes('start'), 'transportStart not called');
    assert(calls[2] === 'playing:true', 'playing flips true after audition starts');
    assert(calls[3] === 'transport.start:true', 'local transport starts without sync payload');
    assert(status === 'Host audition: P2 (no save)', 'status reports host audition');
    assert(playing === true, 'playing flag set');
});

await test('stopPlayback stops transport, resets timeline, and clears playing state', async () => {
    const calls = [];
    let playing = true;
    let status = '';

    await stopPlayback({
        api: {
            transportStop: async () => { calls.push('transportStop'); },
        },
        transport: {
            stopWrapSync: () => { calls.push('transport.stopWrapSync'); },
            stop: () => { calls.push('transport.stop'); },
        },
        resetTimeline: () => { calls.push('resetTimeline'); },
        setPlaying: (v) => { playing = v; calls.push(`playing:${v}`); },
        setStatus: (msg) => { status = msg; calls.push(`status:${msg}`); },
    });

    assert(calls[0] === 'transport.stopWrapSync', 'wrap sync stops first');
    assert(calls[1] === 'transportStop', 'transportStop after wrap sync reset');
    assert(calls[2] === 'playing:false', 'playing false after stop');
    assert(calls[3] === 'transport.stop', 'transport.stop called');
    assert(calls[4] === 'resetTimeline', 'timeline reset called');
    assert(status === 'Stopped - timeline reset', 'stop status called');
    assert(playing === false, 'playing flag cleared');
});

await test('stopPlayback stops host audition in audition mode', async () => {
    const calls = [];

    await stopPlayback({
        api: {
            auditionStop: async () => { calls.push('auditionStop'); },
            transportStop: async () => { calls.push('transportStop'); },
        },
        transport: {
            stopWrapSync: () => { calls.push('transport.stopWrapSync'); },
            stop: () => { calls.push('transport.stop'); },
        },
        resetTimeline: () => { calls.push('resetTimeline'); },
        auditionMode: true,
        setPlaying: (v) => { calls.push(`playing:${v}`); },
        setStatus: (msg) => { calls.push(`status:${msg}`); },
    });

    assert(calls[0] === 'transport.stopWrapSync', 'wrap sync stops first');
    assert(calls[1] === 'auditionStop', 'auditionStop after wrap sync reset');
    assert(!calls.includes('transportStop'), 'transportStop not called for host audition');
    assert(calls[2] === 'playing:false', 'playing false after audition stop');
    assert(calls[3] === 'transport.stop', 'transport.stop called');
    assert(calls[4] === 'resetTimeline', 'timeline reset called');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
