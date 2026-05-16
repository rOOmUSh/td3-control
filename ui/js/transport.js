// Main-page transport - BPM knob, play toggle, and the timeline-aware beat
// loop that cycles patterns through the device scratch slot.
//
// Dual-tracker playback model (per the GROUND TRUTH RULE): the TD-3 always
// plays its in-flight buffer until wrap, even after a new SysEx save. So:
//
//   currentDevicePatternIdx  - what the device is audibly looping right
//                              now. Updates at wrap (device swaps in the
//                              scratch contents). Drives step highlighting.
//   queuedPatternIdx         - what the scratch slot currently holds.
//                              Updates on every savePattern call. Becomes
//                              currentDevicePatternIdx at the next wrap.
//
// Timeline cursor (currentTlPos) is used for pre-load math ("what slot
// comes next after the currently-playing one?") and for the column
// highlight in the timeline modal - but NOT for step highlighting. Step
// highlighting follows currentDevicePatternIdx so it tracks what the
// device is audibly playing, not the cursor that may have already advanced
// past it on a checkbox-driven timeline rearrangement.
//
// Playback contract:
//   1. Start finds the first non-empty timeline slot and saves that pattern
//      to the scratch (LIVE UPDATE ON). Seeds both trackers to that pattern.
//   2. At the configured pre-load step we save the next-in-timeline pattern
//      to scratch, unless it matches the current one. queuedPatternIdx
//      updates to reflect the save.
//   3. At the active-steps wrap: currentDevicePatternIdx adopts
//      queuedPatternIdx (device just swapped buffers), and the cursor is
//      re-synced via advanceCursorToDevicePattern - in the uninterrupted
//      case this picks the same slot nextTimelinePos would; in the
//      interrupted case (a checkbox override queued a different pattern)
//      it finds the matching slot anywhere in the new timeline.
//   4. Mid-play structural changes (checkbox toggles, drag-to-reorder,
//      timeline edits) fire through onStateChangeDuringPlay:
//        • 0→1 checked-mode transition ("first checkbox"): immediately
//          save the first slot of the new checked timeline to scratch so
//          the device swaps to it at the next wrap; suppress further
//          pre-loads this cycle.
//        • Any other structural change: just clamp the cursor so pre-load
//          math and column highlight stay valid.

import * as state from './multipattern/multipattern-state.js';
import { api } from './api.js';
import { highlightStep } from './multipattern/multipattern-list.js';
import { highlightColumn } from './multipattern/multipattern-timeline.js';
import * as preview from './multipattern/multipattern-preview.js';
import { envInt } from './td3-env.js';
import { stepIntervalMs as timingStepIntervalMs } from './shared/transport-timing.js';
import {
    delayToNextStep,
    preloadStep,
} from './shared/transport-sync-timing.js';
import {
    firstTimelinePos,
    nextTimelinePos,
    advanceCursorToDevicePattern,
    countNonEmpty,
    needsImmediateScratchSave,
} from './multipattern/multipattern-transport-helpers.js';
import { bindPointerPressActivation } from './shared/pointer-activation.js';

const btnPlay = document.getElementById('btn-play');
const bpmDisplay = document.getElementById('bpm-display');
const bpmKnob = document.getElementById('bpm-knob');
const knobIndicator = document.getElementById('knob-indicator');
const bpmFineToggle = document.getElementById('bpm-fine-toggle');

let bpmFineMode = false;

// Pre-load save step. Env `PROGRESSION_NEXT_PATTERN_SAVE_STEP` (exposed
// as `progressionNextPatternSaveStep` on the inlined env snapshot) is the
// 0-based step where the pre-load SysEx fires. At 16-step patterns the
// template ships `2`, giving ~14 steps of MIDI travel time before the
// device wraps - which avoids the "first pattern plays twice" desync.
const ENV_PRELOAD_SAVE_STEP = envInt('progressionNextPatternSaveStep');

let setStatus = () => {};
let beatTimer = null;

// Beat-loop cursor state.
let currentStep = -1;
let localWrapCount = 0;
let currentTlPos = -1;            // -1 means single-pattern loop mode
let nextPatternSent = false;      // guards pre-load from double-firing
let scratchSlot = { group: 1, pattern: 1, side: 'A' };

// Dual-tracker model - see module header.
let currentDevicePatternIdx = null;   // what device is audibly looping
let queuedPatternIdx = null;          // what's in scratch right now

// Checked-mode transition detector. True = checked mode was active on the
// last structural change. Flipping false→true is the "first checkbox"
// event that triggers an immediate scratch save.
let prevCheckedMode = false;

