// Progression transport - live cycling of patterns to the device during playback.
//
// When LIVE UPDATE is ON and playback starts:
//   1. The first pattern in the timeline is immediately saved to the device
//   2. The device loops that pattern internally
//   3. 14 steps before a pattern CHANGE, the next different pattern is saved
//   4. The device picks up the new pattern on its internal wrap-around
//   5. No saves happen when the same pattern repeats (e.g., P1,P1,P1,P1)
//   6. Timeline loops back to position 0 at the end

import * as state from './progression-state.js';
import { highlightStep } from './progression-sequencer.js';
import { morphRequestPercent } from '../shared/triplet-morph-timing.js';
import { highlightColumn } from './progression-timeline.js';
import { api } from '../api.js';
import { auditionLaneFields, effectiveLane, laneState } from '../shared/step-lanes.js';
import { envInt } from '../td3-env.js';
import { stepIntervalMs as timingStepIntervalMs } from '../shared/transport-timing.js';
import {
    cycleAnchorPulseIndex,
    delayToNextStep,
    elapsedStepBoundaries,
    phasePreservingStepDelay,
    preloadStep,
    pulsesPerStep,
    stepSyncFromCycle,
    stepSyncFromPulse,
    stepSyncFromTransportStart,
} from '../shared/transport-sync-timing.js';
import {
    clearBackgroundTimeout,
    setBackgroundTimeout,
} from '../shared/background-timer.js';

// 0-based step index where the pre-load SysEx fires. Sourced from env
// `PROGRESSION_NEXT_PATTERN_SAVE_STEP` (exposed on window.TD3_CONFIG_ENV
// as `progressionNextPatternSaveStep`) so main + progression share one
// ground truth. Clamped to [1, activeSteps-1]: 0 would collide with the
// wrap advance and values ≥ activeSteps could never fire within the
// cycle.
const ENV_PRELOAD_SAVE_STEP = envInt('progressionNextPatternSaveStep');

let setStatus = () => {};
let beatTimer = null;
let nextBeatEpochMs = 0;
let tempoSyncRequestId = 0;
let pendingDeviceTempoSync = null;
let deviceTempoSyncInFlight = false;
let latestTempoRevision = -1;
let devicePlayback = false;
let nextPatternSent = false;
let scratchSlot = { group: 1, pattern: 1, side: 'A' };
// When set, the next pattern-wrap in advanceBeat jumps to the first
// non-empty timeline position (i.e. P1 start) instead of advancing to
// findNextNonEmpty(pos). Set by queueRandomizeReset() so a randomize-
// during-playback visually waits for the device to finish its current
// pattern before switching - matching the fact that the device keeps
// looping its internal buffer until its own wrap before picking up the
// newly-saved pattern.
let pendingRandomizeReset = false;
let localWrapCount = 0;
let wrapSync = {
    anchorEpochMs: 0,
    anchorEpochMicros: 0,
    anchorPulseIndex: 0,
    transportId: 0,
    wrapIndex: 0,
};
let hostAuditionUpdatePendingIdx = null;
let hostAuditionUpdateInFlight = false;
/**
 * Set while a tempo change has stopped the step timer and an audition
 * acknowledgement is expected to restart it from the server's cycle.
 * Holds the step boundary and interval in force at the moment of the
 * pause so a fallback resume can keep the musical phase.
 */
let hostTempoPausePhase = null;

/**
 * Initialize with a status callback and scratch slot.
 * @param {function} statusFn
 * @param {{ group: number, pattern: number, side: string }} scratch
 */
export function init(statusFn, scratch) {
    setStatus = statusFn;
    if (scratch) scratchSlot = scratch;
}

/**
 * Start progression playback - begins the beat timer and sends the first pattern.
 */
