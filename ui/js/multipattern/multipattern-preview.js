// Per-card TD-3 pattern preview controller for the main Control page.
//
// Mirrors progression-main.js#handlePatternPreview: clicking PREVIEW on a card
// saves that pattern to the scratch slot and starts transport; clicking it
// again stops. Only one preview can run at a time - starting on card B while
// card A is previewing silently stops A first. The shared beat transport
// (transport.js play button) and per-card preview are mutually exclusive;
// starting either one stops the other.

import * as state from './multipattern-state.js';
import { api } from '../api.js';
import { highlightStep } from './multipattern-list.js';
import { delayToNextStep, nextStepInCycle } from '../shared/transport-sync-timing.js';
import { stepIntervalMs as timingStepIntervalMs } from '../shared/transport-timing.js';

let setStatus = () => {};
let scratchSlot = null;
let activeIdx = -1;
// Which playback path is active: 'device' (scratch upload + device clock) or
// 'audition' (host-sequenced, non-saving). Drives which stop endpoint stop()
// calls. null when idle.
let activeMode = null;
let beatTimer = null;
let currentStep = -1;
let localWrapCount = 0;
let wrapSync = { anchorEpochMs: 0, transportId: 0, wrapIndex: 0 };
let auditionUpdateInFlight = false;
let auditionUpdatePending = false;
const listeners = new Set();

function stepIntervalMs(idx) {
    const bpm = state.getBpm();
    const triplet = state.getTriplet(idx);
    return timingStepIntervalMs(bpm, triplet);
}

function startBeatTimer(idx, startSync) {
    stopBeatTimer();
    currentStep = 0;
    localWrapCount = 0;
    highlightStep(currentStep, idx);
    scheduleNextBeat(idx, delayToNextStep(startSync, stepIntervalMs(idx)));
    startWrapSync(idx, startSync);
}

function scheduleNextBeat(idx, delayMs) {
    if (activeIdx < 0) return;
    const delay = Number.isFinite(delayMs) && delayMs > 0 ? delayMs : stepIntervalMs(idx);
    beatTimer = setTimeout(() => {
        runBeatTimer();
        scheduleNextBeat(activeIdx);
    }, delay);
}

function runBeatTimer() {
    if (activeIdx < 0) return;
    const activeSteps = state.getActiveSteps(activeIdx) || 16;
    const next = nextStepInCycle(currentStep, activeSteps);
    currentStep = next.step;
    if (next.wrapped) localWrapCount += 1;
    highlightStep(currentStep, activeIdx);
}

function stopBeatTimer() {
    if (beatTimer) { clearTimeout(beatTimer); beatTimer = null; }
    currentStep = -1;
    localWrapCount = 0;
    highlightStep(-1);
    stopWrapSync();
}

function startWrapSync(idx, startSync) {
    stopWrapSync();
    if (!startSync || !Number.isFinite(startSync.transportId)
        || !Number.isFinite(startSync.startedAtEpochMs)) {
        return;
    }
    wrapSync = {
        anchorEpochMs: startSync.startedAtEpochMs,
        transportId: startSync.transportId,
        wrapIndex: 0,
    };
    pollWrapSync(idx);
}

function stopWrapSync() {
    wrapSync = { anchorEpochMs: 0, transportId: 0, wrapIndex: 0 };
}

async function pollWrapSync(idx) {
    if (activeIdx !== idx || !wrapSync.transportId) return;
    try {
        const pulse = await api.transportWrapPulse({
            transportId: wrapSync.transportId,
            anchorEpochMs: wrapSync.anchorEpochMs,
            wrapIndex: wrapSync.wrapIndex,
            activeSteps: state.getActiveSteps(idx) || 16,
            triplet: state.getTriplet(idx),
        });
        if (!pulse.ok) return;
        if (activeIdx !== idx || pulse.transportId !== wrapSync.transportId) return;
        applyWrapPulse(idx, pulse);
        wrapSync.anchorEpochMs = pulse.wrapEpochMs;
        wrapSync.wrapIndex = pulse.wrapIndex;
        pollWrapSync(idx);
    } catch (err) {
        if (activeIdx === idx && wrapSync.transportId) {
            setStatus('Preview wrap sync error: ' + err.message);
        }
    }
}