// Signature of the active timeline last seen by onStateChangeDuringPlay.
// Same-signature notifications (focus change, viewport edit, unrelated
// step edit) become cheap no-ops - except for an explicit first-checkbox
// transition, which always runs.
let lastSeenTl = null;
let wrapSync = {
    anchorEpochMs: 0,
    transportId: 0,
    wrapIndex: 0,
};

// ---------------------------------------------------------------------------
// Init / status
// ---------------------------------------------------------------------------

export function init(statusFn, scratch) {
    setStatus = statusFn;
    if (scratch) scratchSlot = scratch;
    updateBpmDisplay();
    updatePlayButton();

    if (state.isPlaying()) {
        startBeatTimer();
        setStatus('Playing at ' + state.getBpm() + ' BPM');
    }

    // Seed transition detector so rejoining a playing session doesn't
    // misfire a spurious "first checkbox" on the very first structural
    // notification.
    prevCheckedMode = state.isCheckedMode();

    state.onChange(onStateChangeDuringPlay);

    bindPointerPressActivation(btnPlay, togglePlay);

    // BPM knob: scroll wheel. Coarse = ±1 BPM, fine = ±0.01 BPM.
    bpmKnob.addEventListener('wheel', (e) => {
        e.preventDefault();
        const step = bpmFineMode ? 0.01 : 1;
        const delta = e.deltaY < 0 ? step : -step;
        state.setBpm(state.getBpm() + delta);
        updateBpmDisplay();
        if (state.isPlaying()) restartBeatTimer();
        if (state.isPlaying() && state.isConnected()) {
            api.transportBpm(state.getBpm()).catch(err => setStatus('BPM error: ' + err.message));
        }
    });

    // BPM fine-mode toggle. On: display shows two decimals, wheel
    // scrolls 0.01 BPM steps. Off: display drops to integer; any
    // fractional component is truncated toward the integer (140.37 →
    // 140), per the spec, so coarse mode is always clean.
    if (bpmFineToggle) {
        bpmFineToggle.addEventListener('click', () => {
            bpmFineMode = !bpmFineMode;
            bpmFineToggle.classList.toggle('sync-pill--active', bpmFineMode);
            bpmFineToggle.setAttribute('aria-pressed', bpmFineMode ? 'true' : 'false');
            if (!bpmFineMode) {
                state.setBpm(Math.trunc(state.getBpm()));
            }
            updateBpmDisplay();
            if (state.isPlaying() && state.isConnected()) {
                api.transportBpm(state.getBpm()).catch(err => setStatus('BPM error: ' + err.message));
            }
        });
    }

    // BPM knob: click-and-drag (3 px per BPM).
    let dragging = false;
    let dragStartY = 0;
    let dragStartBpm = 0;
    bpmKnob.addEventListener('mousedown', (e) => {
        dragging = true;
        dragStartY = e.clientY;
        dragStartBpm = state.getBpm();
        e.preventDefault();
    });
    document.addEventListener('mousemove', (e) => {
        if (!dragging) return;
        const delta = Math.round((dragStartY - e.clientY) / 3);
        state.setBpm(dragStartBpm + delta);
        updateBpmDisplay();
    });
    document.addEventListener('mouseup', () => {
        if (!dragging) return;
        dragging = false;
        if (state.isPlaying()) restartBeatTimer();
        if (state.isPlaying() && state.isConnected()) {
            api.transportBpm(state.getBpm()).catch(err => setStatus('BPM error: ' + err.message));
        }
    });
}

// ---------------------------------------------------------------------------
// Play toggle
// ---------------------------------------------------------------------------

async function togglePlay() {
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }
    try {
        if (state.isPlaying()) {
            stopWrapSync();
            await api.transportStop();
            state.setPlaying(false);
            stopBeatTimer();
            setStatus('Stopped');
        } else {
            // Stop any active per-card preview first - they share the
            // device transport, so starting main play over a preview would
            // double-send transportStart and leave the preview button lit.
            await preview.stop();
            await startTimelinePlayback();
            primeFirstStepHighlight();
            const startSync = await api.transportStart(state.getBpm());
            state.setPlaying(true);
            startBeatTimer(startSync);
            startWrapSync(startSync);
        }
        updatePlayButton();
    } catch (err) {
        setStatus('Transport error: ' + err.message);
    }
}

/**
 * Prime the timeline cursor + send the first pattern to the scratch slot.
 * Called once on play-start. When the timeline is empty or single-slot we
 * fall back to focused-pattern loop mode: currentTlPos stays -1, advanceBeat
 * will leave it alone.
 */