export async function start(startSync, { resume = false } = {}) {
    tempoSyncRequestId += 1;
    pendingDeviceTempoSync = null;
    latestTempoRevision = -1;
    devicePlayback = !!(startSync && Number.isFinite(startSync.transportId));
    const tl = state.getTimeline();
    // Find first non-empty column
    let startPos = 0;
    while (startPos < tl.length && (tl[startPos] < 1 || tl[startPos] > 4)) {
        startPos++;
    }
    if (startPos >= tl.length) {
        setStatus('Timeline is empty - add patterns first');
        return;
    }

    state.setCurrentTimelinePos(startPos);
    nextPatternSent = false;

    const patIdx = tl[startPos] - 1; // 0-based
    state.setActivePatternIndex(patIdx);
    const activeSteps = state.getActiveSteps(patIdx);
    const triplet = state.getTriplet(patIdx);
    const playbackSync = resume && startSync && Number.isFinite(startSync.effectivePulseIndex)
        ? {
            ...startSync,
            anchorPulseIndex: cycleAnchorPulseIndex(
                startSync.effectivePulseIndex,
                activeSteps,
                triplet,
            ),
        }
        : startSync;

    // Send the first pattern to the device if live update is on
    if (devicePlayback && state.isConnected() && !resume) {
        try {
            await api.savePattern(
                scratchSlot.group, scratchSlot.pattern, scratchSlot.side,
                state.getPattern(patIdx)
            );
            await pushStepLane(patIdx);
            setStatus(`Loaded P${patIdx + 1} - playing loop 1/${countNonEmpty()}`);
        } catch (err) {
            setStatus('Live send error: ' + err.message);
        }
    }

    const projected = stepSyncFromTransportStart({
        startSync: playbackSync,
        bpm: state.getBpm(),
        activeSteps,
        triplet,
    });
    const startStep = projected ? projected.step : 0;
    state.setCurrentStepInPattern(startStep);
    highlightStep(patIdx, startStep);
    highlightColumn(startPos);

    if (projected) {
        scheduleBeatAt(
            projected.nextStepEpochMicros / 1000,
            projected.tickPeriodMicros * pulsesPerStep(triplet) / 1000,
        );
    } else {
        scheduleNextBeat(delayToNextStep(playbackSync, stepIntervalMs()));
    }
    startWrapSync(playbackSync);
}

/**
 * Stop progression playback - clears timers and highlights.
 */
/**
 * Hand the per-step cutoff lane of pattern `patIdx` to the clock thread
 * for device-sequenced playback. The lane always carries the pattern's
 * timing; its values go only while the lane is switched on.
 * `atCycleBoundary` defers the switch to the next wrap.
 */
export function pushStepLane(patIdx, { atCycleBoundary = false } = {}) {
    if (!state.isConnected()) return Promise.resolve();
    const pat = state.getPattern(patIdx);
    if (!pat) return Promise.resolve();
    const lanes = laneState(pat);
    return api.transportStepLane({
        cutoffs: lanes.cutoffOn ? effectiveLane(lanes, 'cutoff') : null,
        activeSteps: pat.active_steps,
        triplet: !!pat.triplet,
        midiChannel: state.getMidiChannel(),
        atCycleBoundary,
    }).catch((err) => setStatus('Step lane error: ' + err.message));
}

export function stop() {
    tempoSyncRequestId += 1;
    pendingDeviceTempoSync = null;
    latestTempoRevision = -1;
    devicePlayback = false;
    if (beatTimer) {
        clearBackgroundTimeout(beatTimer);
        beatTimer = null;
    }
    nextBeatEpochMs = 0;
    state.setCurrentStepInPattern(0);
    localWrapCount = 0;
    nextPatternSent = false;
    pendingRandomizeReset = false;
    clearHostAuditionUpdateQueue();
    highlightStep(-1, -1);
    highlightColumn(-1);
    stopWrapSync();
}

/**
 * Queue a timeline reset to take effect on the *next* pattern wrap, not
 * immediately. The caller (the randomize handler) has already written the
 * new P1 to the device scratch slot, but the device keeps looping its
 * current pattern until its own internal wrap - so the UI must also wait
 * for the wrap before jumping to P1 step 0, otherwise the highlight and
 * the audible pattern desync.
 *
 * We also flip `nextPatternSent=true` to suppress the pre-load block in
 * advanceBeat for the remainder of this cycle: the device already has
 * the correct "next" pattern (new P1) from the randomize handler's direct
 * save - a second pre-load would overwrite it with findNextNonEmpty's
 * pick from the updated timeline (typically new P2).
 */
export function queueRandomizeReset() {
    pendingRandomizeReset = true;
    nextPatternSent = true;
}

