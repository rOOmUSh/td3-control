// Main-page transport - BPM knob, play toggle, and the timeline-aware beat
// loop. LIVE UPDATE ON cycles patterns through the device scratch slot.
// LIVE UPDATE OFF follows the same timeline with host-sequenced no-save
// audition updates.
//
// Dual-tracker playback model (per the GROUND TRUTH RULE): the TD-3 always
// plays its in-flight buffer until wrap, even after a new SysEx save. So:
//
//   currentDevicePatternIdx  - what the device is audibly looping right
//                              now. Updates at wrap (device swaps in the
//                              scratch contents). Drives step highlighting.
//   queuedPatternIdx         - what the next wrap should adopt. In LIVE
//                              UPDATE ON this mirrors the scratch slot. In
//                              no-save audition it mirrors the pending host
//                              schedule update.
//
// Timeline cursor (currentTlPos) is used for pre-load math ("what slot
// comes next after the currently-playing one?") and for the column
// highlight in the timeline modal - but NOT for step highlighting. Step
// highlighting follows currentDevicePatternIdx so it tracks what the
// device is audibly playing, not the cursor that may have already advanced
// past it on a checkbox-driven timeline rearrangement.
//
// Playback contract:
//   1. Start finds the first non-empty timeline slot. LIVE UPDATE ON saves
//      that pattern to scratch; no-save audition starts that host schedule.
//      Both seed the trackers to that pattern.
//   2. At the configured pre-load step we queue the next-in-timeline pattern,
//      unless it matches the current one. LIVE UPDATE ON writes scratch;
//      no-save audition records the pending host schedule swap.
//   3. At the active-steps wrap: currentDevicePatternIdx adopts
//      queuedPatternIdx (device just swapped buffers), and the cursor is
//      re-synced via advanceCursorToDevicePattern - in the uninterrupted
//      case this picks the same slot nextTimelinePos would; in the
//      interrupted case (a checkbox override queued a different pattern)
//      it finds the matching slot anywhere in the new timeline.
//   4. Mid-play structural changes (checkbox toggles, drag-to-reorder,
//      timeline edits) fire through onStateChangeDuringPlay:
//        • 0→1 checked-mode transition ("first checkbox"): immediately
//          queue the first slot of the new checked timeline so playback
//          swaps to it at the next wrap; suppress further pre-loads this
//          cycle.
//        • Any other structural change: clamp the cursor so pre-load math
//          and column highlight stay valid.

import * as state from './multipattern/multipattern-state.js';
import { api } from './api.js';
import { highlightStep } from './multipattern/multipattern-list.js';
import { highlightColumn } from './multipattern/multipattern-timeline.js';
import * as preview from './multipattern/multipattern-preview.js';
import { envInt } from './td3-env.js';
import { stepIntervalMs as timingStepIntervalMs } from './shared/transport-timing.js';
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
} from './shared/transport-sync-timing.js';
import {
    adoptQueuedTiming,
    playbackTiming,
    snapshotTiming,
} from './shared/audible-timing.js';
import {
    morphDisplayStepForUniformTick,
    morphRequestPercent,
} from './shared/triplet-morph-timing.js';
import { planForPattern } from './multipattern/triplet-morph-view.js';
import {
    startSyncFromTargetMicros,
    targetEpochMicrosForPlay,
} from './remote-sync-timing.js';
import * as remoteSync from './remote-sync.js';
import { applyRemoteTripletCommand } from './remote-sync-triplet.js';
import {
    firstTimelinePos,
    nextTimelinePos,
    advanceCursorToDevicePattern,
    countNonEmpty,
    needsImmediateScratchSave,
    queueSlotAfterTimelineChange,
} from './multipattern/multipattern-transport-helpers.js';
import { bindPointerPressActivation } from './shared/pointer-activation.js';
import {
    clearBackgroundTimeout,
    setBackgroundTimeout,
} from './shared/background-timer.js';

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
let nextBeatEpochMs = 0;
let scheduledStepIntervalMs = 0;
let bpmPausePhase = null;
let pendingDeviceBpmSync = null;
let deviceBpmSyncInFlight = false;
let bpmSyncGeneration = 0;
let latestTempoRevision = -1;

// Beat-loop cursor state.
let currentStep = -1;
let localWrapCount = 0;
let currentTlPos = -1;            // -1 means single-pattern loop mode
let nextPatternSent = false;      // guards pre-load from double-firing
// True while the main transport is running host-sequenced no-save audition:
// the active timeline is followed with timed Note On/Off, no scratch write,
// and no device clock. Drives which stop endpoint is called.
let auditionMode = false;
let auditionUpdateInFlight = false;
let auditionUpdatePending = false;
let auditionUpdateRevision = 0;
let hostAuditionQueueGeneration = 0;
let hostAuditionQueuedForWrap = null;
let latestHostAuditionCycleEpochMicros = -1;
let hostAuditionScheduleGeneration = null;
let hostAuditionRetryTimer = null;
const HOST_AUDITION_MAX_QUEUE_RETRIES = 3;
const HOST_AUDITION_RETRY_BASE_MS = 100;
const HOST_AUDITION_GENERATION_WATCHDOG_MS = 1000;
let scratchSlot = { group: 1, pattern: 1, side: 'A' };

// Acknowledged audible morph amount. The knob shows the requested value
// immediately, but highlighting follows the server-acknowledged effective
// timing: a nextCycle deferral keeps the previous audible amount until
// its fully-applied cycle epoch passes.
let audibleMorph = { percent: 0, pending: null };

function noteMorphAcknowledgement(response) {
    if (!response || response.tripletMorphPercent === undefined) return;
    const percent = Number(response.tripletMorphPercent) || 0;
    const applyAtMicros = Number(response.tripletMorphFullyAppliedEpochMicros);
    if (response.tripletMorphApplyMode === 'nextCycle'
        && Number.isFinite(applyAtMicros)
        && applyAtMicros > Date.now() * 1000) {
        audibleMorph.pending = { percent, applyAtMicros };
        setStatus(`TRIPLET ${percent}% queued for next cycle`);
        return;
    }
    audibleMorph = { percent, pending: null };
}

function audibleMorphPercentNow() {
    const pending = audibleMorph.pending;
    if (pending && Date.now() * 1000 >= pending.applyAtMicros) {
        audibleMorph = { percent: pending.percent, pending: null };
    }
    return audibleMorph.percent;
}

/**
 * Step index to paint for one uniform beat boundary. While a morph
 * audition is audible the highlighter follows the cached plan's warped
 * onsets; at the endpoint it walks the 12 target cells.
 */
function displayStepFor(step) {
    if (step < 0 || !auditionMode) return step;
    const percent = audibleMorphPercentNow();
    if (percent <= 0) return step;
    const plan = planForPattern(playingPatternIdx());
    if (!plan?.assignments) return step;
    return morphDisplayStepForUniformTick(
        plan.assignments,
        percent,
        step,
        currentPlaybackTiming().activeSteps,
    );
}