async function startTimelinePlayback() {
    const tl = state.getTimeline();
    const start = firstTimelinePos(tl);
    const activeTimelineSlots = countNonEmpty(tl);
    lastSeenTl = timelineSignature(tl);
    prevCheckedMode = state.isCheckedMode();

    // Single-slot (or empty) timeline → legacy single-pattern loop.
    if (activeTimelineSlots <= 1) {
        currentTlPos = -1;
        currentStep = -1;
        nextPatternSent = false;
        currentDevicePatternIdx = null;
        queuedPatternIdx = null;
        return;
    }

    // Multi-slot timeline → seed trackers and prime the device.
    currentTlPos = start;
    currentStep = -1;
    nextPatternSent = false;

    const firstPatIdx = tl[start] - 1;
    currentDevicePatternIdx = firstPatIdx;
    queuedPatternIdx = firstPatIdx;

    if (state.isLiveUpdate() && state.isConnected()) {
        try {
            const pat = state.getPattern(firstPatIdx);
            if (pat) {
                await api.savePattern(
                    scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
                );
                setStatus(`Playing P${firstPatIdx + 1} - loop 1/${activeTimelineSlots}`);
                return;
            }
        } catch (err) {
            setStatus('Timeline save error: ' + err.message);
            return;
        }
    }
    setStatus(`Playing P${firstPatIdx + 1} - loop 1/${activeTimelineSlots}`);
}

// ---------------------------------------------------------------------------
// Beat timer
// ---------------------------------------------------------------------------

/**
 * Steps-per-beat: Normal=4, Triplet=3. Driven by the *currently-playing*
 * pattern's triplet toggle, matching the pre-timeline behavior. Timelines
 * with mixed triplet settings will inherit the playing pattern's cadence.
 */
function stepIntervalMs() {
    const bpm = state.getBpm();
    const patIdx = playingPatternIdx();
    const triplet = patIdx !== null ? state.getTriplet(patIdx) : state.getTriplet();
    return timingStepIntervalMs(bpm, triplet);
}

