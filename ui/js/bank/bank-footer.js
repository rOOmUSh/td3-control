// Bank UI footer: simplified copy of the main Control page's transport bar.
// Carries only the MIDI connection LED/toggle and the global BPM knob -
// the "play/stop" transport button from the main app is deliberately
// omitted, because every pattern surface in the Bank (cards, table,
// drawer, snapshot grid, duplicate chips, related tiles, imported-entry
// rows) already has its own per-item play button via bank-play.js.
// A global transport Play would be ambiguous here: "play which pattern?"
//
// Connection state is polled every 3 seconds (mirrors midi-status.js).
// BPM is owned by bank-play.js so the per-item buttons and the knob share
// the same value; the knob repaints via subscribeBpm() when anything
// else changes it (e.g. a future preset or slider).

import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { getBankBpm, setBankBpm, subscribeBpm } from './bank-play.js';
import { applyState } from '../shared/class-state.js';

// HTML bank.html owns the structural classes on #btn-midi, its icon,
// and #midi-label. This module only swaps the state-dependent subset
// (border color, LED glow, foreground color) via applyState() - no
// more full-className rewrites.

const MIDI_BTN_STATES = {
    connected:    ['border-primary-fixed', 'led-glow-green'],
    disconnected: ['border-secondary-container', 'led-glow-red'],
};

const MIDI_FG_STATES = {
    connected:    ['text-primary-fixed'],
    disconnected: ['text-secondary-container'],
};

const POLL_MS = 3000;

let _connected = false;
let _pollTimer = null;

export function initFooter() {
    const btnMidi = document.getElementById('btn-midi');
    const midiLabel = document.getElementById('midi-label');
    const bpmDisplay = document.getElementById('bpm-display');
    const bpmKnob = document.getElementById('bpm-knob');
    const knobIndicator = document.getElementById('knob-indicator');

    if (!btnMidi || !midiLabel || !bpmDisplay || !bpmKnob || !knobIndicator) {
        console.warn('bank-footer: footer nodes missing, skipping init');
        return;
    }

    wireMidi(btnMidi, midiLabel);
    wireBpm(bpmDisplay, bpmKnob, knobIndicator);
}

function wireMidi(btnMidi, midiLabel) {
    paintConnection(btnMidi, midiLabel, false);

    btnMidi.addEventListener('click', async () => {
        btnMidi.disabled = true;
        try {
            if (_connected) {
                await bankApi.midiDisconnect();
                _connected = false;
                paintConnection(btnMidi, midiLabel, false);
                toast('MIDI disconnected', 'info');
            } else {
                const res = await bankApi.midiConnect();
                _connected = true;
                paintConnection(btnMidi, midiLabel, true);
                const label = res.product_name
                    ? `Connected: ${res.product_name} v${res.firmware || '?'}`
                    : 'MIDI connected';
                toast(label, 'success');
            }
        } catch (e) {
            toast(`MIDI: ${e.message}`, 'error');
        } finally {
            btnMidi.disabled = false;
        }
    });

    const refresh = async () => {
        try {
            const res = await bankApi.midiStatus();
            const next = !!res.connected;
            if (next !== _connected) {
                _connected = next;
                paintConnection(btnMidi, midiLabel, _connected);
            }
        } catch { /* server unreachable - keep last known state */ }
    };
    void refresh();
    _pollTimer = setInterval(refresh, POLL_MS);
}

function wireBpm(bpmDisplay, bpmKnob, knobIndicator) {
    paintBpm(bpmDisplay, knobIndicator, getBankBpm());
    subscribeBpm((bpm) => paintBpm(bpmDisplay, knobIndicator, bpm));

    bpmKnob.addEventListener('wheel', (ev) => {
        ev.preventDefault();
        const delta = ev.deltaY < 0 ? 1 : -1;
        void setBankBpm(getBankBpm() + delta);
    }, { passive: false });

    let dragging = false;
    let dragStartY = 0;
    let dragStartBpm = 0;
    bpmKnob.addEventListener('mousedown', (ev) => {
        dragging = true;
        dragStartY = ev.clientY;
        dragStartBpm = getBankBpm();
        ev.preventDefault();
    });
    document.addEventListener('mousemove', (ev) => {
        if (!dragging) return;
        const delta = Math.round((dragStartY - ev.clientY) / 3);
        void setBankBpm(dragStartBpm + delta);
    });
    document.addEventListener('mouseup', () => {
        if (dragging) dragging = false;
    });
}

function paintConnection(btn, label, isConnected) {
    const icon = btn.querySelector('.material-symbols-outlined');
    const key = isConnected ? 'connected' : 'disconnected';

    applyState(btn, MIDI_BTN_STATES, key);
    applyState(icon, MIDI_FG_STATES, key);
    applyState(label, MIDI_FG_STATES, key);

    if (isConnected) {
        icon.textContent = 'sync';
        label.textContent = 'CONNECTED';
        btn.title = 'Disconnect from TD-3';
    } else {
        icon.textContent = 'sync_problem';
        label.textContent = 'DISCONNECTED';
        btn.title = 'Connect to TD-3';
    }
}

function paintBpm(display, indicator, bpm) {
    display.textContent = bpm;
    // Map 20-300 BPM → -150° to +150° knob rotation (same curve as transport.js).
    const angle = ((bpm - 20) / 280) * 300 - 150;
    indicator.style.transform = `rotate(${angle}deg)`;
}

export function teardownFooter() {
    if (_pollTimer) { clearInterval(_pollTimer); _pollTimer = null; }
}