// Dual-tracker model - see module header.
let currentDevicePatternIdx = null;   // what device is audibly looping
let queuedPatternIdx = null;          // what's in scratch right now
let currentDeviceTiming = null;       // active steps/triplet audibly in flight
let queuedDeviceTiming = null;        // active steps/triplet queued for wrap

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
    anchorPulseIndex: 0,
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

    remoteSync.init({
        setStatus,
        onCommand: handleRemoteCommand,
    });

    bindPointerPressActivation(btnPlay, togglePlay);

    // BPM knob: scroll wheel. Coarse = ±1 BPM, fine = ±0.01 BPM.
    bpmKnob.addEventListener('wheel', (e) => {
        e.preventDefault();
        const step = bpmFineMode ? 0.01 : 1;
        const delta = e.deltaY < 0 ? step : -step;
        applyLiveBpm(state.getBpm() + delta);
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
                applyLiveBpm(Math.trunc(state.getBpm()));
            } else {
                updateBpmDisplay();
            }
        });
    }

    // BPM knob: click-and-drag (3 px per BPM).
    let dragging = false;
    let dragStartY = 0;
    let dragStartBpm = 0;
    let dragBpm = null;
    bpmKnob.addEventListener('mousedown', (e) => {
        dragging = true;
        dragStartY = e.clientY;
        dragStartBpm = state.getBpm();
        dragBpm = dragStartBpm;
        e.preventDefault();
    });
    document.addEventListener('mousemove', (e) => {
        if (!dragging) return;
        const delta = Math.round((dragStartY - e.clientY) / 3);
        dragBpm = normalizedBpm(dragStartBpm + delta);
        updateBpmDisplay(dragBpm);
    });
    document.addEventListener('mouseup', () => {
        if (!dragging) return;
        dragging = false;
        const bpm = dragBpm;
        dragBpm = null;
        applyLiveBpm(bpm);
    });
}

/** Apply document tempo to the UI session and any transport already running. */
export function applyImportedBpm(centibpm) {
    if (!Number.isInteger(centibpm) || centibpm < 2000 || centibpm > 30000) {
        return false;
    }
    applyLiveBpm(centibpm / 100);
    return true;
}

export async function stopPlaybackForModeChange() {
    if (!state.isPlaying()) return;
    stopWrapSync();
    if (auditionMode) {
        clearHostAuditionQueue();
        await api.auditionStop();
    } else {
        await api.transportStop();
    }
    state.setPlaying(false);
    stopBeatTimer();
    auditionMode = false;
    auditionUpdatePending = false;
    auditionUpdateInFlight = false;
    currentTlPos = -1;
    currentStep = -1;
    nextPatternSent = false;
    updatePlayButton();
}

export function syncAuditionPattern() {
    if (!state.isPlaying() || !auditionMode || !state.isConnected()) {
        // No update will be sent, so nothing will arrive to restart a
        // timer a BPM change paused.
        resumeBeatTimerIfStillPaused();
        return;
    }
    refreshQueuedHostAudition();
    auditionUpdatePending = true;
    if (auditionUpdateInFlight) return;
    flushAuditionUpdate();
}

export function isAuditionActive() {
    return state.isPlaying() && auditionMode;
}

async function flushAuditionUpdate() {
    auditionUpdateInFlight = true;
    try {
        while (auditionUpdatePending) {
            auditionUpdatePending = false;
            if (!state.isPlaying() || !auditionMode || !state.isConnected()) break;
            const patIdx = playingPatternIdx();
            const pat = state.getPattern(patIdx);
            if (!pat) break;
            const generation = hostAuditionQueueGeneration;
            const revision = ++auditionUpdateRevision;
            const expectedScheduleGeneration = hostAuditionScheduleGeneration;
            const requestedTiming = snapshotTiming(pat, currentPlaybackTiming());
            let response;
            try {
                response = await api.auditionUpdate(
                    pat,
                    state.getBpm(),
                    true,
                    expectedScheduleGeneration,
                    state.getGatePercent(),
                    morphRequestPercent(pat, state.getTripletMorphPercent()),
                    state.getMidiChannel(),
                );
            } catch (err) {
                if (isRecoverableAuditionConflict(err)) continue;
                if (isFatalAuditionError(err)) {
                    stopHostAuditionAfterFatalError(err);
                    continue;
                }
                if (generation === hostAuditionQueueGeneration
                    && revision === auditionUpdateRevision
                    && state.isPlaying()
                    && auditionMode) {
                    setStatus('Audition update error: ' + err.message);
                }
                continue;
            }
            if (!auditionUpdatePending
                && generation === hostAuditionQueueGeneration
                && revision === auditionUpdateRevision
                && state.isPlaying()
                && auditionMode
                && playingPatternIdx() === patIdx
                && hostAuditionScheduleGeneration === expectedScheduleGeneration
                && response.scheduleGeneration === expectedScheduleGeneration) {
                noteMorphAcknowledgement(response);
                reconcileAuditionTiming(response, requestedTiming);
            }
        }
    } finally {
        auditionUpdateInFlight = false;
        if (auditionUpdatePending && state.isPlaying() && auditionMode && state.isConnected()) {
            flushAuditionUpdate();
        } else {
            // Nothing further will reconcile: if the loop drained without
            // an authoritative acknowledgement the timer is still paused.
            resumeBeatTimerIfStillPaused();
        }
    }
}

async function handleRemoteCommand(command) {
    if (!command || !command.command) return;
    if (command.command === 'play') {
        if (Number.isFinite(command.centibpm)) {
            state.setBpm(command.centibpm / 100);
            updateBpmDisplay();
        }
        if (!state.isPlaying()) {
            await togglePlay(null, {
                remoteTriggered: true,
                targetEpochMicros: command.targetEpochMicros,
            });
        }
    } else if (command.command === 'stop') {
        if (state.isPlaying()) {
            await togglePlay(null, { remoteTriggered: true });
        }
    } else if (command.command === 'bpm') {
        if (!Number.isFinite(command.centibpm)) return;
        applyLiveBpm(command.centibpm / 100, { relayRemote: false });
    } else if (command.command === 'triplet') {
        if (applyRemoteTripletCommand(command, state)) {
            setStatus(`Remote triplet ${command.triplet ? 'ON' : 'OFF'}`);
        }
    }
}

function mirrorRemoteBpm() {
    if (!state.isPlaying() || !remoteSync.isEnabled()) return;
    remoteSync.relayBpm(Math.round(state.getBpm() * 100))
        .catch(err => setStatus('Remote BPM error: ' + err.message));
}