/**
 * Pure helper: return the timeline position the pattern wrap should
 * advance to. Exported so the behavior can be unit-tested without the
 * DOM / transport plumbing.
 *
 * @param {number[]} tl       timeline slots (values 1..4 mean P1..P4)
 * @param {number} currentPos current timeline position
 * @param {boolean} pendingReset when true, jump to first non-empty slot
 *   instead of walking forward from `currentPos`
 * @returns {number} next position, or -1 if the timeline is all-empty
 */
export function nextTimelinePosAfterWrap(tl, currentPos, pendingReset) {
    if (pendingReset) {
        for (let i = 0; i < tl.length; i += 1) {
            if (tl[i] >= 1 && tl[i] <= 4) return i;
        }
        return -1;
    }
    const len = tl.length;
    for (let i = 1; i <= len; i += 1) {
        const candidate = (currentPos + i) % len;
        if (tl[candidate] >= 1 && tl[candidate] <= 4) return candidate;
    }
    return -1;
}

export function shouldUpdateHostAuditionPattern(liveUpdate, connected, previousPatIdx, nextPatIdx) {
    return !liveUpdate
        && connected
        && Number.isInteger(previousPatIdx)
        && Number.isInteger(nextPatIdx)
        && nextPatIdx >= 0
        && nextPatIdx !== previousPatIdx;
}

/**
 * Reconcile a BPM change without discarding the current musical phase.
 */
export function restartTimer() {
    if (!state.isPlaying()) return;
    if (state.isConnected() && devicePlayback) {
        restartDeviceTempo();
        return;
    }
    restartHostTempo();
}