function applyWrapPulse(idx, pulse) {
    if (pulse.wrapIndex > localWrapCount) {
        localWrapCount = pulse.wrapIndex;
    }
    currentStep = 0;
    highlightStep(currentStep, idx);
    if (beatTimer) clearTimeout(beatTimer);
    scheduleNextBeat(idx);
}

/** Wire the controller. Call once from main.js after scratch + status are known. */
export function init(statusFn, scratch) {
    setStatus = typeof statusFn === 'function' ? statusFn : () => {};
    if (scratch) scratchSlot = scratch;
}

/** Current previewing card index, or -1 when idle. */
export function getActiveIdx() { return activeIdx; }
export function isActive(idx) { return activeIdx === idx; }
export function isAuditionActive() { return activeMode === 'audition' && activeIdx >= 0; }

export function subscribe(fn) { listeners.add(fn); return () => listeners.delete(fn); }
function notify() { for (const fn of listeners) { try { fn(activeIdx); } catch (_) {} } }

/**
 * Stop any active preview without starting a new one. Returns a promise that
 * resolves once the device transport is quiesced. Safe to call when idle.
 */
export async function stop() {
    if (activeIdx < 0) return;
    stopBeatTimer();
    const mode = activeMode;
    try {
        if (mode === 'audition') {
            await api.auditionStop();
        } else {
            await api.transportStop();
        }
    } catch (_) { /* ignore */ }
    activeIdx = -1;
    activeMode = null;
    auditionUpdatePending = false;
    auditionUpdateInFlight = false;
    notify();
}

export function syncActiveAudition() {
    if (activeMode !== 'audition' || activeIdx < 0 || !state.isConnected()) return;
    auditionUpdatePending = true;
    if (auditionUpdateInFlight) return;
    flushAuditionUpdate();
}

async function flushAuditionUpdate() {
    auditionUpdateInFlight = true;
    try {
        while (auditionUpdatePending) {
            auditionUpdatePending = false;
            if (activeMode !== 'audition' || activeIdx < 0 || !state.isConnected()) break;
            const pattern = state.getPattern(activeIdx);
            if (!pattern) break;
            await api.auditionUpdate(pattern, state.getBpm(), true);
        }
    } catch (err) {
        setStatus('Audition update error: ' + err.message);
    } finally {
        auditionUpdateInFlight = false;
        if (auditionUpdatePending && activeMode === 'audition' && activeIdx >= 0 && state.isConnected()) {
            flushAuditionUpdate();
        }
    }
}

/**
 * Toggle preview on `idx`: start if not active, stop if already active on
 * this card, otherwise stop the other active preview first and start here.
 * Refuses when the main transport is playing, or MIDI is disconnected.
 */
export async function toggle(idx) {
    if (!Number.isInteger(idx) || idx < 0) return;
    if (state.isPlaying()) {
        setStatus('Stop transport before auditioning patterns');
        return;
    }
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }
    const wasThis = activeIdx === idx;
    await stop();
    if (wasThis) {
        setStatus('Pattern preview stopped');
        return;
    }
    const pattern = state.getPattern(idx);
    if (!pattern) return;

    // NO SAVE (or live update off): audition from the host without writing
    // the pattern to the device or starting the device sequencer.
    if (state.isNoSave(idx)) {
        try {
            await api.auditionPattern(pattern, state.getBpm(), true);
            activeIdx = idx;
            activeMode = 'audition';
            auditionUpdatePending = false;
            auditionUpdateInFlight = false;
            // No device transport to sync against; the local beat timer
            // drives the step highlight only.
            startBeatTimer(idx, null);
            notify();
            setStatus(`Host audition: P${idx + 1} (no save)`);
        } catch (err) {
            setStatus('Audition error: ' + err.message);
        }
        return;
    }

    // Save path: upload to the scratch slot and start the device clock.
    if (!scratchSlot) {
        setStatus('Scratch slot not available');
        return;
    }
    try {
        await api.savePattern(
            scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pattern,
        );
        const startSync = await api.transportStart(state.getBpm());
        activeIdx = idx;
        activeMode = 'device';
        startBeatTimer(idx, startSync);
        notify();
        const label = scratchSlot.label || `G${scratchSlot.group}P${scratchSlot.pattern}${scratchSlot.side}`;
        setStatus(`TD-3 preview: P${idx + 1} → ${label}`);
    } catch (err) {
        setStatus('Preview error: ' + err.message);
    }
}