function normalizedBpm(value) {
    const numeric = typeof value === 'number' ? value : Number.parseFloat(value);
    const safe = Number.isFinite(numeric) ? numeric : 120;
    return Math.round(Math.max(20, Math.min(300, safe)) * 100) / 100;
}

function applyLiveBpm(value, { relayRemote = true } = {}) {
    const previousBpm = state.getBpm();
    const nextBpm = normalizedBpm(value);
    if (nextBpm === previousBpm) {
        updateBpmDisplay();
        return;
    }

    if (state.isPlaying()) pauseBeatTimerForBpm(previousBpm);
    state.setBpm(nextBpm);
    updateBpmDisplay();

    if (state.isPlaying()) {
        if (auditionMode) {
            syncAuditionPattern();
        } else if (state.isConnected()) {
            queueDeviceBpmSync();
        } else {
            resumeBeatTimerPreservingPhase();
        }
    } else {
        preview.syncBpmChange(previousBpm);
    }

    if (relayRemote) mirrorRemoteBpm();
}

function pauseBeatTimerForBpm(previousBpm) {
    if (bpmPausePhase) return;
    const timing = currentPlaybackTiming();
    const previousIntervalMs = scheduledStepIntervalMs
        || timingStepIntervalMs(previousBpm, timing.triplet);
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
 * A BPM change pauses the beat timer and hands responsibility for
 * restarting it to the audition acknowledgement, which carries the
 * server's real cycle epoch. The audition itself keeps sounding whatever
 * happens to that request, so every path that fails to reconcile - a
 * rejected or dropped request, a response that is not authoritative, a
 * superseded generation, a disconnect - used to leave the timer stopped
 * under audible playback, freezing the highlight until playback was
 * restarted. Falling back to local phase-preserving scheduling keeps the
 * highlight advancing at the new tempo. It is not device-anchored, but a
 * later successful update re-anchors it, and it tracks the audio far
 * better than a stopped timer.
 */
function resumeBeatTimerIfStillPaused() {
    if (!bpmPausePhase) return;
    resumeBeatTimerPreservingPhase();
}

function resumeBeatTimerPreservingPhase() {
    if (!state.isPlaying()) {
        bpmPausePhase = null;
        return;
    }
    const nextIntervalMs = stepIntervalMs();
    const phase = bpmPausePhase;
    bpmPausePhase = null;
    const delayMs = phase
        ? phasePreservingStepDelay(
            phase.previousIntervalMs,
            nextIntervalMs,
            phase.nextBoundaryEpochMs,
        )
        : nextIntervalMs;
    scheduleNextBeat(Math.max(0.01, delayMs));
}

function queueDeviceBpmSync() {
    bpmSyncGeneration += 1;
    pendingDeviceBpmSync = {
        generation: bpmSyncGeneration,
        bpm: state.getBpm(),
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
                    setStatus('BPM error: ' + err.message);
                }
                continue;
            }

            if (pendingDeviceBpmSync || request.generation !== bpmSyncGeneration) continue;
            if (!state.isPlaying() || auditionMode) {
                bpmPausePhase = null;
                continue;
            }
            reconcileDeviceBpm(response, request.bpm);
        }
    } finally {
        deviceBpmSyncInFlight = false;
        if (pendingDeviceBpmSync) flushDeviceBpmSync();
    }
}

function reconcileDeviceBpm(response, requestedBpm) {
    const expectedCentibpm = Math.round(requestedBpm * 100);
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
        setStatus('BPM sync error: device timing acknowledgement missing');
        return;
    }

    latestTempoRevision = response.tempoRevision;
    const timing = currentPlaybackTiming();
    const sync = stepSyncFromPulse({
        pulseIndex: response.effectivePulseIndex,
        pulseEpochMicros: response.effectiveAtEpochMicros,
        anchorPulseIndex: wrapSync.anchorPulseIndex,
        centibpm: response.centibpm,
        activeSteps: timing.activeSteps,
        triplet: timing.triplet,
    });
    const activeSteps = timing.activeSteps;
    const authoritativeStepOffset = Math.floor(
        Math.max(0, sync.currentPulseIndex - wrapSync.anchorPulseIndex)
        / pulsesPerStep(timing.triplet),
    );
    const isSingleNonWrappingAdvance = currentStep >= 0
        && currentStep + 1 < activeSteps
        && authoritativeStepOffset === currentStep + 1;
    const isSameStep = currentStep >= 0 && authoritativeStepOffset === currentStep;

    if (isSingleNonWrappingAdvance) {
        advanceBeat();
    } else if (!isSameStep) {
        // A delayed acknowledgement can arrive after more than one step. Move
        // only the visual cursor; replaying missed preload or wrap side effects
        // here could mutate the device out of order. The pulse wrap waiter
        // remains authoritative for pattern transitions.
        currentStep = sync.step;
        highlightStep(displayStepFor(currentStep), playingPatternIdx());
    }

    bpmPausePhase = null;
    scheduleBeatAt(sync.nextStepEpochMicros / 1000);
}

function reconcileAuditionTiming(response, audibleTiming = null) {
    const authoritative = response
        && Number.isFinite(response.centibpm)
        && response.centibpm === Math.round(state.getBpm() * 100)
        && Number.isFinite(response.cycleEpochMicros)
        && Number.isFinite(response.cyclePeriodMicros);
    if (!authoritative) {
        if (bpmPausePhase) {
            setStatus('Audition sync error: applied timing acknowledgement missing');
        }
        return false;
    }

    if (audibleTiming) currentDeviceTiming = audibleTiming;
    const timing = currentPlaybackTiming();
    const sync = stepSyncFromCycle({
        cycleEpochMicros: response.cycleEpochMicros,
        cyclePeriodMicros: response.cyclePeriodMicros,
        activeSteps: timing.activeSteps,
    });
    currentStep = sync.step;
    highlightStep(displayStepFor(currentStep), playingPatternIdx());
    bpmPausePhase = null;
    scheduleBeatAt(sync.nextStepEpochMicros / 1000, sync.stepPeriodMicros / 1000);
    return true;
}

function captureRemoteRelay(promise) {
    return promise.then(
        response => ({ response }),
        error => ({ error }),
    );
}

async function reportRemoteRelayOutcome(outcomePromise, label) {
    const outcome = await outcomePromise;
    if (outcome.error) {
        setStatus(`Remote ${label} error: ${outcome.error.message}`);
        return;
    }
    if (outcome.response && !outcome.response.skipped) {
        setStatus(remoteSync.formatRemoteSyncSuccess(`Remote ${label}`, outcome.response));
    }
}

// ---------------------------------------------------------------------------
// Play toggle
// ---------------------------------------------------------------------------

