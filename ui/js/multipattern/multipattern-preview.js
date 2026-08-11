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
import {
    cycleAnchorPulseIndex,
    delayToNextStep,
    elapsedStepBoundaries,
    nextStepInCycle,
    phasePreservingStepDelay,
    pulsesPerStep,
    stepSyncFromCycle,
    stepSyncFromPulse,
    stepSyncFromTransportStart,
} from '../shared/transport-sync-timing.js';
import { stepIntervalMs as timingStepIntervalMs } from '../shared/transport-timing.js';
import {
    clearBackgroundTimeout,
    setBackgroundTimeout,
} from '../shared/background-timer.js';
import { morphRequestPercent } from '../shared/triplet-morph-timing.js';

let setStatus = () => {};
let scratchSlot = null;
let activeIdx = -1;
// Which playback path is active: 'device' (scratch upload + device clock) or
// 'audition' (host-sequenced, non-saving). Drives which stop endpoint stop()
// calls. null when idle.
let activeMode = null;
let beatTimer = null;
let nextBeatEpochMs = 0;
let scheduledStepIntervalMs = 0;
let bpmPausePhase = null;
let pendingDeviceBpmSync = null;
let deviceBpmSyncInFlight = false;
let bpmSyncGeneration = 0;
let latestTempoRevision = -1;
let currentStep = -1;
let localWrapCount = 0;
let wrapSync = {
    anchorEpochMs: 0,
    anchorPulseIndex: 0,
    transportId: 0,
    wrapIndex: 0,
};
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
    localWrapCount = 0;
    const triplet = state.getTriplet(idx);
    const projected = stepSyncFromTransportStart({
        startSync,
        bpm: state.getBpm(),
        activeSteps: state.getActiveSteps(idx) || 16,
        triplet,
    });
    currentStep = projected ? projected.step : 0;
    highlightStep(currentStep, idx);
    if (projected) {
        scheduleBeatAt(
            idx,
            projected.nextStepEpochMicros / 1000,
            projected.tickPeriodMicros * pulsesPerStep(triplet) / 1000,
        );
    } else {
        scheduleNextBeat(idx, delayToNextStep(startSync, stepIntervalMs(idx)));
    }
    startWrapSync(idx, startSync);
}

function scheduleNextBeat(idx, delayMs) {
    if (activeIdx < 0) return;
    const interval = stepIntervalMs(idx);
    const delay = Number.isFinite(delayMs) && delayMs >= 0 ? delayMs : interval;
    scheduleBeatAt(idx, Date.now() + delay, interval);
}

function scheduleBeatAt(idx, deadlineEpochMs, intervalMs = stepIntervalMs(idx)) {
    if (activeIdx < 0) return;
    const nowMs = Date.now();
    const deadline = Number.isFinite(deadlineEpochMs)
        ? deadlineEpochMs
        : nowMs + intervalMs;
    const interval = Number.isFinite(intervalMs) && intervalMs > 0
        ? intervalMs
        : stepIntervalMs(idx);
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    scheduledStepIntervalMs = interval;
    nextBeatEpochMs = deadline;
    beatTimer = setBackgroundTimeout(() => {
        const boundaryEpochMs = nextBeatEpochMs || Date.now();
        const currentInterval = stepIntervalMs(activeIdx);
        const elapsedBoundaries = elapsedStepBoundaries(
            boundaryEpochMs,
            currentInterval,
        );
        beatTimer = null;
        nextBeatEpochMs = 0;
        if (activeMode === 'device' && elapsedBoundaries > 1) {
            queueDeviceBpmSync();
            return;
        }
        for (let i = 0; i < elapsedBoundaries; i += 1) runBeatTimer();
        scheduleBeatAt(
            activeIdx,
            boundaryEpochMs + elapsedBoundaries * currentInterval,
            currentInterval,
        );
    }, Math.max(0.01, deadline - nowMs));
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
    if (beatTimer) { clearBackgroundTimeout(beatTimer); beatTimer = null; }
    nextBeatEpochMs = 0;
    scheduledStepIntervalMs = 0;
    bpmPausePhase = null;
    pendingDeviceBpmSync = null;
    bpmSyncGeneration += 1;
    latestTempoRevision = -1;
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
        anchorPulseIndex: Number.isFinite(startSync.effectivePulseIndex)
            ? startSync.effectivePulseIndex
            : 0,
        transportId: startSync.transportId,
        wrapIndex: 0,
    };
    pollWrapSync(idx);
}

