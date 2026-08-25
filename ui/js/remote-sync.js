import { api } from './api.js';

const ENABLED_KEY = 'td3_remote_sync_enabled';
const PORT_KEY = 'td3_remote_sync_port';
const PORTS_KEY = 'td3_remote_sync_ports';
const MAX_REMOTE_PORTS = 8;

let setStatus = () => {};
let onCommand = async () => {};
let enabled = false;
let portText = '';
let togglePending = false;
let lastProbe = null;

const doc = typeof document !== 'undefined' ? document : null;
const controls = doc ? doc.getElementById('remote-sync-controls') : null;
const toggle = doc ? doc.getElementById('remote-sync-toggle') : null;
const portInput = doc ? doc.getElementById('remote-sync-port') : null;

export function init(options = {}) {
    setStatus = options.setStatus || setStatus;
    onCommand = options.onCommand || onCommand;

    if (!controls || !toggle || !portInput) return;
    controls.classList.remove('hidden');
    controls.classList.add('flex');

    enabled = localStorage.getItem(ENABLED_KEY) === '1';
    portText = migrateRemoteSyncPortsStorage(localStorage);
    portInput.value = portText;
    paint();

    toggle.addEventListener('click', async () => {
        if (togglePending) return;
        if (enabled) {
            setEnabled(false);
            setStatus('Remote sync OFF');
            return;
        }

        const ports = currentPorts();
        if (ports === null) {
            setEnabled(false);
            setStatus(invalidPortsMessage());
            return;
        }

        togglePending = true;
        toggle.disabled = true;
        paint();
        try {
            const response = await api.remoteSyncProbe({ ports });
            if (!response || !response.ok) {
                throw new Error(formatRemoteSyncFailure(response, 'Remote sync unavailable'));
            }
            setEnabled(true);
            lastProbe = response;
            setStatus(formatProbeRoundTrip(response) || remoteLabel());
        } catch (err) {
            setEnabled(false);
            setStatus(err && err.message ? err.message : unavailableMessageForPorts(ports));
        } finally {
            togglePending = false;
            toggle.disabled = false;
            paint();
        }
    });

    portInput.addEventListener('change', () => {
        portText = portInput.value.trim();
        localStorage.setItem(PORTS_KEY, portText);
        paint();
    });

    pollLoop();
}

export function isEnabled() {
    return enabled;
}

export function parsePort(value) {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    if (!/^[0-9]+$/.test(trimmed)) return null;
    const port = Number(trimmed);
    if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
    return port;
}

export function parsePorts(value, currentBrowserPort = null) {
    if (typeof value !== 'string') return null;
    const tokens = value.trim().split(/[,\s]+/).filter(token => token !== '');
    if (tokens.length === 0) return null;
    const selfPort = Number.isInteger(currentBrowserPort) ? currentBrowserPort : null;
    const ports = [];
    for (const token of tokens) {
        const port = parsePort(token);
        if (port === null) return null;
        if (selfPort !== null && port === selfPort) return null;
        if (!ports.includes(port)) {
            ports.push(port);
            if (ports.length > MAX_REMOTE_PORTS) return null;
        }
    }
    return ports;
}

export function migrateRemoteSyncPortsStorage(storage) {
    const storedPorts = (storage.getItem(PORTS_KEY) || '').trim();
    if (storedPorts !== '') return storedPorts;

    const legacyPort = (storage.getItem(PORT_KEY) || '').trim();
    const migrated = parsePorts(legacyPort);
    if (migrated === null) return '';

    const text = formatPorts(migrated);
    storage.setItem(PORTS_KEY, text);
    return text;
}

export function buildRelayRequest(payload, ports) {
    return { ...payload, ports: ports.slice() };
}

export function unavailableMessageForPort(port) {
    return `No server on port ${port}`;
}

export function unavailableMessageForPorts(ports) {
    if (!Array.isArray(ports) || ports.length === 0) return 'No remote sync servers configured';
    if (ports.length === 1) return unavailableMessageForPort(ports[0]);
    return `No server on one or more remote ports: ${formatPorts(ports)}`;
}

export function formatRemoteSyncFailure(response, fallback = 'Remote sync failed') {
    const results = response && Array.isArray(response.results) ? response.results : [];
    const failed = results.filter(result => !result.ok || result.queued === false);
    if (failed.length === 0) return fallback;
    return `${fallback}: ${failed.map(formatFailedResult).join('; ')}`;
}

/**
 * The round trip each peer answered the probe in, as a status line.
 *
 * The figure is the server to server hop measured on the master, which is the
 * same path a relayed command takes, so it is the latency a start delay has
 * to cover. Returns an empty string when no result carried a measurement, so
 * a caller can fall back to its own message.
 */
