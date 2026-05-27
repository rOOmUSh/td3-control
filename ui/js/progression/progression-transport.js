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
import { highlightColumn } from './progression-timeline.js';
import { api } from '../api.js';
import { envInt } from '../td3-env.js';
import { stepIntervalMs as timingStepIntervalMs } from '../shared/transport-timing.js';
import {
    delayToNextStep,
    preloadStep,
} from '../shared/transport-sync-timing.js';

// 0-based step index where the pre-load SysEx fires. Sourced from env
// `PROGRESSION_NEXT_PATTERN_SAVE_STEP` (exposed on window.TD3_CONFIG_ENV
// as `progressionNextPatternSaveStep`) so main + progression share one
// ground truth. Clamped to [1, activeSteps-1]: 0 would collide with the
// wrap advance and values ≥ activeSteps could never fire within the
// cycle.
const ENV_PRELOAD_SAVE_STEP = envInt('progressionNextPatternSaveStep');

let setStatus = () => {};
let beatTimer = null;
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
    transportId: 0,
    wrapIndex: 0,
};
let hostAuditionUpdatePendingIdx = null;
let hostAuditionUpdateInFlight = false;

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
export async function start(startSync) {
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
    state.setCurrentStepInPattern(0);
    nextPatternSent = false;

    const patIdx = tl[startPos] - 1; // 0-based
    state.setActivePatternIndex(patIdx);

    // Send the first pattern to the device if live update is on
    if (state.isLiveUpdate() && state.isConnected()) {
        try {
            await api.savePattern(
                scratchSlot.group, scratchSlot.pattern, scratchSlot.side,
                state.getPattern(patIdx)
            );
            setStatus(`Loaded P${patIdx + 1} - playing loop 1/${countNonEmpty()}`);
        } catch (err) {
            setStatus('Live send error: ' + err.message);
        }
    }

    // Highlight
    highlightStep(patIdx, 0);
    highlightColumn(startPos);

    scheduleNextBeat(delayToNextStep(startSync, stepIntervalMs()));
    startWrapSync(startSync);
}

/**
 * Stop progression playback - clears timers and highlights.
 */
export function stop() {
    if (beatTimer) {
        clearTimeout(beatTimer);
        beatTimer = null;
    }
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
 * Restart the beat timer (call after BPM changes).
 * Also sends the new BPM to the device.
 */
export function restartTimer() {
    if (!state.isPlaying() || !beatTimer) return;
    clearTimeout(beatTimer);
    scheduleNextBeat();
    // Sync BPM to the device-clock path only.
    if (state.isConnected() && state.isLiveUpdate()) {
        api.transportBpm(state.getBpm()).catch(() => {});
    }
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

function scheduleNextBeat(delayMs) {
    if (!state.isPlaying()) return;
    const delay = Number.isFinite(delayMs) && delayMs > 0 ? delayMs : stepIntervalMs();
    beatTimer = setTimeout(runBeatTimer, delay);
}

function runBeatTimer() {
    beatTimer = null;
    advanceBeat();
    scheduleNextBeat();
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
        if (nextPos >= 0 && state.isLiveUpdate() && state.isConnected()) {
            const nextPatIdx = tl[nextPos] - 1;
            if (nextPatIdx !== patIdx) {
                api.savePattern(
                    scratchSlot.group, scratchSlot.pattern, scratchSlot.side,
                    state.getPattern(nextPatIdx)
                ).then(() => {
                    setStatus(`Pre-loaded P${nextPatIdx + 1}`);
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
        state.isLiveUpdate(),
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
            if (!state.isPlaying() || state.isLiveUpdate() || !state.isConnected()) break;
            const pattern = state.getPattern(patIdx);
            if (!pattern) break;
            await api.auditionUpdate(pattern, state.getBpm(), true);
        }
    } catch (err) {
        if (state.isPlaying() && !state.isLiveUpdate()) {
            setStatus('Audition update error: ' + err.message);
        }
    } finally {
        hostAuditionUpdateInFlight = false;
        if (hostAuditionUpdatePendingIdx !== null
            && state.isPlaying()
            && !state.isLiveUpdate()
            && state.isConnected()) {
            flushHostAuditionUpdate();
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
        transportId: startSync.transportId,
        wrapIndex: 0,
    };
    pollWrapSync();
}

export function stopWrapSync() {
    wrapSync = { anchorEpochMs: 0, transportId: 0, wrapIndex: 0 };
}

async function pollWrapSync() {
    if (!state.isPlaying() || !wrapSync.transportId) return;
    const patIdx = state.getActivePatternIndex();
    try {
        const pulse = await api.transportWrapPulse({
            transportId: wrapSync.transportId,
            anchorEpochMs: wrapSync.anchorEpochMs,
            wrapIndex: wrapSync.wrapIndex,
            activeSteps: state.getActiveSteps(patIdx),
            triplet: state.getTriplet(patIdx),
        });
        if (!pulse.ok) return;
        if (!state.isPlaying() || pulse.transportId !== wrapSync.transportId) return;
        applyWrapPulse(pulse);
        wrapSync.anchorEpochMs = pulse.wrapEpochMs;
        wrapSync.wrapIndex = pulse.wrapIndex;
        pollWrapSync();
    } catch (err) {
        setStatus('Wrap sync error: ' + err.message);
    }
}

function applyWrapPulse(pulse) {
    if (pulse.wrapIndex > localWrapCount) {
        handlePatternWrap(pulse.wrapIndex);
    }
    state.setCurrentStepInPattern(0);
    highlightStep(state.getActivePatternIndex(), 0);
    if (beatTimer) clearTimeout(beatTimer);
    scheduleNextBeat();
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