function stopWrapSync() {
    wrapSync = { anchorEpochMs: 0, anchorPulseIndex: 0, transportId: 0, wrapIndex: 0 };
}

async function pollWrapSync(idx) {
    if (activeIdx !== idx || !wrapSync.transportId) return;
    const activeSteps = state.getActiveSteps(idx) || 16;
    const triplet = state.getTriplet(idx);
    try {
        const pulse = await api.transportWrapPulse({
            transportId: wrapSync.transportId,
            anchorEpochMs: wrapSync.anchorEpochMs,
            anchorPulseIndex: wrapSync.anchorPulseIndex,
            wrapIndex: wrapSync.wrapIndex,
            activeSteps,
            triplet,
        });
        if (!pulse.ok) return;
        if (activeIdx !== idx || pulse.transportId !== wrapSync.transportId) return;
        if (pulse.exactBoundary === false) {
            const recoveredAnchor = applyMissedWrapPulse(
                idx,
                pulse,
                wrapSync.anchorPulseIndex,
                activeSteps,
                triplet,
            );
            if (!Number.isFinite(recoveredAnchor)) return;
            wrapSync.anchorEpochMs = pulse.wrapEpochMs;
            wrapSync.anchorPulseIndex = recoveredAnchor;
            wrapSync.wrapIndex = pulse.wrapIndex;
            pollWrapSync(idx);
            return;
        }
        applyWrapPulse(idx, pulse);
        wrapSync.anchorEpochMs = pulse.wrapEpochMs;
        if (Number.isFinite(pulse.pulseIndex)) {
            wrapSync.anchorPulseIndex = pulse.pulseIndex;
        }
        wrapSync.wrapIndex = pulse.wrapIndex;
        pollWrapSync(idx);
    } catch (err) {
        if (activeIdx === idx && wrapSync.transportId) {
            setStatus('Preview wrap sync error: ' + err.message);
        }
    }
}

function applyMissedWrapPulse(
    idx,
    pulse,
    previousAnchorPulseIndex,
    activeSteps,
    triplet,
) {
    if (!Number.isFinite(pulse.pulseIndex)) {
        setStatus('Preview wrap sync error: recovery pulse missing');
        return null;
    }
    const pulseEpochMicros = Number.isFinite(pulse.wrapEpochMicros)
        ? pulse.wrapEpochMicros
        : pulse.wrapEpochMs * 1000;
    const recoveredAnchor = cycleAnchorPulseIndex(
        pulse.pulseIndex,
        activeSteps,
        triplet,
        previousAnchorPulseIndex,
    );
    const sync = stepSyncFromPulse({
        pulseIndex: pulse.pulseIndex,
        pulseEpochMicros,
        anchorPulseIndex: recoveredAnchor,
        centibpm: Math.round(state.getBpm() * 100),
        activeSteps,
        triplet,
    });
    localWrapCount = Math.max(localWrapCount, pulse.wrapIndex);
    currentStep = sync.step;
    highlightStep(currentStep, idx);
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = null;
    nextBeatEpochMs = 0;
    if (!deviceBpmSyncInFlight && !pendingDeviceBpmSync) {
        bpmPausePhase = null;
        scheduleBeatAt(idx, sync.nextStepEpochMicros / 1000);
    }
    return recoveredAnchor;
}

function applyWrapPulse(idx, pulse) {
    if (pulse.wrapIndex > localWrapCount) {
        localWrapCount = pulse.wrapIndex;
    }
    const activeSteps = state.getActiveSteps(idx) || 16;
    const pulseIndex = Number.isFinite(pulse.pulseIndex)
        ? pulse.pulseIndex
        : wrapSync.anchorPulseIndex;
    const pulseEpochMicros = Number.isFinite(pulse.wrapEpochMicros)
        ? pulse.wrapEpochMicros
        : pulse.wrapEpochMs * 1000;
    const sync = stepSyncFromPulse({
        pulseIndex,
        pulseEpochMicros,
        anchorPulseIndex: pulseIndex,
        centibpm: Math.round(state.getBpm() * 100),
        activeSteps,
        triplet: state.getTriplet(idx),
    });
    currentStep = sync.step;
    highlightStep(currentStep, idx);
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = null;
    nextBeatEpochMs = 0;
    if (deviceBpmSyncInFlight || pendingDeviceBpmSync) return;
    bpmPausePhase = null;
    scheduleBeatAt(idx, sync.nextStepEpochMicros / 1000);
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
    if (activeMode !== 'audition' || activeIdx < 0 || !state.isConnected()) {
        // No update will be sent, so nothing will arrive to restart a
        // timer a BPM change paused.
        resumeBeatTimerIfStillPaused();
        return;
    }
    auditionUpdatePending = true;
    if (auditionUpdateInFlight) return;
    flushAuditionUpdate();
}

