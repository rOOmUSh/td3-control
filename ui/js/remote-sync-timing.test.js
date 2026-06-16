import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    cycleDurationMicros,
    eventEpochMicros,
    startSyncFromTargetMicros,
    targetEpochMicrosForPlay,
} from './remote-sync-timing.js';
import {
    buildRelayRequest,
    formatRemoteSyncFailure,
    migrateRemoteSyncPortsStorage,
    parsePort,
    parsePorts,
    unavailableMessageForPort,
} from './remote-sync.js';

function storageStub(entries = {}) {
    const data = new Map(Object.entries(entries));
    return {
        getItem: key => data.has(key) ? data.get(key) : null,
        setItem: (key, value) => data.set(key, String(value)),
        dump: () => Object.fromEntries(data.entries()),
    };
}

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

test('parsePorts accepts comma and whitespace separated ports', () => {
    assert.deepEqual(parsePorts('3031,3032'), [3031, 3032]);
    assert.deepEqual(parsePorts('3031 3032'), [3031, 3032]);
    assert.deepEqual(parsePorts(' 3031, 3032 3033 '), [3031, 3032, 3033]);
});

test('parsePorts rejects invalid tokens and self port', () => {
    assert.equal(parsePorts('3031x,3032'), null);
    assert.equal(parsePorts('0,3032'), null);
    assert.equal(parsePorts('65536'), null);
    assert.equal(parsePorts('3030,3031', 3030), null);
});

test('parsePorts deduplicates in first occurrence order', () => {
    assert.deepEqual(parsePorts('3031,3032,3031,3033'), [3031, 3032, 3033]);
});

test('parsePorts rejects more than eight normalized ports', () => {
    assert.equal(parsePorts('1,2,3,4,5,6,7,8,9'), null);
});

test('migrateRemoteSyncPortsStorage seeds new storage from old valid port', () => {
    const storage = storageStub({ td3_remote_sync_port: '3031' });
    assert.equal(migrateRemoteSyncPortsStorage(storage), '3031');
    assert.equal(storage.dump().td3_remote_sync_ports, '3031');
});

test('migrateRemoteSyncPortsStorage keeps existing multi-port value', () => {
    const storage = storageStub({
        td3_remote_sync_port: '3031',
        td3_remote_sync_ports: '3032,3033',
    });
    assert.equal(migrateRemoteSyncPortsStorage(storage), '3032,3033');
    assert.equal(storage.dump().td3_remote_sync_ports, '3032,3033');
});

test('buildRelayRequest sends normalized ports array', () => {
    assert.deepEqual(
        buildRelayRequest({ command: 'play', centibpm: 12500 }, [3031, 3032]),
        { command: 'play', centibpm: 12500, ports: [3031, 3032] },
    );
});

test('formatRemoteSyncFailure names failed ports', () => {
    assert.equal(
        formatRemoteSyncFailure({
            ok: false,
            queued: false,
            results: [
                { port: 3031, ok: true, queued: true },
                { port: 3032, ok: false, queued: false, error: 'No server on port 3032' },
            ],
        }, 'Remote sync relay failed'),
        'Remote sync relay failed: port 3032: No server on port 3032',
    );
});

test('unavailableMessageForPort names the missing server port', () => {
    assert.equal(unavailableMessageForPort(3031), 'No server on port 3031');
});