async function togglePlay(event, options = {}) {
    const remoteTriggered = !!options.remoteTriggered;
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }
    try {
        if (state.isPlaying()) {
            stopWrapSync();
            if (auditionMode) {
                clearHostAuditionQueue();
                await api.auditionStop();
            } else {
                await api.transportStop();
            }
            state.setPlaying(false);
            stopBeatTimer();
            auditionMode = false;
            auditionUpdatePending = false;
            auditionUpdateInFlight = false;
            setStatus('Stopped');
            if (!remoteTriggered && remoteSync.isEnabled()) {
                remoteSync.relayStop()
                    .catch(err => setStatus('Remote stop error: ' + err.message));
            }
        } else {
            // Stop any active per-card preview first - they share the
            // device output, so starting main play over a preview would
            // double-send and leave the preview button lit.
            await preview.stop();

            let targetEpochMicros = Number.isFinite(options.targetEpochMicros)
                ? options.targetEpochMicros
                : null;
            let remotePlayOutcome = null;
            if (!remoteTriggered && remoteSync.isEnabled()) {
                targetEpochMicros = plannedStartTargetEpochMicros(event);
                remotePlayOutcome = captureRemoteRelay(remoteSync.relayPlay({
                    centibpm: Math.round(state.getBpm() * 100),
                    targetEpochMicros,
                }));
            }

            // LIVE UPDATE off: host-sequence the active timeline with timed
            // Note On/Off, no scratch write, and no device clock.
            if (!state.isLiveUpdate()) {
                auditionMode = false;
                await startTimelinePlayback();
                const patIdx = currentDevicePatternIdx !== null
                    ? currentDevicePatternIdx
                    : playingPatternIdx();
                const pat = state.getPattern(patIdx);
                if (!pat) { setStatus('No pattern to audition'); return; }
                const auditionStart = await api.auditionPattern(
                    pat,
                    state.getBpm(),
                    true,
                    targetEpochMicros,
                    state.getGatePercent(),
                    morphRequestPercent(pat, state.getTripletMorphPercent()),
                    state.getMidiChannel(),
                );
                noteMorphAcknowledgement(auditionStart);
                hostAuditionScheduleGeneration = Number.isFinite(
                    auditionStart.scheduleGeneration,
                ) ? auditionStart.scheduleGeneration : null;
                auditionMode = true;
                auditionUpdatePending = false;
                auditionUpdateInFlight = false;
                state.setPlaying(true);
                if (!reconcileAuditionTiming(auditionStart)) {
                    const auditionStartSync = Number.isFinite(
                        auditionStart.effectiveAtEpochMicros,
                    ) ? auditionStart : startSyncFromTargetMicros(targetEpochMicros);
                    startBeatTimer(auditionStartSync);
                }
                const activeTimelineSlots = countNonEmpty(state.getTimeline());
                if (currentTlPos >= 0 && activeTimelineSlots > 1) {
                    setStatus(`Host audition P${patIdx + 1} - loop 1/${activeTimelineSlots}`);
                } else {
                    setStatus(`Host audition: P${patIdx + 1} (no save)`);
                }
            } else {
                auditionMode = false;
                await startTimelinePlayback();
                const startSync = await api.transportStart(state.getBpm(), targetEpochMicros);
                state.setPlaying(true);
                startBeatTimer(startSync);
                startWrapSync(startSync);
            }

            if (remotePlayOutcome) {
                await reportRemoteRelayOutcome(remotePlayOutcome, 'play');
            }
        }
        updatePlayButton();
    } catch (err) {
        setStatus('Transport error: ' + err.message);
    }
}

function plannedStartPatternIdx() {
    const tl = state.getTimeline();
    const start = firstTimelinePos(tl);
    const activeTimelineSlots = countNonEmpty(tl);
    if (activeTimelineSlots >= 1 && start >= 0) {
        const patIdx = tl[start] - 1;
        if (patIdx >= 0 && patIdx < state.getPatternCount()) return patIdx;
    }
    return state.getFocusedIdx();
}

function plannedStartTargetEpochMicros(event) {
    const patIdx = plannedStartPatternIdx();
    const activeSteps = patIdx !== null ? state.getActiveSteps(patIdx) : state.getActiveSteps();
    const triplet = patIdx !== null ? state.getTriplet(patIdx) : state.getTriplet();
    return targetEpochMicrosForPlay(event, state.getBpm(), activeSteps, triplet);
}

function timingForPattern(patIdx) {
    const fallback = {
        activeSteps: patIdx !== null ? state.getActiveSteps(patIdx) : state.getActiveSteps(),
        triplet: patIdx !== null ? state.getTriplet(patIdx) : state.getTriplet(),
    };
    const pat = patIdx !== null ? state.getPattern(patIdx) : null;
    return snapshotTiming(pat, fallback);
}

function currentPlaybackTiming() {
    return playbackTiming({
        liveUpdate: state.isLiveUpdate(),
        auditionMode,
        audibleTiming: currentDeviceTiming,
        fallbackTiming: timingForPattern(playingPatternIdx()),
    });
}

function seedDeviceTiming(patIdx) {
    if (patIdx === null || patIdx === undefined) {
        currentDeviceTiming = null;
        queuedDeviceTiming = null;
        return;
    }
    currentDeviceTiming = timingForPattern(patIdx);
    queuedDeviceTiming = currentDeviceTiming;
}

function noteQueuedDeviceTiming(patIdx, pattern = null) {
    if (patIdx === null || patIdx === undefined) return;
    queuedPatternIdx = patIdx;
    queuedDeviceTiming = snapshotTiming(pattern || state.getPattern(patIdx), {
        activeSteps: state.getActiveSteps(patIdx),
        triplet: state.getTriplet(patIdx),
    });
}

function adoptQueuedDeviceTiming() {
    if (queuedPatternIdx !== null) {
        currentDevicePatternIdx = queuedPatternIdx;
        currentDeviceTiming = adoptQueuedTiming(currentDeviceTiming, queuedDeviceTiming)
            || timingForPattern(currentDevicePatternIdx);
    }
}

export function noteLiveScratchPatternQueued(patIdx, pattern = null) {
    if (!state.isPlaying() || !state.isLiveUpdate() || auditionMode) return;
    noteQueuedDeviceTiming(patIdx, pattern);
}

/**
 * Prime the timeline cursor and playback trackers. In LIVE UPDATE ON mode
 * this also sends the first pattern to the scratch slot. In no-save host
 * audition mode it only chooses the first pattern to audition.
 *
 * Called once on play-start. When the timeline is empty or single-slot we
 * fall back to focused-pattern loop mode: currentTlPos stays -1, advanceBeat
 * will leave it alone.
 */
