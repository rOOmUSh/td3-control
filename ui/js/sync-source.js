// Sync-source pill column. Reads the device clock source from
// /api/status and posts /api/midi/sync-source on click.

import { api } from './api.js';

const ALL_SOURCES = ['int', 'usb', 'din', 'trig'];

let pillButtons = [];
let setStatus = () => {};
let inFlight = false;
let lastKnown = null;

/**
 * Wire pill click handlers.
 * @param {Function} statusFn - status log callback
 */
export function init(statusFn) {
    setStatus = statusFn || (() => {});
    pillButtons = Array.from(document.querySelectorAll('#sync-pills .sync-pill'));
    for (const btn of pillButtons) {
        btn.addEventListener('click', () => onPillClick(btn));
    }
}

/**
 * Reflect the latest /api/status payload in the pill column.
 * Called by midi-status.js on every poll tick and on the connect path.
 */
export function updateFromStatus(statusRes) {
    const connected = !!(statusRes && statusRes.connected);
    const source = statusRes && typeof statusRes.sync_source === 'string' ? statusRes.sync_source : null;

    if (connected && source && ALL_SOURCES.includes(source)) {
        lastKnown = source;
    }

    const enabled = connected && !inFlight;
    const activeKey = connected && source ? source : 'usb';

    for (const btn of pillButtons) {
        const key = btn.dataset.sync;
        btn.disabled = !enabled;
        btn.classList.toggle('sync-pill--active', key === activeKey);
    }
}

async function onPillClick(btn) {
    if (inFlight) return;
    const target = btn.dataset.sync;
    if (!ALL_SOURCES.includes(target)) return;
    if (btn.classList.contains('sync-pill--active')) return;

    inFlight = true;
    setLoadingState(true);
    try {
        const res = await api.setSyncSource(target);
        const newSource = res && res.source ? res.source : target;
        lastKnown = newSource;
        applyActive(newSource);
        setStatus(`Sync source: ${newSource.toUpperCase()}`);
    } catch (err) {
        setStatus('Sync error: ' + err.message);
        applyActive(lastKnown || 'usb');
    } finally {
        inFlight = false;
        setLoadingState(false);
    }
}

function applyActive(source) {
    for (const btn of pillButtons) {
        btn.classList.toggle('sync-pill--active', btn.dataset.sync === source);
    }
}

function setLoadingState(busy) {
    for (const btn of pillButtons) {
        btn.disabled = busy ? true : btn.disabled;
    }
}
