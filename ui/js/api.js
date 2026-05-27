// Fetch wrappers for the TD-3 backend API.

const BASE = '/api';

async function request(method, path, body, signal) {
    const opts = { method, headers: { 'Content-Type': 'application/json' } };
    if (body !== undefined) opts.body = JSON.stringify(body);
    if (signal) opts.signal = signal;
    const res = await fetch(BASE + path, opts);
    const json = await res.json();
    if (!res.ok) throw new Error(json.error || `HTTP ${res.status}`);
    return json;
}

export const api = {
    status:     ()      => request('GET', '/status'),
    ports:      ()      => request('GET', '/ports'),
    connect:    (body)  => request('POST', '/midi/connect', body || {}),
    disconnect: ()      => request('POST', '/midi/disconnect', {}),
    setSyncSource: (source) => request('POST', '/midi/sync-source', { source }),
    loadPattern:(g,p,s) => request('POST', '/pattern/load', { patgroup: g, pattern: p, side: s }),
    savePattern:(g,p,s,data) => request('POST', '/pattern/save', { patgroup: g, pattern: p, side: s, data }),
    // payload: either { content: string, format } for text formats
    // (toml/json/steps/pat) or { bytes: number[], format } for binary
    // formats (seq/mid). Byte arrays go through JSON as plain arrays - no
    // base64 dep on either side; files are small (≤ few KB).
    importPattern:(payload) => request('POST', '/pattern/import', payload),
    // Parse a raw .sqs/.rbs byte array into a 64-slot preview grid. Each
    // non-empty slot carries its WebPattern inline so the picker can show a
    // preview, audition via playPatternPreview, and commit locally without a
    // second round-trip.
    parsePatternBank:(bytes, format) => request('POST', '/pattern/parse-bank', { bytes, format }),
    // Transient audition for a pattern that isn't backed by a library item
    // (e.g. one picked from an imported sqs/rbs bank). Uploads to the
    // scratch slot and starts the clock, mirroring bank play-item. Stop is
    // the regular /transport/stop.
    playPatternPreview:(pattern, bpm) => request(
        'POST',
        '/pattern/play-preview',
        bpm != null ? { pattern, centibpm: Math.round(bpm * 100) } : { pattern },
    ),
    // Host-sequenced, non-saving audition: plays the pattern as timed Note
    // On/Off from the host with no MIDI Start and no scratch-slot write, so
    // the device sequencer stays idle and device memory is untouched. Stop
    // is /pattern/audition/stop. `looping` defaults to true server-side.
    auditionPattern:(pattern, bpm, looping = true, targetEpochMicros = null) => {
        const body = bpm != null
            ? { pattern, centibpm: Math.round(bpm * 100), looping }
            : { pattern, looping };
        if (targetEpochMicros != null) body.targetEpochMicros = targetEpochMicros;
        return request('POST', '/pattern/audition', body);
    },
    auditionUpdate:(pattern, bpm, looping = true) => request(
        'POST',
        '/pattern/audition/update',
        bpm != null
            ? { pattern, centibpm: Math.round(bpm * 100), looping }
            : { pattern, looping },
    ),
    auditionStop: () => request('POST', '/pattern/audition/stop', {}),
    exportPool:(patterns) => request('POST', '/pattern/export-pool', { patterns }),
    exportPattern: async (pattern, format, extra = {}) => {
        const res = await fetch(BASE + '/pattern/export', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ pattern, format, ...extra }),
        });
        if (!res.ok) {
            let msg = `HTTP ${res.status}`;
            try { const j = await res.json(); if (j && j.error) msg = j.error; } catch (_) {}
            throw new Error(msg);
        }
        return await res.blob();
    },
    transportStart:(bpm, targetEpochMicros = null)=> {
        const body = { centibpm: Math.round(bpm * 100) };
        if (targetEpochMicros != null) body.targetEpochMicros = targetEpochMicros;
        return request('POST', '/transport/start', body);
    },
    transportStop: ()   => request('POST', '/transport/stop', {}),
    transportBpm:  (bpm)=> request('POST', '/transport/bpm', { centibpm: Math.round(bpm * 100) }),
    transportWrapPulse: (body, signal) => request('POST', '/transport/wrap-pulse', body, signal),
    notePreview:   (note, transpose, accent) => request('POST', '/note/preview', { note, transpose, accent }),
    getKeyboardConfig: () => request('GET', '/config/keyboard'),
    saveKeyboardConfig: (config) => request('POST', '/config/keyboard', config),
    getScalesConfig: () => request('GET', '/config/scales'),
    saveScalesConfig: (config) => request('POST', '/config/scales', config),
    getProgressionConfig: () => request('GET', '/config/progression'),
    saveProgressionConfig: (config) => request('POST', '/config/progression', config),
    getHarmonyConfig: async () => {
        const res = await fetch('/config/harmony-config.json');
        if (!res.ok) throw new Error(`HTTP ${res.status} loading harmony-config.json`);
        return res.json();
    },
    getScratchPattern: () => request('GET', '/scratch-pattern'),
    getEnvConfig: () => request('GET', '/config/env'),
    exportProgressionPackage: (payload) => request('POST', '/progression/export-package', payload),
    appendControlQueue: (patterns) => request('POST', '/control/queue/append', { patterns }),
    consumeControlQueue: () => request('GET', '/control/queue/consume'),
    remoteSyncRelay: (body) => request('POST', '/remote-sync/relay', body),
    remoteSyncProbe: (body) => request('POST', '/remote-sync/probe', body),
    remoteSyncPoll: () => request('GET', '/remote-sync/poll'),
};