async function startTimelinePlayback() {
    clearHostAuditionQueue();
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
        const focused = state.getFocusedIdx();
        const patIdx = start >= 0 ? tl[start] - 1 : focused;
        const validPatIdx = Number.isInteger(patIdx)
            && patIdx >= 0
            && patIdx < state.getPatternCount();
        currentDevicePatternIdx = validPatIdx ? patIdx : null;
        queuedPatternIdx = validPatIdx ? patIdx : null;
        seedDeviceTiming(validPatIdx ? patIdx : null);
        if (validPatIdx && state.isLiveUpdate() && state.isConnected()) {
            try {
                const pat = state.getPattern(patIdx);
                if (pat) {
                    await api.savePattern(
                        scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
                    );
                    setStatus(`Playing P${patIdx + 1}`);
                    return;
                }
            } catch (err) {
                setStatus('Timeline save error: ' + err.message);
                return;
            }
        }
        return;
    }

    // Multi-slot timeline → seed trackers and prime the device.
    currentTlPos = start;
    currentStep = -1;
    nextPatternSent = false;

    const firstPatIdx = tl[start] - 1;
    currentDevicePatternIdx = firstPatIdx;
    queuedPatternIdx = firstPatIdx;
    seedDeviceTiming(firstPatIdx);

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
    return timingStepIntervalMs(bpm, currentPlaybackTiming().triplet);
}

function startBeatTimer(startSync) {
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    bpmPausePhase = null;
    const timing = currentPlaybackTiming();
    const projected = stepSyncFromTransportStart({
        startSync,
        bpm: state.getBpm(),
        activeSteps: timing.activeSteps,
        triplet: timing.triplet,
    });
    if (projected) {
        currentStep = projected.step;
        if (currentTlPos >= 0) highlightColumn(currentTlPos);
        highlightStep(displayStepFor(currentStep), playingPatternIdx());
        scheduleBeatAt(
            projected.nextStepEpochMicros / 1000,
            projected.tickPeriodMicros * pulsesPerStep(timing.triplet) / 1000,
        );
        return;
    }
    if (currentStep < 0) advanceBeat();
    scheduleNextBeat(delayToNextStep(startSync, stepIntervalMs()));
}

function scheduleNextBeat(delayMs) {
    if (!state.isPlaying()) return;
    const interval = stepIntervalMs();
    const delay = Number.isFinite(delayMs) && delayMs >= 0 ? delayMs : interval;
    scheduleBeatAt(Date.now() + delay, interval);
}

function scheduleBeatAt(deadlineEpochMs, intervalMs = stepIntervalMs()) {
    if (!state.isPlaying()) return;
    const nowMs = Date.now();
    const deadline = Number.isFinite(deadlineEpochMs)
        ? deadlineEpochMs
        : nowMs + intervalMs;
    const delay = Math.max(0.01, deadline - nowMs);
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    const interval = Number.isFinite(intervalMs) && intervalMs > 0
        ? intervalMs
        : stepIntervalMs();
    scheduledStepIntervalMs = interval;
    nextBeatEpochMs = deadline;
    beatTimer = setBackgroundTimeout(runBeatTimer, delay);
}

function runBeatTimer() {
    const boundaryEpochMs = nextBeatEpochMs || Date.now();
    const interval = stepIntervalMs();
    const nowMs = Date.now();
    beatTimer = null;
    nextBeatEpochMs = 0;
    const elapsedBoundaries = elapsedStepBoundaries(boundaryEpochMs, interval, nowMs);

    if (!auditionMode && state.isConnected() && elapsedBoundaries > 1) {
        queueDeviceBpmSync();
        return;
    }

    let reanchored = false;
    for (let i = 0; i < elapsedBoundaries; i += 1) {
        if (advanceBeat()) {
            reanchored = true;
            break;
        }
    }
    if (!reanchored) {
        scheduleBeatAt(boundaryEpochMs + elapsedBoundaries * interval, interval);
    }
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
    currentTlPos = -1;
    nextPatternSent = false;
    currentDevicePatternIdx = null;
    queuedPatternIdx = null;
    currentDeviceTiming = null;
    queuedDeviceTiming = null;
    audibleMorph = { percent: 0, pending: null };
    clearHostAuditionQueue();
    prevCheckedMode = false;
    lastSeenTl = null;
    highlightStep(-1);
    highlightColumn(-1);
    stopWrapSync();
}

/**
 * Resolve which pattern is audibly playing right now. In timeline mode
 * this is currentDevicePatternIdx (last adopted at wrap). When no device
 * tracker has been seeded, fall back to the focused card.
 */
function playingPatternIdx() {
    if (currentDevicePatternIdx !== null) {
        return currentDevicePatternIdx;
    }
    return state.getFocusedIdx();
}

function clearHostAuditionQueue() {
    if (hostAuditionRetryTimer) {
        clearTimeout(hostAuditionRetryTimer);
        hostAuditionRetryTimer = null;
    }
    hostAuditionQueueGeneration += 1;
    auditionUpdateRevision += 1;
    hostAuditionQueuedForWrap = null;
    latestHostAuditionCycleEpochMicros = -1;
    hostAuditionScheduleGeneration = null;
}

function refreshQueuedHostAudition() {
    const queued = hostAuditionQueuedForWrap;
    if (!queued
        || queued.response
        || !state.isPlaying()
        || !auditionMode
        || state.isLiveUpdate()
        || !state.isConnected()
        || (currentTlPos < 0 && queued.toPos !== -1)) {
        return false;
    }
    const tl = state.getTimeline();
    const target = currentTlPos >= 0 ? nextTimelinePos(tl, currentTlPos) : -1;
    if (target < 0 && queued.toPos !== -1) return false;
    const patIdx = target >= 0 ? tl[target] - 1 : state.getFocusedIdx();
    const pattern = state.getPattern(patIdx);
    if (!pattern) return false;
    const timing = snapshotTiming(pattern, {
        activeSteps: state.getActiveSteps(patIdx),
        triplet: state.getTriplet(patIdx),
    });
    return queueHostAuditionForNextCycle(target, patIdx, pattern, timing, true);
}

