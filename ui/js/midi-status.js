// MIDI connection status polling and connect/disconnect button.
//
// Shared between index.html and progression.html. The caller injects the
// page's state module (state.js or progression-state.js) via init(state,...);
// this module only needs the setConnected/isConnected surface, which both
// state modules expose identically.

import { api } from './api.js';
import * as deviceBackup from './device-backup.js';
import * as syncSource from './sync-source.js';
import { applyState } from './shared/class-state.js';

// HTML partials/transport-bar.html owns the structural classes on
// #btn-midi, its icon, and #midi-label. This module only swaps the
// state-dependent subset (border color, LED glow, foreground color)
// via applyState() - no more `el.className = "<full string>"` which
// would silently reset structural classes (e.g. sizing, rounded).

const MIDI_BTN_STATES = {
    green:  ['border-primary-fixed',      'led-glow-green'],
    yellow: ['border-yellow-500',         'led-glow-yellow'],
    grey:   ['border-secondary-container', 'led-glow-red'],
};

const MIDI_FG_STATES = {
    green:  ['text-primary-fixed'],
    yellow: ['text-yellow-500'],
    grey:   ['text-secondary-container'],
};

const btnMidi = document.getElementById('btn-midi');
const midiLabel = document.getElementById('midi-label');
const firmwareLabel = document.getElementById('firmware-label');

let state = null;
let pollTimer = null;
let setStatus = () => {};
let onConnect = null;
let modeSendDone = false;

/**
 * Initialize MIDI status polling and button.
 *
 * @param {object} stateModule - state.js or progression-state.js (must expose
 *                               setConnected(bool) and isConnected())
 * @param {Function} statusFn - status log callback
 * @param {Function|null} onConnectFn - optional callback fired on first connect
 * @param {object} [opts] - { autoConnect: boolean } - when false, do not try
 *                          to connect automatically at boot (driven by
 *                          UI_AUTO_CONNECT_TO_MIDI). Defaults to true so
 *                          callers that don't pass the flag keep prior
 *                          behavior.
 */
export function init(stateModule, statusFn, onConnectFn, opts) {
    state = stateModule;
    setStatus = statusFn;
    onConnect = onConnectFn || null;
    btnMidi.addEventListener('click', toggleConnection);
    syncSource.init(statusFn);
    const shouldAutoConnect = !opts || opts.autoConnect !== false;
    if (shouldAutoConnect) autoConnect();
    startPolling();
}

// Auto-connect on page load: check server, connect if needed, send mode pattern
async function autoConnect() {
    try {
        const res = await api.status();
        if (res.connected) {
            // Server already has a session (e.g. server auto-connected or page switch)
            state.setConnected(true);
            updateUI(res);
            // Ensure backup exists (skips if already stored)
            deviceBackup.ensureBackup(res.firmware || 'unknown');
            fireModeSend();
            return;
        }
        // Not connected - attempt auto-connect
        setStatus('Auto-connecting...');
        const conn = await api.connect();
        state.setConnected(true);
        const refreshed = await api.status().catch(() => null);
        updateUI(refreshed || { connected: true, sync_source: 'usb' });
        setStatus(`Connected: ${conn.product_name} v${conn.firmware}`);
        deviceBackup.ensureBackup(conn.firmware);
        fireModeSend();
    } catch (_) {
        // Auto-connect failed (no device plugged in, server down, etc.) - silent
        updateUI();
    }
}

function fireModeSend() {
    if (!modeSendDone && onConnect) {
        modeSendDone = true;
        onConnect();
    }
}

async function toggleConnection() {
    try {
        if (state.isConnected()) {
            await api.disconnect();
            state.setConnected(false);
            setStatus('MIDI disconnected');
            updateUI();
        } else {
            setStatus('Connecting...');
            const res = await api.connect();
            state.setConnected(true);
            const refreshed = await api.status().catch(() => null);
            updateUI(refreshed || { connected: true, sync_source: 'usb' });
            setStatus(`Connected: ${res.product_name} v${res.firmware}`);
            deviceBackup.ensureBackup(res.firmware);
            fireModeSend();
        }
    } catch (err) {
        setStatus('MIDI error: ' + err.message);
    }
}

function startPolling() {
    pollTimer = setInterval(async () => {
        try {
            const res = await api.status();
            const wasDisconnected = !state.isConnected();
            state.setConnected(res.connected);
            updateUI(res);
            if (res.connected && wasDisconnected) {
                fireModeSend();
            }
        } catch (_) {
            // Server unreachable - ignore
        }
    }, 3000);
}

function updateUI(statusRes) {
    const connected = state.isConnected();
    const source = statusRes && typeof statusRes.sync_source === 'string' ? statusRes.sync_source : null;

    let key;
    if (!connected) {
        key = 'grey';
    } else if (source === 'usb') {
        key = 'green';
    } else {
        key = 'yellow';
    }

    const icon = btnMidi.querySelector('.material-symbols-outlined');
    applyState(btnMidi, MIDI_BTN_STATES, key);
    applyState(icon, MIDI_FG_STATES, key);
    applyState(midiLabel, MIDI_FG_STATES, key);

    if (key === 'green') {
        icon.textContent = 'sync';
        midiLabel.textContent = 'CONNECTED';
        if (statusRes && statusRes.product_name) {
            firmwareLabel.textContent = `${statusRes.product_name} v${statusRes.firmware || '?'}`;
        }
    } else {
        icon.textContent = key === 'yellow' ? 'sync' : 'sync_problem';
        midiLabel.textContent = 'DISCONNECTED';
        if (key === 'yellow' && statusRes && statusRes.product_name) {
            firmwareLabel.textContent = `${statusRes.product_name} v${statusRes.firmware || '?'}`;
        } else {
            firmwareLabel.textContent = '';
        }
    }

    syncSource.updateFromStatus(statusRes || { connected, sync_source: source });
}

export function cleanup() {
    if (pollTimer) clearInterval(pollTimer);
}
