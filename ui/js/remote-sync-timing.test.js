import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    cycleDurationMicros,
    eventEpochMicros,
    startSyncFromTargetMicros,
    targetEpochMicrosForPlay,
} from './remote-sync-timing.js';
import { parsePort, unavailableMessageForPort } from './remote-sync.js';

test('eventEpochMicros converts high-resolution event time to epoch micros', () => {
    assert.equal(eventEpochMicros({ timeStamp: 345.678 }, 1000), 1345678);
});

test('eventEpochMicros accepts epoch-style event timestamps', () => {
    assert.equal(eventEpochMicros({ timeStamp: 1_763_456_789_123 }, 1000), 1_763_456_789_123_000);
});

test('cycleDurationMicros matches normal 16-step timing at 125 BPM', () => {
    assert.equal(cycleDurationMicros(125, 16, false), 1_920_000);
});

test('cycleDurationMicros supports triplet timing and active steps', () => {
    assert.equal(cycleDurationMicros(120, 8, true), 1_333_312);
});

test('targetEpochMicrosForPlay adds one cycle to the click epoch', () => {
    assert.equal(
        targetEpochMicrosForPlay({ timeStamp: 345.678 }, 125, 16, false, 1000),
        3_265_678,
    );
});

test('startSyncFromTargetMicros returns transport-compatible milliseconds', () => {
    assert.deepEqual(startSyncFromTargetMicros(3_265_678), { startedAtEpochMs: 3265.678 });
    assert.equal(startSyncFromTargetMicros(null), null);
});

test('parsePort accepts valid numeric ports only', () => {
    assert.equal(parsePort('3031'), 3031);
    assert.equal(parsePort(' 65535 '), 65535);
    assert.equal(parsePort('0'), null);
    assert.equal(parsePort('65536'), null);
    assert.equal(parsePort('abc'), null);
    assert.equal(parsePort('3031x'), null);
});

test('unavailableMessageForPort names the missing server port', () => {
    assert.equal(unavailableMessageForPort(3031), 'No server on port 3031');
});