function queueHostAuditionForNextCycle(
    toPos,
    patIdx,
    pattern = null,
    timing = null,
    replace = false,
    retryCount = 0,
    unguarded = false,
) {
    if (!state.isPlaying()
        || !auditionMode
        || state.isLiveUpdate()
        || !state.isConnected()
        || (currentTlPos < 0 && toPos !== -1)) {
        return false;
    }
    const nextPattern = pattern || state.getPattern(patIdx);
    if (!nextPattern) return false;

    const existing = hostAuditionQueuedForWrap;
    const sameTarget = existing
        && existing.generation === hostAuditionQueueGeneration
        && existing.fromPos === currentTlPos
        && existing.toPos === toPos
        && existing.patIdx === patIdx;
    if (sameTarget && !replace) {
        nextPatternSent = true;
        return true;
    }

    if (hostAuditionRetryTimer) {
        clearTimeout(hostAuditionRetryTimer);
        hostAuditionRetryTimer = null;
    }
    const queued = {
        generation: hostAuditionQueueGeneration,
        revision: (existing?.revision || 0) + 1,
        fromPos: currentTlPos,
        toPos,
        patIdx,
        timing: timing || snapshotTiming(nextPattern, {
            activeSteps: state.getActiveSteps(patIdx),
            triplet: state.getTriplet(patIdx),
        }),
        centibpm: Math.round(state.getBpm() * 100),
        gatePercent: state.getGatePercent(),
        midiChannel: state.getMidiChannel(),
        tripletMorphPercent: morphRequestPercent(nextPattern, state.getTripletMorphPercent()),
        expectedScheduleGeneration: unguarded ? null : hostAuditionScheduleGeneration,
        inFlight: true,
        response: null,
        wrapReached: false,
        needsRetry: false,
        retryCount,
    };
    hostAuditionQueuedForWrap = queued;
    nextPatternSent = true;

    api.auditionQueueNextCycle(
        nextPattern,
        state.getBpm(),
        true,
        queued.expectedScheduleGeneration,
        queued.gatePercent,
        queued.tripletMorphPercent,
        queued.midiChannel,
    ).then((response) => {
        if (queued.generation !== hostAuditionQueueGeneration) return;
        queued.inFlight = false;
        queued.response = response;
        completeHostAuditionTransition(queued);
    }).catch((err) => {
        if (hostAuditionQueuedForWrap !== queued
            || queued.generation !== hostAuditionQueueGeneration) {
            return;
        }
        if (isRecoverableAuditionConflict(err)) {
            queued.inFlight = false;
            queued.needsRetry = true;
            if (err.response?.code === 'generation_conflict'
                && hostAuditionScheduleGeneration !== queued.expectedScheduleGeneration) {
                retryHostAuditionQueue(queued);
            } else if (err.response?.code === 'generation_conflict') {
                scheduleHostAuditionGenerationWatchdog(queued);
            } else if (err.response?.code === 'superseded') {
                scheduleHostAuditionQueueRetry(queued, err);
            }
            return;
        }
        if (isFatalAuditionError(err)) {
            stopHostAuditionAfterFatalError(err);
            return;
        }
        scheduleHostAuditionQueueRetry(queued, err);
    });
    return true;
}

function scheduleHostAuditionGenerationWatchdog(queued) {
    if (hostAuditionQueuedForWrap !== queued || hostAuditionRetryTimer) return;
    nextPatternSent = true;
    hostAuditionRetryTimer = setTimeout(() => {
        hostAuditionRetryTimer = null;
        retryHostAuditionQueue(queued, false, true);
    }, HOST_AUDITION_GENERATION_WATCHDOG_MS);
}

function isRecoverableAuditionConflict(err) {
    return err?.status === 409
        && (err.response?.code === 'generation_conflict'
            || err.response?.code === 'superseded');
}

function isFatalAuditionError(err) {
    return err?.response?.code === 'audition_stopped'
        || err?.response?.code === 'playback_failed'
        || (err?.status === 409 && !isRecoverableAuditionConflict(err));
}

function stopHostAuditionAfterFatalError(err) {
    if (!state.isPlaying() && !auditionMode) return;
    api.auditionStop().catch(() => {});
    state.setPlaying(false);
    stopBeatTimer();
    auditionMode = false;
    auditionUpdatePending = false;
    auditionUpdateInFlight = false;
    updatePlayButton();
    const code = err?.response?.code;
    const label = code === 'playback_failed'
        ? 'Audition playback failed'
        : 'Audition stopped';
    setStatus(`${label}: ${err.message}`);
}

function scheduleHostAuditionQueueRetry(queued, err) {
    if (hostAuditionQueuedForWrap !== queued) return;
    queued.inFlight = false;
    queued.needsRetry = true;
    nextPatternSent = true;
    if (queued.retryCount >= HOST_AUDITION_MAX_QUEUE_RETRIES) {
        hostAuditionQueuedForWrap = null;
        setStatus(`Audition queue error: ${err.message}; retry limit reached`);
        return;
    }
    if (hostAuditionRetryTimer) return;
    const delayMs = HOST_AUDITION_RETRY_BASE_MS * (2 ** queued.retryCount);
    setStatus(`Audition queue error: ${err.message}; retrying`);
    hostAuditionRetryTimer = setTimeout(() => {
        hostAuditionRetryTimer = null;
        retryHostAuditionQueue(queued, true);
    }, delayMs);
}

function retryHostAuditionQueue(queued, countFailure = false, unguarded = false) {
    if (hostAuditionQueuedForWrap !== queued
        || !queued.needsRetry
        || !state.isPlaying()
        || !auditionMode
        || !state.isConnected()) {
        return false;
    }
    const pattern = state.getPattern(queued.patIdx);
    if (!pattern) return false;
    return queueHostAuditionForNextCycle(
        queued.toPos,
        queued.patIdx,
        pattern,
        snapshotTiming(pattern, queued.timing),
        true,
        countFailure ? queued.retryCount + 1 : queued.retryCount,
        unguarded,
    );
}

function completeHostAuditionTransition(queued) {
    if (!queued
        || queued.generation !== hostAuditionQueueGeneration
        || !queued.response
        || !state.isPlaying()
        || !auditionMode
        || state.isLiveUpdate()
        || !state.isConnected()) {
        return false;
    }
    const tl = state.getTimeline();
    if (queued.response.centibpm !== queued.centibpm
        || !Number.isFinite(queued.response.cycleEpochMicros)
        || !Number.isFinite(queued.response.cyclePeriodMicros)) {
        if (hostAuditionQueuedForWrap === queued) {
            hostAuditionQueuedForWrap = null;
            nextPatternSent = false;
        }
        setStatus('Audition sync error: queued timing acknowledgement missing');
        return false;
    }
    const responseGeneration = queued.response.scheduleGeneration;
    const hasScheduleGeneration = Number.isFinite(responseGeneration);
    if (hasScheduleGeneration
        ? (hostAuditionScheduleGeneration !== null
            && responseGeneration <= hostAuditionScheduleGeneration)
        : queued.response.cycleEpochMicros <= latestHostAuditionCycleEpochMicros) {
        return false;
    }

    latestHostAuditionCycleEpochMicros = queued.response.cycleEpochMicros;
    if (hasScheduleGeneration) hostAuditionScheduleGeneration = responseGeneration;
    if (hostAuditionQueuedForWrap === queued) hostAuditionQueuedForWrap = null;
    queuedPatternIdx = queued.patIdx;
    queuedDeviceTiming = queued.timing;
    adoptQueuedDeviceTiming();
    auditionUpdateRevision += 1;
    const appliedPos = tl[queued.toPos] === queued.patIdx + 1
        ? queued.toPos
        : advanceCursorToDevicePattern(tl, currentTlPos, queued.patIdx);
    if (appliedPos >= 0) currentTlPos = appliedPos;
    nextPatternSent = hostAuditionQueuedForWrap !== null;
    highlightColumn(currentTlPos);
    const loopNum = countLoopsUpTo(tl, currentTlPos);
    const total = countNonEmpty(tl);
    if (total > 0 && currentTlPos >= 0) {
        setStatus(`Playing P${queued.patIdx + 1} - loop ${loopNum}/${total}`);
    } else {
        setStatus(`Host audition: P${queued.patIdx + 1} (no save)`);
    }
    noteMorphAcknowledgement(queued.response);
    const reconciled = reconcileAuditionTiming(queued.response);
    const pending = hostAuditionQueuedForWrap;
    if (pending?.needsRetry) retryHostAuditionQueue(pending);
    return reconciled;
}

