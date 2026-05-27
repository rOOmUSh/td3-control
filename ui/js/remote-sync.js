import { api } from './api.js';

const ENABLED_KEY = 'td3_remote_sync_enabled';
const PORT_KEY = 'td3_remote_sync_port';

let setStatus = () => {};
let onCommand = async () => {};
let enabled = false;
let portText = '';
let togglePending = false;

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
    portText = localStorage.getItem(PORT_KEY) || '';
    portInput.value = portText;
    paint();

    toggle.addEventListener('click', async () => {
        if (togglePending) return;
        if (enabled) {
            setEnabled(false);
            setStatus('Remote sync OFF');
            return;
        }

        const port = currentPort();
        if (port === null) {
            setEnabled(false);
            setStatus('Remote sync port must be 1-65535');
            return;
        }

        togglePending = true;
        toggle.disabled = true;
        paint();
        try {
            await api.remoteSyncProbe({ port });
            setEnabled(true);
            setStatus(remoteLabel());
        } catch (_) {
            setEnabled(false);
            setStatus(unavailableMessageForPort(port));
        } finally {
            togglePending = false;
            toggle.disabled = false;
            paint();
        }
    });

    portInput.addEventListener('change', () => {
        portText = portInput.value.trim();
        localStorage.setItem(PORT_KEY, portText);
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

export function unavailableMessageForPort(port) {
    return `No server on port ${port}`;
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

export async function relayTriplet(triplet) {
    return relay({ command: 'triplet', triplet: !!triplet });
}

async function relay(payload) {
    if (!enabled) return { skipped: true };
    const port = currentPort();
    if (port === null) {
        throw new Error('Remote sync port must be 1-65535');
    }
    return api.remoteSyncRelay({ port, ...payload });
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
    const port = currentPort();
    return port === null ? 'Remote sync ON, set port' : `Remote sync ON: ${port}`;
}

function currentPort() {
    return parsePort(portInput ? portInput.value : portText);
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
    portInput.style.borderColor = portInput.value.trim() !== '' && parsePort(portInput.value) === null
        ? '#dc143c'
        : '';
}