/** Keep an active per-card preview aligned after the shared BPM changes. */
export function syncBpmChange(previousBpm) {
    if (activeIdx < 0) return;
    pauseBeatTimerForBpm(previousBpm);
    if (activeMode === 'audition') {
        syncActiveAudition();
    } else if (activeMode === 'device' && state.isConnected()) {
        queueDeviceBpmSync();
    } else {
        resumeBeatTimerPreservingPhase();
    }
}

function pauseBeatTimerForBpm(previousBpm) {
    if (bpmPausePhase || activeIdx < 0) return;
    const previousIntervalMs = scheduledStepIntervalMs
        || timingStepIntervalMs(previousBpm, state.getTriplet(activeIdx));
    bpmPausePhase = {
        previousIntervalMs,
        nextBoundaryEpochMs: nextBeatEpochMs || (Date.now() + previousIntervalMs),
    };
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = null;
    nextBeatEpochMs = 0;
}

/**
 * Resume local step timing if a BPM change paused it and no authoritative
 * cycle ever arrived to re-anchor it.
 *
 * A BPM change pauses the beat timer and leaves the audition
 * acknowledgement to restart it from the server's real cycle epoch. The
 * preview keeps sounding whatever happens to that request, so a failed,
 * superseded or non-authoritative update used to leave the timer stopped
 * under audible playback and freeze the row highlight. Local
 * phase-preserving scheduling keeps it advancing until a later update
 * re-anchors it.
 */
function resumeBeatTimerIfStillPaused() {
    if (!bpmPausePhase) return;
    resumeBeatTimerPreservingPhase();
}

function resumeBeatTimerPreservingPhase() {
    if (activeIdx < 0) {
        bpmPausePhase = null;
        return;
    }
    const phase = bpmPausePhase;
    const nextIntervalMs = stepIntervalMs(activeIdx);
    bpmPausePhase = null;
    const delayMs = phase
        ? phasePreservingStepDelay(
            phase.previousIntervalMs,
            nextIntervalMs,
            phase.nextBoundaryEpochMs,
        )
        : nextIntervalMs;
    scheduleNextBeat(activeIdx, Math.max(0.01, delayMs));
}

function queueDeviceBpmSync() {
    bpmSyncGeneration += 1;
    pendingDeviceBpmSync = {
        generation: bpmSyncGeneration,
        bpm: state.getBpm(),
        idx: activeIdx,
    };
    if (!deviceBpmSyncInFlight) flushDeviceBpmSync();
}

async function flushDeviceBpmSync() {
    deviceBpmSyncInFlight = true;
    try {
        while (pendingDeviceBpmSync) {
            const request = pendingDeviceBpmSync;
            pendingDeviceBpmSync = null;
            let response;
            try {
                response = await api.transportBpm(request.bpm);
            } catch (err) {
                if (!pendingDeviceBpmSync && request.generation === bpmSyncGeneration) {
                    setStatus('Preview BPM error: ' + err.message);
                }
                continue;
            }

            if (pendingDeviceBpmSync || request.generation !== bpmSyncGeneration) continue;
            if (activeMode !== 'device' || activeIdx !== request.idx) {
                bpmPausePhase = null;
                continue;
            }
            reconcileDeviceBpm(response, request);
        }
    } finally {
        deviceBpmSyncInFlight = false;
        if (pendingDeviceBpmSync) flushDeviceBpmSync();
    }
}