export function tempoStepReconciliation(currentStep, authoritativeStep, activeSteps) {
    if (authoritativeStep === currentStep) return 'same';
    if (Number.isInteger(currentStep)
        && Number.isInteger(authoritativeStep)
        && Number.isInteger(activeSteps)
        && currentStep >= 0
        && currentStep + 1 < activeSteps
        && authoritativeStep === currentStep + 1) {
        return 'advance';
    }
    return 'resync';
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

/** Calculate ms per step from BPM. Normal = 4 steps/beat, Triplet = 3 steps/beat. */
function stepIntervalMs() {
    const bpm = state.getBpm();
    const patIdx = state.getActivePatternIndex();
    return timingStepIntervalMs(bpm, state.getTriplet(patIdx));
}

function scheduleNextBeat(delayMs, intervalMs = stepIntervalMs()) {
    if (!state.isPlaying()) return;
    const interval = Number.isFinite(intervalMs) && intervalMs > 0
        ? intervalMs
        : stepIntervalMs();
    const delay = Number.isFinite(delayMs) && delayMs >= 0 ? delayMs : interval;
    scheduleBeatAt(Date.now() + delay, interval);
}

function scheduleBeatAt(deadlineEpochMs, intervalMs = stepIntervalMs()) {
    if (!state.isPlaying()) return;
    const nowMs = Date.now();
    const interval = Number.isFinite(intervalMs) && intervalMs > 0
        ? intervalMs
        : stepIntervalMs();
    nextBeatEpochMs = Number.isFinite(deadlineEpochMs)
        ? deadlineEpochMs
        : nowMs + interval;
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = setBackgroundTimeout(runBeatTimer, Math.max(0.01, nextBeatEpochMs - nowMs));
}

function runBeatTimer() {
    const boundaryEpochMs = nextBeatEpochMs || Date.now();
    const interval = stepIntervalMs();
    const elapsedBoundaries = elapsedStepBoundaries(boundaryEpochMs, interval);
    beatTimer = null;
    nextBeatEpochMs = 0;
    if (devicePlayback && state.isConnected() && elapsedBoundaries > 1) {
        restartDeviceTempo();
        return;
    }
    for (let i = 0; i < elapsedBoundaries; i += 1) advanceBeat();
    scheduleBeatAt(boundaryEpochMs + elapsedBoundaries * interval, interval);
}

function pauseBeatTimer() {
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = null;
}

function restartHostTempo() {
    if (!hostTempoPausePhase) {
        const previousIntervalMs = stepIntervalMs();
        hostTempoPausePhase = {
            previousIntervalMs,
            nextBoundaryEpochMs: nextBeatEpochMs || (Date.now() + previousIntervalMs),
        };
    }
    pauseBeatTimer();
}

/**
 * Resume local step timing if a tempo change stopped it and no
 * authoritative cycle ever arrived to re-anchor it.
 *
 * The audition keeps sounding whatever happens to the update request, so
 * every path that fails to reconcile - a rejected or dropped request, a
 * response that is not authoritative, a disconnect - used to leave the
 * timer stopped under audible playback and freeze the highlight until
 * playback was restarted. Falling back to local phase-preserving
 * scheduling keeps it advancing until a later update re-anchors it.
 */
export function resumeHostTempoIfStillPaused() {
    const phase = hostTempoPausePhase;
    if (!phase) return;
    hostTempoPausePhase = null;
    if (!state.isPlaying() || devicePlayback) return;
    const nextIntervalMs = stepIntervalMs();
    const delayMs = phasePreservingStepDelay(
        phase.previousIntervalMs,
        nextIntervalMs,
        phase.nextBoundaryEpochMs,
    );
    scheduleNextBeat(Math.max(0.01, delayMs), nextIntervalMs);
}

export function applyAuditionTiming(response) {
    const authoritative = response
        && Number.isFinite(response.centibpm)
        && response.centibpm === Math.round(state.getBpm() * 100)
        && Number.isFinite(response.cycleEpochMicros)
        && Number.isFinite(response.cyclePeriodMicros);
    if (!authoritative || !state.isPlaying() || devicePlayback) return false;

    const patIdx = state.getActivePatternIndex();
    const sync = stepSyncFromCycle({
        cycleEpochMicros: response.cycleEpochMicros,
        cyclePeriodMicros: response.cyclePeriodMicros,
        activeSteps: state.getActiveSteps(patIdx),
    });
    state.setCurrentStepInPattern(sync.step);
    highlightStep(patIdx, sync.step);
    hostTempoPausePhase = null;
    scheduleBeatAt(sync.nextStepEpochMicros / 1000, sync.stepPeriodMicros / 1000);
    return true;
}

function restartDeviceTempo() {
    const requestId = ++tempoSyncRequestId;
    pendingDeviceTempoSync = {
        requestId,
        bpm: state.getBpm(),
    };
    pauseBeatTimer();
    if (!deviceTempoSyncInFlight) flushDeviceTempoSync();
}

async function flushDeviceTempoSync() {
    deviceTempoSyncInFlight = true;
    try {
        while (pendingDeviceTempoSync) {
            const request = pendingDeviceTempoSync;
            pendingDeviceTempoSync = null;
            let response;
            try {
                response = await api.transportBpm(request.bpm);
            } catch (err) {
                if (!pendingDeviceTempoSync && request.requestId === tempoSyncRequestId) {
                    setStatus('Tempo sync error: ' + err.message);
                }
                continue;
            }
            if (pendingDeviceTempoSync || request.requestId !== tempoSyncRequestId) continue;
            applyDeviceTempoSync(request, response);
        }
    } finally {
        deviceTempoSyncInFlight = false;
        if (pendingDeviceTempoSync) flushDeviceTempoSync();
    }
}

function applyDeviceTempoSync(request, response) {
    if (request.requestId !== tempoSyncRequestId) return;
    if (!state.isPlaying()) return;
    if (!devicePlayback || !state.isConnected()) {
        restartHostTempo();
        return;
    }
    if (!response || !Number.isFinite(response.effectivePulseIndex)
        || !Number.isFinite(response.effectiveAtEpochMicros)
        || !Number.isFinite(response.centibpm)
        || !Number.isFinite(response.tempoRevision)
        || response.centibpm !== Math.round(request.bpm * 100)
        || response.tempoRevision < latestTempoRevision) {
        setStatus('Tempo sync error: device timing acknowledgement missing');
        return;
    }
    if (wrapSync.transportId && response.transportId !== wrapSync.transportId) {
        setStatus('Tempo sync error: stale transport acknowledgement');
        return;
    }
    latestTempoRevision = response.tempoRevision;

    const patIdx = state.getActivePatternIndex();
    const activeSteps = state.getActiveSteps(patIdx);
    const sync = stepSyncFromPulse({
        pulseIndex: response.effectivePulseIndex,
        pulseEpochMicros: response.effectiveAtEpochMicros,
        anchorPulseIndex: wrapSync.anchorPulseIndex,
        centibpm: response.centibpm,
        activeSteps,
        triplet: state.getTriplet(patIdx),
    });
    const currentStep = state.getCurrentStepInPattern();
    const reconciliation = tempoStepReconciliation(currentStep, sync.step, activeSteps);
    const isSameStep = reconciliation === 'same';
    const isNextNormalStep = reconciliation === 'advance';

    if (isNextNormalStep) advanceBeat();

    if (!isSameStep && !isNextNormalStep) {
        state.setCurrentStepInPattern(sync.step);
        highlightStep(state.getActivePatternIndex(), sync.step);
    }
    scheduleBeatAt(sync.nextStepEpochMicros / 1000, stepIntervalMs());
}

function scheduleFromLastWrap() {
    const patIdx = state.getActivePatternIndex();
    const sync = stepSyncFromPulse({
        pulseIndex: wrapSync.anchorPulseIndex,
        pulseEpochMicros: wrapSync.anchorEpochMicros,
        anchorPulseIndex: wrapSync.anchorPulseIndex,
        centibpm: Math.round(state.getBpm() * 100),
        activeSteps: state.getActiveSteps(patIdx),
        triplet: state.getTriplet(patIdx),
    });
    state.setCurrentStepInPattern(sync.step);
    highlightStep(patIdx, sync.step);
    scheduleBeatAt(sync.nextStepEpochMicros / 1000, stepIntervalMs());
}

/** Advance one step - the core of the live cycling logic. */
function advanceBeat() {
    const tl = state.getTimeline();
    let step = state.getCurrentStepInPattern() + 1;
    let pos = state.getCurrentTimelinePos();
    const patIdx = state.getActivePatternIndex();
    const activeSteps = state.getActiveSteps(patIdx);

    // --- Pre-load next pattern at env-configured save step ---
    // The device loops its internal pattern; we only need to save when
    // the pattern is about to change. Firing the save early (step 2 by
    // default) gives the SysEx plenty of travel time to reach the device
    // before its internal wrap - correctness of the pattern change
    // matters more than tight timing during a live jam.
    const preStep = preloadStep(activeSteps, ENV_PRELOAD_SAVE_STEP);
    if (step === preStep && !nextPatternSent) {
        nextPatternSent = true;
        const nextPos = findNextNonEmpty(tl, pos);
        if (nextPos >= 0 && devicePlayback && state.isConnected()) {
            const nextPatIdx = tl[nextPos] - 1;
            if (nextPatIdx !== patIdx) {
                api.savePattern(
                    scratchSlot.group, scratchSlot.pattern, scratchSlot.side,
                    state.getPattern(nextPatIdx)
                ).then(() => {
                    setStatus(`Pre-loaded P${nextPatIdx + 1}`);
                    return pushStepLane(nextPatIdx, { atCycleBoundary: true });
                }).catch(err => {
                    setStatus('Pre-load error: ' + err.message);
                });
            }
        }
    }

    // --- End of pattern: advance timeline position ---
    if (step >= activeSteps) {
        step = 0;
        handlePatternWrap(null);
    }

    state.setCurrentStepInPattern(step);

    // Highlight the current step on the active pattern row
    highlightStep(state.getActivePatternIndex(), step);
}

function handlePatternWrap(rustWrapIndex) {
    if (Number.isFinite(rustWrapIndex)) {
        localWrapCount = Math.max(localWrapCount, rustWrapIndex);
    } else {
        localWrapCount += 1;
    }
    const tl = state.getTimeline();
    const pos = state.getCurrentTimelinePos();
    const previousPatIdx = state.getActivePatternIndex();
    nextPatternSent = false;

    const wasPendingReset = pendingRandomizeReset;
    const nextPos = nextTimelinePosAfterWrap(tl, pos, pendingRandomizeReset);
    pendingRandomizeReset = false;

    if (nextPos < 0) return;
    const newPatIdx = tl[nextPos] - 1;
    state.setCurrentTimelinePos(nextPos);
    state.setActivePatternIndex(newPatIdx);
    highlightColumn(nextPos);
    if (shouldUpdateHostAuditionPattern(
        devicePlayback,
        state.isConnected(),
        previousPatIdx,
        newPatIdx,
    )) {
        scheduleHostAuditionUpdate(newPatIdx);
    }

    if (wasPendingReset) {
        setStatus(`Playing P${newPatIdx + 1} - regenerated`);
    } else {
        const loopNum = countLoopsUpTo(tl, nextPos) + 1;
        const totalLoops = countNonEmpty();
        setStatus(`Playing P${newPatIdx + 1} - loop ${loopNum}/${totalLoops}`);
    }
}

function clearHostAuditionUpdateQueue() {
    hostAuditionUpdatePendingIdx = null;
}

function scheduleHostAuditionUpdate(patIdx) {
    if (!state.isPlaying() || devicePlayback || !state.isConnected()) {
        // No update will be sent, so nothing will arrive to restart a
        // timer a tempo change stopped.
        resumeHostTempoIfStillPaused();
        return;
    }
    hostAuditionUpdatePendingIdx = patIdx;
    if (!hostAuditionUpdateInFlight) {
        flushHostAuditionUpdate();
    }
}

async function flushHostAuditionUpdate() {
    hostAuditionUpdateInFlight = true;
    try {
        while (hostAuditionUpdatePendingIdx !== null) {
            const patIdx = hostAuditionUpdatePendingIdx;
            hostAuditionUpdatePendingIdx = null;
            if (!state.isPlaying() || devicePlayback || !state.isConnected()) break;
            const pattern = state.getPattern(patIdx);
            if (!pattern) break;
            const morphPercent = morphRequestPercent(pattern, state.getTripletMorphPercent());
            const response = await api.auditionUpdate(
                pattern,
                state.getBpm(),
                true,
                null,
                state.getGatePercent(),
                morphPercent,
                state.getMidiChannel(),
                auditionLaneFields(pattern),
            );
            if (hostAuditionUpdatePendingIdx === null && !applyAuditionTiming(response)) {
                setStatus('Audition sync error: applied timing acknowledgement missing');
            }
        }
    } catch (err) {
        if (state.isPlaying() && !devicePlayback) {
            setStatus('Audition update error: ' + err.message);
        }
    } finally {
        hostAuditionUpdateInFlight = false;
        if (hostAuditionUpdatePendingIdx !== null
            && state.isPlaying()
            && !devicePlayback
            && state.isConnected()) {
            flushHostAuditionUpdate();
        } else {
            // Nothing further will reconcile: if the loop drained without
            // an authoritative acknowledgement the timer is still stopped.
            resumeHostTempoIfStillPaused();
        }
    }
}

function startWrapSync(startSync) {
    stopWrapSync();
    if (!startSync || !Number.isFinite(startSync.transportId)
        || !Number.isFinite(startSync.startedAtEpochMs)) {
        return;
    }
    wrapSync = {
        anchorEpochMs: startSync.startedAtEpochMs,
        anchorEpochMicros: Number.isFinite(startSync.effectiveAtEpochMicros)
            ? startSync.effectiveAtEpochMicros
            : startSync.startedAtEpochMs * 1000,
        anchorPulseIndex: Number.isFinite(startSync.anchorPulseIndex)
            ? startSync.anchorPulseIndex
            : Number.isFinite(startSync.effectivePulseIndex)
                ? startSync.effectivePulseIndex
                : 0,
        transportId: startSync.transportId,
        wrapIndex: 0,
    };
    pollWrapSync();
}

export function stopWrapSync() {
    wrapSync = {
        anchorEpochMs: 0,
        anchorEpochMicros: 0,
        anchorPulseIndex: 0,
        transportId: 0,
        wrapIndex: 0,
    };
}

async function pollWrapSync() {
    if (!state.isPlaying() || !wrapSync.transportId) return;
    const patIdx = state.getActivePatternIndex();
    const activeSteps = state.getActiveSteps(patIdx);
    const triplet = state.getTriplet(patIdx);
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
        if (!state.isPlaying() || pulse.transportId !== wrapSync.transportId) return;
        if (pulse.exactBoundary === false) {
            const recoveredAnchor = applyMissedWrapPulse(
                pulse,
                wrapSync.anchorPulseIndex,
                activeSteps,
                triplet,
            );
            if (!Number.isFinite(recoveredAnchor)) return;
            wrapSync.anchorEpochMs = pulse.wrapEpochMs;
            wrapSync.anchorEpochMicros = Number.isFinite(pulse.wrapEpochMicros)
                ? pulse.wrapEpochMicros
                : pulse.wrapEpochMs * 1000;
            wrapSync.anchorPulseIndex = recoveredAnchor;
            wrapSync.wrapIndex = pulse.wrapIndex;
            pollWrapSync();
            return;
        }
        wrapSync.anchorEpochMs = pulse.wrapEpochMs;
        wrapSync.anchorEpochMicros = Number.isFinite(pulse.wrapEpochMicros)
            ? pulse.wrapEpochMicros
            : pulse.wrapEpochMs * 1000;
        if (Number.isFinite(pulse.pulseIndex)) {
            wrapSync.anchorPulseIndex = pulse.pulseIndex;
        }
        wrapSync.wrapIndex = pulse.wrapIndex;
        applyWrapPulse(pulse);
        pollWrapSync();
    } catch (err) {
        setStatus('Wrap sync error: ' + err.message);
    }
}