/**
 * Resolve the 0-based step index where the pre-load save fires, clamped
 * to the current pattern's active-step window.
 */
function advanceBeat() {
    const activeSteps = currentPlaybackTiming().activeSteps;
    const nextStep = currentStep + 1;

    // --- Pre-load window: queue the next timeline pattern ----------------
    if (currentTlPos >= 0) {
        const preStep = preloadStep(activeSteps, ENV_PRELOAD_SAVE_STEP);
        if (nextStep >= preStep && !nextPatternSent) {
            const tl = state.getTimeline();
            const target = nextTimelinePos(tl, currentTlPos);
            if (target >= 0) {
                const nextNum = tl[target];
                const curNum = tl[currentTlPos];
                if (nextNum !== curNum && nextNum >= 1) {
                    const nextPatIdx = nextNum - 1;
                    const pat = state.getPattern(nextPatIdx);
                    if (pat) {
                        const queuedTiming = snapshotTiming(pat, {
                            activeSteps: state.getActiveSteps(nextPatIdx),
                            triplet: state.getTriplet(nextPatIdx),
                        });
                        if (state.isLiveUpdate() && state.isConnected()) {
                            nextPatternSent = true;
                            queuedPatternIdx = nextPatIdx;
                            api.savePattern(
                                scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
                            ).then(() => {
                                queuedDeviceTiming = queuedTiming;
                                setStatus(`Pre-loaded P${nextPatIdx + 1}`);
                            })
                             .catch(err => setStatus('Pre-load error: ' + err.message));
                        } else if (auditionMode && state.isConnected()) {
                            queueHostAuditionForNextCycle(target, nextPatIdx, pat, queuedTiming);
                        }
                    }
                } else {
                    nextPatternSent = true;
                }
            }
        }
    }

    // --- Wrap at active-steps: device swaps buffer; sync trackers --------
    let step = nextStep;
    let queuedHostTransition = null;
    if (step >= activeSteps) {
        step = 0;
        queuedHostTransition = handlePatternWrap(null);
    } else if (step === 0 && currentTlPos >= 0) {
        // First tick of the very first cycle - paint the column highlight.
        highlightColumn(currentTlPos);
    }

    currentStep = step;
    highlightStep(displayStepFor(currentStep), playingPatternIdx());
    if (queuedHostTransition) return completeHostAuditionTransition(queuedHostTransition);
    return false;
}

function handlePatternWrap(rustWrapIndex) {
    if (Number.isFinite(rustWrapIndex)) {
        localWrapCount = Math.max(localWrapCount, rustWrapIndex);
    } else {
        localWrapCount += 1;
    }
    const previousPatIdx = currentDevicePatternIdx;
    const hostPlayback = auditionMode && !state.isLiveUpdate() && state.isConnected();
    if (hostPlayback && currentTlPos >= 0) {
        const tl = state.getTimeline();
        const target = nextTimelinePos(tl, currentTlPos);
        const targetPatIdx = target >= 0 ? tl[target] - 1 : -1;
        const queued = hostAuditionQueuedForWrap;
        if (queued && queued.generation === hostAuditionQueueGeneration) {
            queued.wrapReached = true;
            nextPatternSent = true;
            return queued.response ? queued : null;
        }
        if (targetPatIdx >= 0 && targetPatIdx !== previousPatIdx) {
            nextPatternSent = false;
            setStatus(`Audition queue error: P${targetPatIdx + 1} will retry next cycle`);
            return null;
        }
    }

    nextPatternSent = false;
    if (!hostPlayback) adoptQueuedDeviceTiming();
    if (currentTlPos < 0) return;
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
    return null;
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
        anchorPulseIndex: Number.isFinite(startSync.effectivePulseIndex)
            ? startSync.effectivePulseIndex
            : 0,
        transportId: startSync.transportId,
        wrapIndex: 0,
    };
    pollWrapSync();
}

function stopWrapSync() {
    wrapSync = { anchorEpochMs: 0, anchorPulseIndex: 0, transportId: 0, wrapIndex: 0 };
}

async function pollWrapSync() {
    if (!state.isPlaying() || !wrapSync.transportId) return;
    const timing = currentPlaybackTiming();
    try {
        const pulse = await api.transportWrapPulse({
            transportId: wrapSync.transportId,
            anchorEpochMs: wrapSync.anchorEpochMs,
            anchorPulseIndex: wrapSync.anchorPulseIndex,
            wrapIndex: wrapSync.wrapIndex,
            activeSteps: timing.activeSteps,
            triplet: timing.triplet,
        });
        if (!pulse.ok) return;
        if (!state.isPlaying() || pulse.transportId !== wrapSync.transportId) return;
        if (pulse.exactBoundary === false) {
            const recoveredAnchor = applyMissedWrapPulse(
                pulse,
                wrapSync.anchorPulseIndex,
                timing,
            );
            if (!Number.isFinite(recoveredAnchor)) return;
            wrapSync.anchorEpochMs = pulse.wrapEpochMs;
            wrapSync.anchorPulseIndex = recoveredAnchor;
            wrapSync.wrapIndex = pulse.wrapIndex;
            pollWrapSync();
            return;
        }
        applyWrapPulse(pulse);
        wrapSync.anchorEpochMs = pulse.wrapEpochMs;
        if (Number.isFinite(pulse.pulseIndex)) {
            wrapSync.anchorPulseIndex = pulse.pulseIndex;
        }
        wrapSync.wrapIndex = pulse.wrapIndex;
        pollWrapSync();
    } catch (err) {
        setStatus('Wrap sync error: ' + err.message);
    }
}