function reconcileDeviceBpm(response, request) {
    const expectedCentibpm = Math.round(request.bpm * 100);
    const authoritative = response
        && Number.isFinite(response.transportId)
        && Number.isFinite(response.centibpm)
        && Number.isFinite(response.tempoRevision)
        && Number.isFinite(response.effectivePulseIndex)
        && Number.isFinite(response.effectiveAtEpochMicros)
        && response.centibpm === expectedCentibpm
        && (!wrapSync.transportId || response.transportId === wrapSync.transportId)
        && response.tempoRevision >= latestTempoRevision;

    if (!authoritative) {
        setStatus('Preview BPM sync error: device timing acknowledgement missing');
        return;
    }

    latestTempoRevision = response.tempoRevision;
    const activeSteps = state.getActiveSteps(request.idx) || 16;
    const sync = stepSyncFromPulse({
        pulseIndex: response.effectivePulseIndex,
        pulseEpochMicros: response.effectiveAtEpochMicros,
        anchorPulseIndex: wrapSync.anchorPulseIndex,
        centibpm: response.centibpm,
        activeSteps,
        triplet: state.getTriplet(request.idx),
    });
    const forward = currentStep < 0
        ? activeSteps
        : (sync.step - currentStep + activeSteps) % activeSteps;
    if (forward === 1) {
        runBeatTimer();
    } else if (currentStep !== sync.step) {
        currentStep = sync.step;
        highlightStep(currentStep, request.idx);
    }

    bpmPausePhase = null;
    scheduleBeatAt(request.idx, sync.nextStepEpochMicros / 1000);
}

async function flushAuditionUpdate() {
    auditionUpdateInFlight = true;
    try {
        while (auditionUpdatePending) {
            auditionUpdatePending = false;
            if (activeMode !== 'audition' || activeIdx < 0 || !state.isConnected()) break;
            const pattern = state.getPattern(activeIdx);
            if (!pattern) break;
            const response = await api.auditionUpdate(
                pattern,
                state.getBpm(),
                true,
                null,
                state.getGatePercent(),
                morphRequestPercent(pattern, state.getTripletMorphPercent()),
                state.getMidiChannel(),
            );
            if (!auditionUpdatePending) reconcileAuditionTiming(response);
        }
    } catch (err) {
        setStatus('Audition update error: ' + err.message);
    } finally {
        auditionUpdateInFlight = false;
        if (auditionUpdatePending && activeMode === 'audition' && activeIdx >= 0 && state.isConnected()) {
            flushAuditionUpdate();
        } else {
            // Nothing further will reconcile: if the loop drained without
            // an authoritative acknowledgement the timer is still paused.
            resumeBeatTimerIfStillPaused();
        }
    }
}

function reconcileAuditionTiming(response) {
    const authoritative = response
        && Number.isFinite(response.centibpm)
        && response.centibpm === Math.round(state.getBpm() * 100)
        && Number.isFinite(response.cycleEpochMicros)
        && Number.isFinite(response.cyclePeriodMicros);
    if (!authoritative) {
        if (bpmPausePhase) {
            setStatus('Preview audition sync error: applied timing acknowledgement missing');
        }
        return;
    }
    if (activeMode !== 'audition' || activeIdx < 0) return;

    const sync = stepSyncFromCycle({
        cycleEpochMicros: response.cycleEpochMicros,
        cyclePeriodMicros: response.cyclePeriodMicros,
        activeSteps: state.getActiveSteps(activeIdx) || 16,
    });
    currentStep = sync.step;
    highlightStep(currentStep, activeIdx);
    bpmPausePhase = null;
    scheduleBeatAt(
        activeIdx,
        sync.nextStepEpochMicros / 1000,
        sync.stepPeriodMicros / 1000,
    );
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
            await api.auditionPattern(
                pattern,
                state.getBpm(),
                true,
                null,
                state.getGatePercent(),
                morphRequestPercent(pattern, state.getTripletMorphPercent()),
                state.getMidiChannel(),
            );
            activeIdx = idx;
            activeMode = 'audition';
            auditionUpdatePending = false;
            auditionUpdateInFlight = false;
            // No device transport to sync against; the local beat timer
            // drives the step highlight only.
            notify();
            startBeatTimer(idx, null);
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
        notify();
        startBeatTimer(idx, startSync);
        const label = scratchSlot.label || `G${scratchSlot.group}P${scratchSlot.pattern}${scratchSlot.side}`;
        setStatus(`TD-3 preview: P${idx + 1} → ${label}`);
    } catch (err) {
        setStatus('Preview error: ' + err.message);
    }
}