export function formatProbeRoundTrip(response) {
    const results = response && Array.isArray(response.results) ? response.results : [];
    const timed = results.filter(
        (result) => result && result.ok && Number.isFinite(result.rttMicros),
    );
    if (timed.length === 0) return '';
    const parts = timed.map(
        (result) => `${result.port} ${formatMicros(result.rttMicros)}`,
    );
    const worst = Math.max(...timed.map((result) => result.rttMicros));
    if (timed.length === 1) return `Remote sync ON: ${parts[0]} round trip`;
    return `Remote sync ON: ${parts.join(', ')} round trip, worst ${formatMicros(worst)}`;
}

/** Microseconds as a compact millisecond reading. */
export function formatMicros(micros) {
    if (!Number.isFinite(micros)) return '?';
    const millis = micros / 1000;
    if (millis < 10) return `${millis.toFixed(2)} ms`;
    if (millis < 100) return `${millis.toFixed(1)} ms`;
    return `${Math.round(millis)} ms`;
}

/** The most recent probe response, or null before any probe. */
export function lastProbeResponse() {
    return lastProbe;
}

export function formatRemoteSyncSuccess(label, response) {
    const results = response && Array.isArray(response.results) ? response.results : [];
    const ports = results
        .filter(result => result.ok && result.queued !== false)
        .map(result => result.port)
        .filter(port => Number.isInteger(port));
    if (ports.length === 0) return `${label} queued`;
    return `${label} queued: ${formatPorts(ports)}`;
}

export async function relayPlay({ centibpm, targetEpochMicros }) {
    return relay({
        command: 'play',
        centibpm,
        targetEpochMicros,
    });
}

export async function relayStop() {
    return relay({ command: 'stop' });
}

export async function relayBpm(centibpm) {
    return relay({ command: 'bpm', centibpm });
}

/**
 * Relay a TRIPLET press. `morph` says whether the sender wrote the 12-step
 * projection or only set the device's triplet flag: the two produce
 * different patterns, so a peer given the flag alone plays something else.
 */
export async function relayTriplet(triplet, morph) {
    return relay({ command: 'triplet', triplet: !!triplet, morph: !!morph });
}

async function relay(payload) {
    if (!enabled) return { skipped: true };
    const ports = currentPorts();
    if (ports === null) {
        throw new Error(invalidPortsMessage());
    }
    const response = await api.remoteSyncRelay(buildRelayRequest(payload, ports));
    if (!response || !response.ok || !response.queued) {
        throw new Error(formatRemoteSyncFailure(response, 'Remote sync relay failed'));
    }
    return response;
}

async function pollLoop() {
    for (;;) {
        try {
            const res = await api.remoteSyncPoll();
            if (res && res.command) {
                await onCommand(res.command);
            }
        } catch (err) {
            await delay(1000);
        }
    }
}

function delay(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

function remoteLabel() {
    const ports = currentPorts();
    if (ports === null) return 'Remote sync ON, set ports';
    if (ports.length === 1) return `Remote sync ON: ${ports[0]}`;
    return `Remote sync ON: ${ports.length} ports ${formatPorts(ports)}`;
}

function currentPorts() {
    return parsePorts(portInput ? portInput.value : portText, currentBrowserPort());
}

function setEnabled(value) {
    enabled = !!value;
    localStorage.setItem(ENABLED_KEY, enabled ? '1' : '0');
    paint();
}

function paint() {
    if (!toggle || !portInput) return;
    toggle.textContent = enabled ? 'ON' : 'OFF';
    toggle.setAttribute('aria-pressed', enabled ? 'true' : 'false');
    toggle.classList.toggle('is-active', enabled);
    toggle.classList.toggle('sync-pill--active', enabled);
    portInput.style.borderColor = portInput.value.trim() !== ''
        && parsePorts(portInput.value, currentBrowserPort()) === null
        ? '#dc143c'
        : '';
}

function formatPorts(ports) {
    return ports.join(',');
}

function formatFailedResult(result) {
    if (Number.isInteger(result.port)) {
        return result.error ? `port ${result.port}: ${result.error}` : `port ${result.port} failed`;
    }
    return result.error || 'remote failed';
}

function invalidPortsMessage() {
    return `Remote sync ports must be 1-65535, max ${MAX_REMOTE_PORTS}, excluding this UI`;
}

function currentBrowserPort() {
    const loc = typeof window !== 'undefined' ? window.location : null;
    if (!loc) return null;
    if (loc.port) return parsePort(loc.port);
    if (loc.protocol === 'http:') return 80;
    if (loc.protocol === 'https:') return 443;
    return null;
}