function startBeatTimer(startSync) {
    if (beatTimer) clearTimeout(beatTimer);
    if (currentStep < 0) advanceBeat();
    scheduleNextBeat(delayToNextStep(startSync, stepIntervalMs()));
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

function stopBeatTimer() {
    if (beatTimer) { clearTimeout(beatTimer); beatTimer = null; }
    currentStep = -1;
    localWrapCount = 0;
    currentTlPos = -1;
    nextPatternSent = false;
    currentDevicePatternIdx = null;
    queuedPatternIdx = null;
    prevCheckedMode = false;
    lastSeenTl = null;
    highlightStep(-1);
    highlightColumn(-1);
    stopWrapSync();
}

function restartBeatTimer() {
    if (!state.isPlaying()) return;
    if (beatTimer) clearTimeout(beatTimer);
    scheduleNextBeat();
}

function primeFirstStepHighlight() {
    currentStep = 0;
    if (currentTlPos >= 0) highlightColumn(currentTlPos);
    highlightStep(currentStep, playingPatternIdx());
}

/**
 * Resolve which pattern is audibly playing right now. In timeline mode
 * this is currentDevicePatternIdx (last adopted at wrap); in single-pattern
 * mode it's the focused card.
 */
function playingPatternIdx() {
    if (currentTlPos >= 0 && currentDevicePatternIdx !== null) {
        return currentDevicePatternIdx;
    }
    return state.getFocusedIdx();
}

/**
 * Resolve the 0-based step index where the pre-load save fires, clamped
 * to the current pattern's active-step window.
 */
function advanceBeat() {
    const patIdx = playingPatternIdx();
    const activeSteps = patIdx !== null ? state.getActiveSteps(patIdx) : state.getActiveSteps();
    const nextStep = currentStep + 1;

    // --- Pre-load window: save the next timeline pattern to scratch ------
    if (currentTlPos >= 0) {
        const preStep = preloadStep(activeSteps, ENV_PRELOAD_SAVE_STEP);
        if (nextStep === preStep && !nextPatternSent) {
            nextPatternSent = true;
            const tl = state.getTimeline();
            const target = nextTimelinePos(tl, currentTlPos);
            if (target >= 0 && state.isLiveUpdate() && state.isConnected()) {
                const nextNum = tl[target];
                const curNum = tl[currentTlPos];
                if (nextNum !== curNum && nextNum >= 1) {
                    const nextPatIdx = nextNum - 1;
                    const pat = state.getPattern(nextPatIdx);
                    if (pat) {
                        queuedPatternIdx = nextPatIdx;
                        api.savePattern(
                            scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
                        ).then(() => setStatus(`Pre-loaded P${nextPatIdx + 1}`))
                         .catch(err => setStatus('Pre-load error: ' + err.message));
                    }
                }
            }
        }
    }

    // --- Wrap at active-steps: device swaps buffer; sync trackers --------
    let step = nextStep;
    if (step >= activeSteps) {
        step = 0;
        handlePatternWrap(null);
    } else if (step === 0 && currentTlPos >= 0) {
        // First tick of the very first cycle - paint the column highlight.
        highlightColumn(currentTlPos);
    }

    currentStep = step;
    highlightStep(currentStep, playingPatternIdx());
}

function handlePatternWrap(rustWrapIndex) {
    if (Number.isFinite(rustWrapIndex)) {
        localWrapCount = Math.max(localWrapCount, rustWrapIndex);
    } else {
        localWrapCount += 1;
    }
    nextPatternSent = false;

    if (currentTlPos < 0) return;
    if (queuedPatternIdx !== null) {
        currentDevicePatternIdx = queuedPatternIdx;
    }
    const tl = state.getTimeline();
    const next = advanceCursorToDevicePattern(
        tl, currentTlPos, currentDevicePatternIdx,
    );
    if (next < 0) return;
    currentTlPos = next;
    highlightColumn(currentTlPos);
    const newNum = tl[next];
    const loopNum = countLoopsUpTo(tl, next);
    const total = countNonEmpty(tl);
    setStatus(`Playing P${newNum} - loop ${loopNum}/${total}`);
}

function countLoopsUpTo(tl, pos) {
    let n = 0;
    for (let i = 0; i <= pos; i += 1) if (tl[i] >= 1) n += 1;
    return n;
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

function stopWrapSync() {
    wrapSync = { anchorEpochMs: 0, transportId: 0, wrapIndex: 0 };
}

async function pollWrapSync() {
    if (!state.isPlaying() || !wrapSync.transportId) return;
    const patIdx = playingPatternIdx();
    const activeSteps = patIdx !== null ? state.getActiveSteps(patIdx) : state.getActiveSteps();
    const triplet = patIdx !== null ? state.getTriplet(patIdx) : state.getTriplet();
    try {
        const pulse = await api.transportWrapPulse({
            transportId: wrapSync.transportId,
            anchorEpochMs: wrapSync.anchorEpochMs,
            wrapIndex: wrapSync.wrapIndex,
            activeSteps,
            triplet,
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
    currentStep = 0;
    highlightStep(currentStep, playingPatternIdx());
    if (beatTimer) clearTimeout(beatTimer);
    scheduleNextBeat();
}

function timelineSignature(tl) {
    return Array.isArray(tl) ? tl.join('|') : '';
}

/**
 * React to structural state changes while playing. Re-clamps the timeline
 * cursor against the new active timeline and the device-pattern tracker,
 * then forces an immediate scratch save whenever the resulting cursor slot
 * holds a pattern that differs from what is currently in scratch.
 *
 * Covers all four transition shapes uniformly:
 *   • 0→1 checked-mode ("first checkbox"): default → checked timeline,
 *     cursor lands on the first checked slot, scratch is updated so the
 *     device swaps to the checked pattern at the next wrap.
 *   • 1→0 checked-mode ("last uncheck"): checked → default timeline. If
 *     the previously-checked pattern is absent from the default timeline,
 *     cursor falls back to the default's first non-empty slot and the
 *     scratch is rewritten to the default's pattern, otherwise the device
 *     would keep looping the stale checked-mode buffer forever.
 *   • Mid-mode check/uncheck (timeline edit while checked): same cursor
 *     re-sync + conditional scratch save covers any case where the
 *     currently-playing pattern was just removed from the arrangement.
 *   • Reorder / add / delete / timeline-modal edit: the scratch save is a
 *     no-op when the cursor's pattern already matches what is queued.
 *
 * Step highlighting is driven by currentDevicePatternIdx, so it naturally
 * stays pinned on whatever the device is audibly playing - the device
 * doesn't change until wrap, and currentDevicePatternIdx isn't mutated
 * here.
 */
function onStateChangeDuringPlay(_patternChanged, structuralChange) {
    if (!state.isPlaying()) return;
    if (!structuralChange) return;

    const tl = state.getTimeline();
    const sig = timelineSignature(tl);
    const nowCheckedMode = state.isCheckedMode();
    const modeTransition = nowCheckedMode !== prevCheckedMode;

    // Cheap no-op path - no timeline change and no checked-mode transition.
    if (sig === lastSeenTl && !modeTransition) {
        prevCheckedMode = nowCheckedMode;
        return;
    }
    lastSeenTl = sig;
    prevCheckedMode = nowCheckedMode;

    const activeSlots = countNonEmpty(tl);
    // Any non-trivial change: let the next pre-load re-evaluate the "next
    // slot" save against the new timeline. The immediate-scratch path
    // below will set this back to true after saving so pre-load stays
    // blocked for the rest of the current cycle.
    nextPatternSent = false;

    if (activeSlots === 0) {
        // Empty timeline - nothing to play. Fall to single-pattern loop:
        // step highlighting falls back to the focused card.
        currentTlPos = -1;
        currentDevicePatternIdx = null;
        queuedPatternIdx = null;
        highlightColumn(-1);
        return;
    }

    // activeSlots >= 1: stay in timeline mode.
    //
    // Transitioning into timeline mode from single-pattern (no cursor yet,
    // or device tracker unset): seed the device-pattern tracker from the
    // focused card, which is what the device was looping.
    if (currentTlPos < 0 || currentDevicePatternIdx === null) {
        currentDevicePatternIdx = state.getFocusedIdx();
        queuedPatternIdx = currentDevicePatternIdx;
    }

    // Clamp cursor to a slot that matches what the device is playing.
    // `from = currentTlPos - 1` makes the scan start *at* currentTlPos, so
    // if the current slot already matches we stay put instead of jumping
    // to a duplicate earlier slot.
    const cursor = advanceCursorToDevicePattern(
        tl, currentTlPos - 1, currentDevicePatternIdx,
    );
    if (cursor < 0) return;
    currentTlPos = cursor;
    highlightColumn(currentTlPos);

    if (needsImmediateScratchSave(tl, cursor, queuedPatternIdx)) {
        immediateScratchSave(tl, cursor);
    }
}

/**
 * Save the given timeline slot's pattern to the scratch slot immediately.
 * Used whenever a structural change leaves the scratch buffer pointing at a
 * pattern that the new timeline no longer wants the device to play (first
 * checkbox, last uncheck, mid-mode uncheck of the playing pattern, etc.).
 * Sets nextPatternSent=true to block pre-load from firing a second save in
 * the current cycle.
 */
function immediateScratchSave(tl, slot) {
    const num = tl[slot];
    if (num < 1 || num > state.getPatternCount()) return;
    const patIdx = num - 1;
    queuedPatternIdx = patIdx;
    nextPatternSent = true;
    if (!state.isLiveUpdate() || !state.isConnected()) return;
    const pat = state.getPattern(patIdx);
    if (!pat) return;
    api.savePattern(
        scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
    ).then(() => setStatus(`P${patIdx + 1} scratched (timeline sync)`))
     .catch(err => setStatus('Scratch error: ' + err.message));
}

/**
 * Re-send the next-in-timeline pattern to the device scratch slot. Callers
 * invoke this after a structural pattern change (drag-to-reorder today;
 * could extend to randomize/content edits later) so the buffer the device
 * loads at the *next* wrap reflects the new ordering instead of whatever
 * was queued by the last pre-load.
 *
 * Current cycle audio is intentionally not interrupted - the device keeps
 * looping its in-flight buffer until wrap.
 *
 * No-op outside multi-slot timeline playback, and when LIVE UPDATE is off
 * or MIDI is disconnected.
 */
export function rescratchUpcoming() {
    if (!state.isPlaying() || !state.isLiveUpdate() || !state.isConnected()) return;
    if (currentTlPos < 0) return;
    const tl = state.getTimeline();
    const nextPos = nextTimelinePos(tl, currentTlPos);
    if (nextPos < 0) return;
    const nextNum = tl[nextPos];
    if (nextNum < 1) return;
    const nextPatIdx = nextNum - 1;
    const pat = state.getPattern(nextPatIdx);
    if (!pat) return;
    queuedPatternIdx = nextPatIdx;
    api.savePattern(
        scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
    ).then(() => setStatus(`Re-scratched P${nextPatIdx + 1}`))
     .catch(err => setStatus('Re-scratch error: ' + err.message));
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

export function updateBpmDisplay() {
    const bpm = state.getBpm();
    bpmDisplay.textContent = bpm.toFixed(bpmFineMode ? 2 : 0);
    const angle = ((bpm - 20) / 280) * 300 - 150;
    knobIndicator.style.transform = `rotate(${angle}deg)`;
}

export function updatePlayButton() {
    const icon = btnPlay.querySelector('.material-symbols-outlined');
    if (state.isPlaying()) {
        icon.textContent = 'stop';
        btnPlay.classList.add('led-glow-green');
    } else {
        icon.textContent = 'play_arrow';
        btnPlay.classList.remove('led-glow-green');
    }
}