function applyMissedWrapPulse(
    pulse,
    previousAnchorPulseIndex,
    activeSteps,
    triplet,
) {
    if (!Number.isFinite(pulse.pulseIndex)) {
        setStatus('Wrap sync error: recovery pulse missing');
        return null;
    }
    const pulseEpochMicros = Number.isFinite(pulse.wrapEpochMicros)
        ? pulse.wrapEpochMicros
        : pulse.wrapEpochMs * 1000;
    const requestedBoundaryPulseIndex = previousAnchorPulseIndex
        + activeSteps * pulsesPerStep(triplet);
    if (pulse.wrapIndex > localWrapCount) {
        handlePatternWrap(pulse.wrapIndex);
    }
    const recoveredPatIdx = state.getActivePatternIndex();
    const recoveredActiveSteps = state.getActiveSteps(recoveredPatIdx);
    const recoveredTriplet = state.getTriplet(recoveredPatIdx);
    const recoveredAnchor = cycleAnchorPulseIndex(
        pulse.pulseIndex,
        recoveredActiveSteps,
        recoveredTriplet,
        requestedBoundaryPulseIndex,
    );
    const sync = stepSyncFromPulse({
        pulseIndex: pulse.pulseIndex,
        pulseEpochMicros,
        anchorPulseIndex: recoveredAnchor,
        centibpm: Math.round(state.getBpm() * 100),
        activeSteps: recoveredActiveSteps,
        triplet: recoveredTriplet,
    });
    state.setCurrentStepInPattern(sync.step);
    highlightStep(recoveredPatIdx, sync.step);
    pauseBeatTimer();
    if (!deviceTempoSyncInFlight && !pendingDeviceTempoSync) {
        scheduleBeatAt(sync.nextStepEpochMicros / 1000, stepIntervalMs());
    }
    return recoveredAnchor;
}

function applyWrapPulse(pulse) {
    if (pulse.wrapIndex > localWrapCount) {
        handlePatternWrap(pulse.wrapIndex);
    }
    state.setCurrentStepInPattern(0);
    highlightStep(state.getActivePatternIndex(), 0);
    pauseBeatTimer();
    if (!deviceTempoSyncInFlight && !pendingDeviceTempoSync) scheduleFromLastWrap();
}

/**
 * Find the next non-empty timeline position after `pos`, wrapping around.
 * Returns -1 if the entire timeline is empty.
 */
function findNextNonEmpty(tl, pos) {
    const len = tl.length;
    for (let i = 1; i <= len; i++) {
        const candidate = (pos + i) % len;
        const val = tl[candidate];
        if (val >= 1 && val <= 4) return candidate;
    }
    return -1;
}

/** Count total non-empty positions in timeline. */
function countNonEmpty() {
    return state.getTimeline().filter(v => v >= 1 && v <= 4).length;
}

/** Count how many non-empty positions exist from 0..pos inclusive. */
function countLoopsUpTo(tl, pos) {
    let count = 0;
    for (let i = 0; i <= pos; i++) {
        if (tl[i] >= 1 && tl[i] <= 4) count++;
    }
    return count;
}