function applyMissedWrapPulse(pulse, previousAnchorPulseIndex, timing) {
    if (!Number.isFinite(pulse.pulseIndex)) {
        setStatus('Wrap sync error: recovery pulse missing');
        return null;
    }
    const pulseEpochMicros = Number.isFinite(pulse.wrapEpochMicros)
        ? pulse.wrapEpochMicros
        : pulse.wrapEpochMs * 1000;
    const requestedBoundaryPulseIndex = previousAnchorPulseIndex
        + timing.activeSteps * pulsesPerStep(timing.triplet);
    if (pulse.wrapIndex > localWrapCount) {
        handlePatternWrap(pulse.wrapIndex);
    }
    const recoveredTiming = currentPlaybackTiming();
    const recoveredAnchor = cycleAnchorPulseIndex(
        pulse.pulseIndex,
        recoveredTiming.activeSteps,
        recoveredTiming.triplet,
        requestedBoundaryPulseIndex,
    );
    const sync = stepSyncFromPulse({
        pulseIndex: pulse.pulseIndex,
        pulseEpochMicros,
        anchorPulseIndex: recoveredAnchor,
        centibpm: Math.round(state.getBpm() * 100),
        activeSteps: recoveredTiming.activeSteps,
        triplet: recoveredTiming.triplet,
    });
    currentStep = sync.step;
    highlightStep(displayStepFor(currentStep), playingPatternIdx());
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = null;
    nextBeatEpochMs = 0;
    if (!deviceBpmSyncInFlight && !pendingDeviceBpmSync) {
        bpmPausePhase = null;
        scheduleBeatAt(sync.nextStepEpochMicros / 1000);
    }
    return recoveredAnchor;
}

function applyWrapPulse(pulse) {
    if (pulse.wrapIndex > localWrapCount) {
        handlePatternWrap(pulse.wrapIndex);
    }
    const timing = currentPlaybackTiming();
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
        activeSteps: timing.activeSteps,
        triplet: timing.triplet,
    });
    currentStep = sync.step;
    highlightStep(displayStepFor(currentStep), playingPatternIdx());
    if (beatTimer) clearBackgroundTimeout(beatTimer);
    beatTimer = null;
    nextBeatEpochMs = 0;
    if (deviceBpmSyncInFlight || pendingDeviceBpmSync) return;
    bpmPausePhase = null;
    scheduleBeatAt(sync.nextStepEpochMicros / 1000);
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
        if (auditionMode && currentTlPos >= 0) {
            const fallbackIdx = state.getFocusedIdx();
            const fallbackPattern = state.getPattern(fallbackIdx);
            const needsFallback = hostAuditionQueuedForWrap
                || fallbackIdx !== currentDevicePatternIdx;
            if (fallbackPattern && needsFallback) {
                queueHostAuditionForNextCycle(
                    -1,
                    fallbackIdx,
                    fallbackPattern,
                    snapshotTiming(fallbackPattern, {
                        activeSteps: state.getActiveSteps(fallbackIdx),
                        triplet: state.getTriplet(fallbackIdx),
                    }),
                    true,
                );
            }
        }
        // Empty timeline - nothing to play. Fall to single-pattern loop:
        // keep the audible pattern highlighted until the fallback queue is
        // acknowledged, while removing only the timeline cursor.
        currentTlPos = -1;
        if (!auditionMode) {
            currentDevicePatternIdx = null;
            queuedPatternIdx = null;
            currentDeviceTiming = null;
            queuedDeviceTiming = null;
        }
        highlightColumn(-1);
        return;
    }

    // activeSlots >= 1: stay in timeline mode.
    //
    // Transitioning into timeline mode with no device tracker: seed from the
    // focused card, which is the best known single-pattern fallback. If a
    // single-slot timeline already seeded the device tracker, keep it.
    if (currentDevicePatternIdx === null) {
        currentDevicePatternIdx = state.getFocusedIdx();
        queuedPatternIdx = currentDevicePatternIdx;
        seedDeviceTiming(currentDevicePatternIdx);
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

    // Queue the next slot after the audible pattern. If the previously
    // queued slot was removed, this replaces it before the current wrap.
    const queueSlot = queueSlotAfterTimelineChange(
        tl, cursor, currentDevicePatternIdx,
    );
    if (needsImmediateScratchSave(tl, queueSlot, queuedPatternIdx)
        || (auditionMode && hostAuditionQueuedForWrap)) {
        immediateScratchSave(tl, queueSlot);
    }
}

/**
 * Queue the given timeline slot's pattern immediately. LIVE UPDATE ON also
 * writes the scratch slot. No-save audition records the same queued pattern
 * so the host schedule can swap at the next wrap.
 *
 * Used whenever a structural change leaves the queued pattern different from
 * the new timeline target. Sets nextPatternSent=true to block pre-load from
 * firing a second queue operation in the current cycle.
 */
function immediateScratchSave(tl, slot) {
    const num = tl[slot];
    if (num < 1 || num > state.getPatternCount()) return;
    const patIdx = num - 1;
    const pat = state.getPattern(patIdx);
    if (!pat) return;
    const queuedTiming = snapshotTiming(pat, {
        activeSteps: state.getActiveSteps(patIdx),
        triplet: state.getTriplet(patIdx),
    });
    if (auditionMode && !state.isLiveUpdate() && state.isConnected()) {
        queueHostAuditionForNextCycle(slot, patIdx, pat, queuedTiming, true);
        return;
    }
    queuedPatternIdx = patIdx;
    nextPatternSent = true;
    if (!state.isLiveUpdate() || !state.isConnected()) {
        queuedDeviceTiming = queuedTiming;
        return;
    }
    api.savePattern(
        scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
    ).then(() => {
        queuedDeviceTiming = queuedTiming;
        setStatus(`P${patIdx + 1} scratched (timeline sync)`);
    })
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
    if (!state.isPlaying() || !state.isConnected()) return;
    if (currentTlPos < 0) return;
    const tl = state.getTimeline();
    const nextPos = nextTimelinePos(tl, currentTlPos);
    if (nextPos < 0) return;
    const nextNum = tl[nextPos];
    if (nextNum < 1) return;
    const nextPatIdx = nextNum - 1;
    const pat = state.getPattern(nextPatIdx);
    if (!pat) return;
    const queuedTiming = snapshotTiming(pat, {
        activeSteps: state.getActiveSteps(nextPatIdx),
        triplet: state.getTriplet(nextPatIdx),
    });
    if (auditionMode && !state.isLiveUpdate()) {
        queueHostAuditionForNextCycle(nextPos, nextPatIdx, pat, queuedTiming, true);
        return;
    }
    if (!state.isLiveUpdate()) return;
    queuedPatternIdx = nextPatIdx;
    api.savePattern(
        scratchSlot.group, scratchSlot.pattern, scratchSlot.side, pat,
    ).then(() => {
        queuedDeviceTiming = queuedTiming;
        setStatus(`Re-scratched P${nextPatIdx + 1}`);
    })
     .catch(err => setStatus('Re-scratch error: ' + err.message));
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

export function updateBpmDisplay(value = state.getBpm()) {
    const bpm = normalizedBpm(value);
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
