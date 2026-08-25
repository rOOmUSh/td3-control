import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    cycleDurationMicros,
    eventEpochMicros,
    startSyncFromTargetMicros,
} from './remote-sync-timing.js';
import {
    buildRelayRequest,
    formatMicros,
    formatProbeRoundTrip,
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

test('formatMicros reads as milliseconds at a sensible precision', () => {
    assert.equal(formatMicros(1234), '1.23 ms');
    assert.equal(formatMicros(45678), '45.7 ms');
    assert.equal(formatMicros(456789), '457 ms');
    assert.equal(formatMicros(undefined), '?');
});

test('formatProbeRoundTrip reports the round trip of one peer', () => {
    assert.equal(
        formatProbeRoundTrip({ ok: true, results: [{ port: 3031, ok: true, rttMicros: 1500 }] }),
        'Remote sync ON: 3031 1.50 ms round trip',
    );
});

test('formatProbeRoundTrip names the worst peer when there are several', () => {
    const line = formatProbeRoundTrip({
        ok: true,
        results: [
            { port: 3031, ok: true, rttMicros: 1500 },
            { port: 3032, ok: true, rttMicros: 4200 },
        ],
    });
    assert.match(line, /3031 1\.50 ms/);
    assert.match(line, /3032 4\.20 ms/);
    assert.match(line, /worst 4\.20 ms/);
});

// A failed probe's duration is its timeout, which says nothing about the
// link, so it must never be reported as a round trip.
test('formatProbeRoundTrip ignores peers that did not answer', () => {
    const line = formatProbeRoundTrip({
        ok: false,
        results: [
            { port: 3031, ok: true, rttMicros: 900 },
            { port: 3032, ok: false, error: 'No server on port 3032' },
        ],
    });
    assert.match(line, /3031/);
    assert.doesNotMatch(line, /3032/);
});

test('formatProbeRoundTrip yields nothing when no peer was timed', () => {
    assert.equal(formatProbeRoundTrip({ ok: true, results: [] }), '');
    assert.equal(formatProbeRoundTrip(null), '');
    assert.equal(formatProbeRoundTrip({ ok: true, results: [{ port: 3031, ok: true }] }), '');
});
