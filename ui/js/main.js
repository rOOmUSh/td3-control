// Bootstrap: wires all modules together.

import * as state from './multipattern/multipattern-state.js';
import * as multipatternList from './multipattern/multipattern-list.js';
import * as multipatternToolbar from './multipattern/multipattern-toolbar.js';
import * as multipatternViewport from './multipattern/multipattern-viewport.js';
import * as multipatternPush from './multipattern/multipattern-push.js';
import * as multipatternBank from './multipattern/multipattern-bank.js';
import * as multipatternDeviceIo from './multipattern/multipattern-device-io.js';
import * as multipatternTimeline from './multipattern/multipattern-timeline.js';
import * as multipatternPreview from './multipattern/multipattern-preview.js';
import * as multipatternReset from './multipattern/multipattern-reset.js';
import {
    buildRbsExportFilename,
    buildRbsExportPayload,
    buildSingleFileExportPlan,
    formatPatternExportTimestamp,
    PATTERN_SET_NAME_INPUT_MAX_LENGTH,
    sanitizePatternSetName,
} from './multipattern/multipattern-export.js';
import { detectImportFormat, unsupportedImportMessage } from './multipattern/multipattern-import.js';
import * as transport from './transport.js';
import * as remoteSync from './remote-sync.js';
import * as selectors from './selectors.js';
import * as randomize from './randomize.js';
import * as midiStatus from './midi-status.js';
import * as keyboard from './keyboard.js';
import * as history from './history.js';
import * as deviceBackup from './device-backup.js';
import { resolveLiveUpdateTargetIndex } from './multipattern/live-update-target.js';
import * as tripletMorphSend from './multipattern/triplet-morph-send.js';
import { api } from './api.js';
import { bankApi } from './bank/bank-api.js';
import { promptModal } from './bank/bank-modal.js';
import { subscribeControlQueue } from './shared/add-to-control.js';
import { loadAppConfig, getAppConfig, applyUiDefaults } from './app-config.js';
import { openImportBankPicker } from './import-bank-picker.js';
import { detectKey, formatKey, buildPitchClassHistogram, CONFIDENCE_HIGH } from './key-detection.js';
import { rankScales, applyRankedOrder, resetToDefaultOrder } from './scale-ranking.js';
import { getAllScales, getTagGroups } from './scales.js';
import { formatPatternAsStepsTxt } from './shared/steps-txt-format.js';
import { parseStepsTxtDocument, looksLikeStepsTxt } from './shared/steps-txt-parse.js';
import { initMidiChannelControl } from './shared/midi-channel-control.js';
import { initGateControl } from './shared/gate-control.js';
import { initTripletMorphControl } from './shared/triplet-morph-control.js';
import { initTripletMorphEndpointToggle } from './shared/triplet-morph-toggle.js';
import * as tripletMorphView from './multipattern/triplet-morph-view.js';
import { morphEditNotice } from './shared/triplet-morph-editing.js';
import { runDownloadBatches } from './shared/download-batches.js';

// Write the focused pattern to the OS clipboard as `.steps.txt` text so the
// user can paste the pattern into any text target (Notepad, chat, email).
// Kept best-effort: a missing/denied Clipboard API still leaves the
// in-memory FULL clipboard populated so PASTE FULL keeps working.
async function copyFocusedPatternToSystemClipboard() {
    try {
        if (!navigator.clipboard || !navigator.clipboard.writeText) return false;
        const focused = state.getFocusedIdx();
        if (focused === null) return false;
        const pat = state.getPattern(focused);
        if (!pat) return false;
        await navigator.clipboard.writeText(formatPatternAsStepsTxt(pat, state.getBpm()));
        return true;
    } catch (_) {
        return false;
    }
}

// Try to consume the OS clipboard as a `.steps.txt` body and paste it into
// the focused pattern. Returns true on success, false when the clipboard
// API is unavailable, access was denied, the text is not steps.txt, or the
// body is malformed - callers fall back to the in-memory FULL clipboard.
async function tryPasteFromSystemClipboard(focusedIdx) {
    try {
        if (!navigator.clipboard || !navigator.clipboard.readText) return false;
        const text = await navigator.clipboard.readText();
        if (!looksLikeStepsTxt(text)) return false;
        const { pattern, centibpm } = parseStepsTxtDocument(text);
        if (centibpm !== null) transport.applyImportedBpm(centibpm);
        state.setPattern(focusedIdx, pattern);
        return true;
    } catch (_) {
        return false;
    }
}

const statusLog = document.getElementById('status-log');
const activeStepsInput = document.getElementById('active-steps');
const tripletToggle = document.getElementById('triplet-toggle');
const tripletMorphSendToggle = document.getElementById('triplet-morph-send');
const btnReset = document.getElementById('btn-reset');
const btnLive = document.getElementById('btn-live');
const slicerInput = document.getElementById('slicer-input');
const btnSlicer = document.getElementById('btn-slicer');
const btnRandSl = document.getElementById('btn-rand-sl');
const btnRandAcc = document.getElementById('btn-rand-acc');
const btnRandRst = document.getElementById('btn-rand-rst');
const btnRandUd = document.getElementById('btn-rand-ud');
const btnKbEdit = document.getElementById('btn-kb-edit');
const btnAutoStep = document.getElementById('btn-auto-step');
const kbStepDisplay = document.getElementById('kb-step-display');
const kbHint = document.getElementById('kb-hint');
const btnShiftBack4 = document.getElementById('btn-shift-back4');
const btnShiftBack2 = document.getElementById('btn-shift-back2');
const btnShiftBack1 = document.getElementById('btn-shift-back1');
const btnShiftFwd1 = document.getElementById('btn-shift-fwd1');
const btnShiftFwd2 = document.getElementById('btn-shift-fwd2');
const btnShiftFwd4 = document.getElementById('btn-shift-fwd4');
const btnShuffleAll = document.getElementById('btn-shuffle-all');
const btnTrnspsUp   = document.getElementById('btn-trnsps-up');
const btnTrnspsDn   = document.getElementById('btn-trnsps-dn');
const btnTrnspsUp12 = document.getElementById('btn-trnsps-up12');
const btnTrnspsDn12 = document.getElementById('btn-trnsps-dn12');

// Status log (exported for other modules)
export function setStatus(msg) {
    statusLog.textContent = msg;
    console.log('[TD3]', msg);
}

// Scratch pattern - the device slot used for play/live-send.
// Loaded from server on init. Load/save use the sidebar-selected slot instead.
let scratch = { group: 1, pattern: 1, side: 'A' };

// Debounced live-update save - always writes to scratch slot
let liveTimer = null;
function cancelLiveSave() {
    if (liveTimer) {
        clearTimeout(liveTimer);
        liveTimer = null;
    }
}

function liveUpdateTargetIndex() {
    return resolveLiveUpdateTargetIndex(
        state.getCheckedArray(),
        state.getFocusedIdx(),
        state.getPatternCount(),
    );
}

function hostAuditionIsActive() {
    return transport.isAuditionActive() || multipatternPreview.isAuditionActive();
}

async function saveLivePatternNow(statusPrefix = 'Live sent') {
    const patIdx = liveUpdateTargetIndex();
    if (patIdx < 0) {
        setStatus('Live update ON: no pattern to send');
        return false;
    }
    if (!state.isConnected()) {
        setStatus('Live update ON: connect MIDI to send scratch');
        return false;
    }
    const pat = state.getPattern(patIdx);
    if (!pat) {
        setStatus('Live update ON: no pattern to send');
        return false;
    }
    const sentTiming = {
        active_steps: pat.active_steps,
        triplet: pat.triplet,
    };
    await api.savePattern(
        scratch.group, scratch.pattern, scratch.side,
        pat,
    );
    transport.noteLiveScratchPatternQueued(patIdx, sentTiming);
    setStatus(`${statusPrefix} P${patIdx + 1} to ${scratch.label}`);
    return true;
}

function scheduleLiveSave() {
    if (!state.isLiveUpdate() || !state.isConnected() || hostAuditionIsActive()) return;
    cancelLiveSave();
    liveTimer = setTimeout(async () => {
        liveTimer = null;
        if (!state.isLiveUpdate() || !state.isConnected() || hostAuditionIsActive()) return;
        try {
            await saveLivePatternNow('Live sent');
        } catch (err) {
            setStatus('Live error: ' + err.message);
        }
    }, 150);
}

// Latest-wins guard for the LIVE ON sequence: a rapid double click
// supersedes the older transition so a stale morph audition can never be
// re-enabled after LIVE becomes ON.
let liveToggleGeneration = 0;
let liveTransitionTarget = null;

async function toggleLiveUpdate() {
    const current = liveTransitionTarget ?? state.isLiveUpdate();
    const next = !current;
    liveTransitionTarget = next;
    const generation = ++liveToggleGeneration;
    if (!next) {
        state.setLiveUpdate(false);
        cancelLiveSave();
        setStatus('Live update OFF');
        if (generation === liveToggleGeneration) liveTransitionTarget = null;
        return;
    }

    // LIVE ON with a morph audition possibly running: stop and silence
    // host audition, restore the canonical view, reset the morph amount,
    // and only then allow normal LIVE behavior.
    try {
        await multipatternPreview.stop();
        await transport.stopPlaybackForModeChange();
        if (state.isTripletMorphActive()) {
            try { await api.auditionStop(); } catch (_) { /* already idle */ }
        }
        if (generation !== liveToggleGeneration) return;
        state.resetTripletMorphSession();
        state.setLiveUpdate(true);
        await saveLivePatternNow('Live update ON, sent');
    } catch (err) {
        setStatus('Live update ON, send error: ' + err.message);
    } finally {
        if (generation === liveToggleGeneration) liveTransitionTarget = null;
    }
}

// Update the LIVE button appearance
function updateLiveBtn() {
    btnLive.classList.toggle('is-active', state.isLiveUpdate());
    gateControl.render();
    midiChannelControl.render();
    tripletMorphControl.render();
    tripletMorphEndpointToggle.render();
}

const gateControl = initGateControl({
    getValue: () => state.getGatePercent(),
    setValue: (value) => state.setGatePercent(value),
    isVisible: () => !state.isLiveUpdate(),
    onValueChange: () => {
        transport.syncAuditionPattern();
        multipatternPreview.syncActiveAudition();
    },
});

// The device only sounds channel-voice messages on its own channel, and
// only host audition sends those, so the selector follows the GATE knob
// in appearing when Live Update is off. A change takes effect on the
// next audition request; nothing is restarted.
const midiChannelControl = initMidiChannelControl({
    getValue: () => state.getMidiChannel(),
    setValue: (value) => state.setMidiChannel(value),
    isVisible: () => !state.isLiveUpdate(),
    onValueChange: () => {
        transport.syncAuditionPattern();
        multipatternPreview.syncActiveAudition();
    },
});


// Shared by the TRIPLET knob and its endpoint toggle: apply the amount
// with one refusal message, and resync audition on any real change.
function setTripletMorphAmount(value) {
    const before = state.getTripletMorphPercent();
    state.setTripletMorphPercent(value);
    if (state.getTripletMorphPercent() === before
        && Number(value) > 0
        && !state.isTripletMorphSourceEligible()) {
        setStatus('TRIPLET morph needs 16-step straight patterns');
    }
}

function onTripletMorphAmountChange(value) {
    if (value > 0) {
        setStatus('Triplet audition is derived. Return TRIPLET to 0 to edit.');
    } else {
        setStatus('TRIPLET morph off - canonical view restored');
    }
    transport.syncAuditionPattern();
    multipatternPreview.syncActiveAudition();
}

const tripletMorphControl = initTripletMorphControl({
    getValue: () => state.getTripletMorphPercent(),
    setValue: setTripletMorphAmount,
    isVisible: () => !state.isLiveUpdate(),
    onValueChange: onTripletMorphAmountChange,
});

const tripletMorphEndpointToggle = initTripletMorphEndpointToggle({
    getValue: () => state.getTripletMorphPercent(),
    setValue: setTripletMorphAmount,
    onValueChange: onTripletMorphAmountChange,
});

// Editing gate across the morph range. At 0 everything is allowed. At
// the 100 endpoint, per-step edits and randomizers are allowed and
// restrict themselves to the surviving notes. Between 1 and 99 the
// positions are mid-transform and nothing may change.
//
// Bulk operations that move or replace whole patterns (shift, transpose,
// reset, add, delete, import, undo, redo, paste, active steps, native
// triplet) stay blocked at the endpoint: they would rewrite the losing
// notes too and scramble the derived mapping.
function canonicalEditBlocked({ allowedAtEndpoint = false } = {}) {
    const amount = state.getTripletMorphPercent();
    if (amount === 0) return false;
    if (amount >= 100 && allowedAtEndpoint) return false;
    const notice = morphEditNotice(amount);
    if (notice) setStatus(notice);
    return true;
}

// Update slicer button appearance
function updateSlicerBtn() {
    const enabled = state.isSliceEnabled();
    btnSlicer.textContent = enabled ? 'ON' : 'OFF';
    btnSlicer.classList.toggle('is-active', enabled);
}

// Update keyboard edit toggle appearance
function updateKbToggles() {
    const kbEnabled = state.isKbEditEnabled();
    btnKbEdit.classList.toggle('is-active', kbEnabled);
    if (kbEnabled) {
        kbStepDisplay.classList.remove('opacity-0');
        kbStepDisplay.classList.add('opacity-60');
        kbHint.classList.remove('opacity-0');
        kbHint.classList.add('opacity-40');
    } else {
        kbStepDisplay.classList.add('opacity-0');
        kbStepDisplay.classList.remove('opacity-60');
        kbHint.classList.add('opacity-0');
        kbHint.classList.remove('opacity-40');
    }
    btnAutoStep.classList.toggle('is-active', state.isAutoStepFwd());
    kbStepDisplay.textContent = 'STEP: ' + String(state.getSelectedStep() + 1).padStart(2, '0');
}

// --- Undo/Redo with debounce ---

let historyDebounce = null;
let isRestoring = false;

function recordHistory() {
    if (isRestoring) return;
    clearTimeout(historyDebounce);
    historyDebounce = setTimeout(() => {
        history.push('multipattern', state.getSnapshot());
    }, 300);
}

// Re-render the multipattern list on any state change. The list module owns
// its own onChange subscription for the card DOM; this handler keeps the
// chrome (STEPS input, toggle LEDs, transport callbacks, history recording)
// in sync with state.
state.onChange((patternChanged) => {
    // Global STEPS shows the longest active_steps across all patterns -
    // so it acts as an upper-bound indicator. Per-card inputs let the
    // user shorten individual patterns; bumping the global re-applies
    // its value to every card (handled by the change/wheel handlers).
    activeStepsInput.value = state.getMaxActiveSteps();
    updateLiveBtn();
    updateSlicerBtn();
    updateKbToggles();
//    updateBankDisplay();
    if (tripletMorphSendToggle) tripletMorphSendToggle.checked = state.isTripletMorphSend();
    const tripletAllOn = isTripletAllOnForTargets();
    tripletToggle.textContent = tripletAllOn ? 'ON' : 'OFF';
    tripletToggle.classList.toggle('is-active', tripletAllOn);
    if (patternChanged) {
        transport.syncAuditionPattern();
        multipatternPreview.syncActiveAudition();
        scheduleLiveSave();
        recordHistory();
    }
});

// --- Ctrl+Z / Ctrl+Y / Ctrl+C / Ctrl+V ---
//
// Clipboard chords live here (not in keyboard.js) because keyboard.js
// gates all its handlers on `isKbEditEnabled`, which would silently break
// copy/paste when the user isn't in step-edit mode. We ignore the chord
// whenever focus is inside an input/textarea/select so typing in the
// STEPS field / slicer field / any search box keeps working.

function inEditableTarget(e) {
    const tag = e.target && e.target.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
        || (e.target && e.target.isContentEditable);
}

document.addEventListener('keydown', async (e) => {
    if (!e.ctrlKey && !e.metaKey) return;
    const k = e.key.toLowerCase();

    if (k === 'z' && !e.shiftKey) {
        e.preventDefault();
        if (canonicalEditBlocked()) return;
        const snap = await history.undo('multipattern');
        if (snap) {
            isRestoring = true;
            state.restoreSnapshot(snap);
            isRestoring = false;
            scheduleLiveSave();
            setStatus('Undo');
        } else {
            setStatus('Nothing to undo');
        }
    } else if (k === 'y' || (k === 'z' && e.shiftKey)) {
        e.preventDefault();
        if (canonicalEditBlocked()) return;
        const snap = await history.redo('multipattern');
        if (snap) {
            isRestoring = true;
            state.restoreSnapshot(snap);
            isRestoring = false;
            scheduleLiveSave();
            setStatus('Redo');
        } else {
            setStatus('Nothing to redo');
        }
    } else if (k === 'c' && !e.shiftKey && !e.altKey) {
        // Ctrl+C copies the focused pattern. Ignore when focus is inside
        // an input so the browser's native copy keeps working.
        if (inEditableTarget(e)) return;
        e.preventDefault();
        const cur = state.getFocusedIdx();
        if (cur === null) { setStatus('Nothing focused to copy'); return; }
        if (state.copyFocused()) {
            const ok = await copyFocusedPatternToSystemClipboard();
            setStatus(ok ? `Copied P${cur + 1} (clipboard + system)` : `Copied P${cur + 1}`);
        }
    } else if (k === 'v' && !e.shiftKey && !e.altKey) {
        if (inEditableTarget(e)) return;
        e.preventDefault();
        if (canonicalEditBlocked()) return;
        const cur = state.getFocusedIdx();
        if (cur === null) { setStatus('Nothing focused to paste into'); return; }
        // Prefer the OS clipboard when it holds a valid .steps.txt body so
        // the user can paste from Notepad / chat. Fall back silently to the
        // in-memory FULL clipboard when the OS text isn't steps.txt or the
        // Clipboard API is unavailable / denied.
        if (await tryPasteFromSystemClipboard(cur)) {
            setStatus(`Pasted → P${cur + 1} (from text)`);
            return;
        }
        if (!state.hasClipboard()) { setStatus('Clipboard empty'); return; }
        if (state.pasteIntoFocused()) setStatus(`Pasted → P${cur + 1}`);
    }
});

// Active steps input - global "apply to all" semantics: typing or scrolling
// here overwrites every per-pattern active_steps with the new value. The
// user accepts the conflict-resolution rule (bump global → all patterns
// follow; ctrl-z to revert if it wasn't intended).
activeStepsInput.addEventListener('change', () => {
    if (canonicalEditBlocked()) return;
    state.setAllActiveSteps(parseInt(activeStepsInput.value) || 16);
});

// Scroll wheel over the STEPS input nudges the count by 1 (clamped to
// 1..=16) and applies it to every pattern. preventDefault keeps the page
// from scrolling while the pointer sits over the input.
activeStepsInput.addEventListener('wheel', (e) => {
    e.preventDefault();
    if (canonicalEditBlocked()) return;
    const cur = state.getMaxActiveSteps();
    // deltaY < 0 → wheel up → increase steps.
    const delta = e.deltaY < 0 ? 1 : -1;
    const next = Math.max(1, Math.min(16, cur + delta));
    if (next !== cur) state.setAllActiveSteps(next);
}, { passive: false });

// Triplet toggle - bulk semantics matching SHIFT/TRNSPS:
//   ≥1 checked → toggle just those, else → toggle every pattern.
// Display reflects the aggregate: ON only when every target is ON;
// any mixed/all-OFF state shows OFF, so a click flips the herd to ON.
// Both TRIPLET buttons - the global one here and the per-pattern one in
// the row module - route through this so morph send behaves identically
// whichever is pressed. Each caller supplies its own `useMorph`: the
// global checkbox for the global button, the row checkbox for its row,
// so one pattern can be projected while its neighbours are not.
//
// Switching off always tries to restore first, whatever the checkbox
// says now: a projection is undone because this session made it, not
// because a mode happens to be set.
async function applyTripletPress(targets, next, useMorph) {
    if (!next) {
        const restored = tripletMorphSend.restoreProjectedSources(state, targets);
        const remaining = targets.filter((index) => !restored.includes(index));
        if (remaining.length > 0) state.setTripletBulk(remaining, false);
        return { restored, morphed: [], skipped: [] };
    }
    if (!useMorph) {
        state.setTripletBulk(targets, true);
        return { restored: [], morphed: [], skipped: [] };
    }
    const { morphed, skipped } = await tripletMorphSend.applyEndpointProjection(state, targets);
    // An ineligible source or an unavailable plan still gets the press it
    // was given, as the plain triplet flag.
    if (skipped.length > 0) state.setTripletBulk(skipped, true);
    return { restored: [], morphed, skipped };
}

function tripletPressLabel(next, outcome) {
    if (!next) {
        return outcome.restored.length > 0
            ? `Triplet OFF, ${outcome.restored.length} restored to 16 steps`
            : 'Triplet OFF';
    }
    if (outcome.morphed.length > 0 && outcome.skipped.length > 0) {
        return `Triplet ON, ${outcome.morphed.length} morphed, ${outcome.skipped.length} flag only`;
    }
    if (outcome.morphed.length > 0) return 'Triplet ON, morphed to 12 steps';
    return 'Triplet ON';
}

export async function pressTripletFor(indices, next, useMorph) {
    return applyTripletPress(indices, next, useMorph);
}

tripletToggle.addEventListener('click', async () => {
    if (canonicalEditBlocked()) return;
    const targets = bulkTargets();
    if (targets.length === 0) return;
    const next = !targets.every((i) => state.getTriplet(i));
    const outcome = await applyTripletPress(targets, next, state.isTripletMorphSend());
    if (remoteSync.isEnabled()) {
        remoteSync.relayTriplet(next)
            .catch(err => setStatus('Remote triplet error: ' + err.message));
    }
    setStatus(bulkLabel(tripletPressLabel(next, outcome)));
});

tripletMorphSendToggle?.addEventListener('change', () => {
    state.setTripletMorphSend(!!tripletMorphSendToggle.checked);
    setStatus(`Triplet morph send ${tripletMorphSendToggle.checked ? 'ON' : 'OFF'}`);
});

function isTripletAllOnForTargets() {
    const targets = bulkTargets();
    if (targets.length === 0) return false;
    return targets.every((i) => state.getTriplet(i));
}

// Live update toggle
btnLive.addEventListener('click', () => {
    toggleLiveUpdate();
});

// Slicer toggle
btnSlicer.addEventListener('click', () => {
    state.setSliceEnabled(!state.isSliceEnabled());
    setStatus(state.isSliceEnabled() ? 'Slicer ON' : 'Slicer OFF');
});
slicerInput.addEventListener('input', () => {
    state.setSliceText(slicerInput.value);
});

// RST / SL / AC - direct-action randomizers. Each click shuffles only
// its attribute family on the current pattern using the configured slider
// percentage and the current slicer window.
btnRandRst.addEventListener('click', () => { if (canonicalEditBlocked({ allowedAtEndpoint: true })) return; randomize.randomizeCategory('rst'); setStatus('Randomized rests'); });
btnRandSl.addEventListener('click',  () => { if (canonicalEditBlocked({ allowedAtEndpoint: true })) return; randomize.randomizeCategory('sl');  setStatus('Randomized slides'); });
btnRandAcc.addEventListener('click', () => { if (canonicalEditBlocked({ allowedAtEndpoint: true })) return; randomize.randomizeCategory('ac');  setStatus('Randomized accents'); });
if (btnRandUd) {
    btnRandUd.addEventListener('click', () => { if (canonicalEditBlocked({ allowedAtEndpoint: true })) return; randomize.randomizeCategory('ud'); setStatus('Randomized UP/DOWN'); });
}

// Shift steps - toolbar bulk: ≥1 checked → just those, else ALL patterns.
// Per-card SHIFT buttons (in multipattern-row) keep their single-pattern
// semantics; the toolbar deliberately skips a "focused only" path because
// each card already has its own SHIFT.
function bulkTargets() {
    const checked = state.getCheckedArray();
    return checked.length > 0 ? checked : state.getAllIndexes();
}
function bulkLabel(suffix) {
    const checked = state.getCheckedSet().size;
    return checked > 0 ? `${suffix} (${checked} checked)` : `${suffix} (all)`;
}
btnShiftBack4.addEventListener('click', () => { if (canonicalEditBlocked()) return; state.shiftStepsBulk(bulkTargets(), -4); setStatus(bulkLabel('Shifted back 4')); });
btnShiftBack2.addEventListener('click', () => { if (canonicalEditBlocked()) return; state.shiftStepsBulk(bulkTargets(), -2); setStatus(bulkLabel('Shifted back 2')); });
btnShiftBack1.addEventListener('click', () => { if (canonicalEditBlocked()) return; state.shiftStepsBulk(bulkTargets(), -1); setStatus(bulkLabel('Shifted back 1')); });
btnShiftFwd1.addEventListener('click',  () => { if (canonicalEditBlocked()) return; state.shiftStepsBulk(bulkTargets(),  1); setStatus(bulkLabel('Shifted forward 1')); });
btnShiftFwd2.addEventListener('click',  () => { if (canonicalEditBlocked()) return; state.shiftStepsBulk(bulkTargets(),  2); setStatus(bulkLabel('Shifted forward 2')); });
btnShiftFwd4.addEventListener('click',  () => { if (canonicalEditBlocked()) return; state.shiftStepsBulk(bulkTargets(),  4); setStatus(bulkLabel('Shifted forward 4')); });
if (btnShuffleAll) {
    btnShuffleAll.addEventListener('click', () => {
        if (canonicalEditBlocked()) return;
        state.shuffleStepsBulk(bulkTargets());
        setStatus(bulkLabel('Shuffled steps'));
    });
}

// Transpose ±1 / ±12 semitones - mutates step.note only, preserves
// step.transpose. Same checked-or-all semantics as SHIFT.
btnTrnspsUp.addEventListener('click',   () => { if (canonicalEditBlocked()) return; state.transposeBulk(bulkTargets(), +1);  setStatus(bulkLabel('Transposed +1')); });
btnTrnspsDn.addEventListener('click',   () => { if (canonicalEditBlocked()) return; state.transposeBulk(bulkTargets(), -1);  setStatus(bulkLabel('Transposed −1')); });
btnTrnspsUp12.addEventListener('click', () => { if (canonicalEditBlocked()) return; state.transposeBulk(bulkTargets(), +12); setStatus(bulkLabel('Transposed +12')); });
btnTrnspsDn12.addEventListener('click', () => { if (canonicalEditBlocked()) return; state.transposeBulk(bulkTargets(), -12); setStatus(bulkLabel('Transposed −12')); });

// Keyboard edit toggles
btnKbEdit.addEventListener('click', () => {
    state.setKbEditEnabled(!state.isKbEditEnabled());
    setStatus(state.isKbEditEnabled() ? 'Keyboard edit ON' : 'Keyboard edit OFF');
});
btnAutoStep.addEventListener('click', () => {
    state.setAutoStepFwd(!state.isAutoStepFwd());
    setStatus(state.isAutoStepFwd() ? 'Auto-step forward ON' : 'Auto-step forward OFF');
});

// Bank size

// RESET button uses checked patterns when checks are active, otherwise the
// full pattern list.
btnReset.addEventListener('click', () => {
    if (canonicalEditBlocked()) return;
    const result = multipatternReset.resetCheckedOrAll(state);
    setStatus(result.mode === 'all'
        ? 'All patterns reset'
        : `Reset ${result.count} checked pattern${result.count === 1 ? '' : 's'}`);
});

function syncResetButtonChrome() {
    const checkedCount = state.getCheckedSet().size;
    btnReset.textContent = multipatternReset.resetToolbarLabel(checkedCount);
    btnReset.title = multipatternReset.resetToolbarTitle(checkedCount);
}
state.onChange(syncResetButtonChrome);
syncResetButtonChrome();

// SEND TO PROGRESSION - write the current single pattern + sidebar-selected
// root/scale into a one-shot sessionStorage handoff and navigate to the
// progression page. The progression page's init reads the handoff, installs
// P1 verbatim, and derives P2..P4 via the shared sibling chain.
const btnSendToProgression = document.getElementById('btn-send-to-progression');
if (btnSendToProgression) {
    btnSendToProgression.addEventListener('click', () => {
        const rootSelect = document.getElementById('root-select');
        const scaleSelect = document.getElementById('scale-select');
        const root = rootSelect ? parseInt(rootSelect.value) : 0;
        const scale = scaleSelect ? scaleSelect.value : '';
        try {
            sessionStorage.setItem('td3_progression_handoff', JSON.stringify({
                p1: state.getPattern(),
                root: Number.isFinite(root) ? root : 0,
                scale,
                sentAt: Date.now(),
            }));
        } catch (err) {
            setStatus('Send failed: ' + err.message);
            return;
        }
        window.location.href = '/progression.html';
    });
}

// Detection chip - persistent visual of the last key detection under the
// sidebar RANDOMIZER heading. High confidence renders green, low renders
// amber. Auto-clears when the user manually changes either select.
const detectionChip = document.getElementById('detection-chip');
const detectionChipLabel = document.getElementById('detection-chip-label');
const detectionChipDismiss = document.getElementById('detection-chip-dismiss');

const CHIP_HIGH_CLASSES = ['bg-green-900/40', 'text-green-300', 'border-green-700'];
const CHIP_LOW_CLASSES = ['bg-amber-900/40', 'text-amber-300', 'border-amber-700'];

function hideDetectionChip() {
    if (!detectionChip) return;
    detectionChip.classList.add('hidden');
    detectionChip.classList.remove('flex', ...CHIP_HIGH_CLASSES, ...CHIP_LOW_CLASSES);
    // Chip dismissal also reverts the scale-select to its default tag-group
    // order - the ranked view is tied to the detection, so once the user
    // dismisses, the "near-to-key" optgroup would misrepresent their state.
    const scaleSelect = document.getElementById('scale-select');
    if (scaleSelect) {
        resetToDefaultOrder(scaleSelect, {
            tagGroups: getTagGroups(), allScales: getAllScales(),
        });
    }
}

function showDetectionChip(detection) {
    if (!detectionChip || !detectionChipLabel || !detection) return;
    const label = formatKey(detection);
    const lowConf = detection.confidence < CONFIDENCE_HIGH;
    const confText = lowConf ? 'low conf' : 'high conf';
    detectionChipLabel.textContent = `Detected: ${label} · ${confText}`;
    detectionChip.title = `confidence: ${detection.confidence.toFixed(3)} · notes analyzed: ${detection.noteCount}`;
    detectionChip.classList.remove('hidden', ...CHIP_HIGH_CLASSES, ...CHIP_LOW_CLASSES);
    detectionChip.classList.add('flex', ...(lowConf ? CHIP_LOW_CLASSES : CHIP_HIGH_CLASSES));
}

if (detectionChipDismiss) {
    detectionChipDismiss.addEventListener('click', hideDetectionChip);
}
// When the user manually overrides either select, the chip no longer
// reflects their active choice - hide it to avoid misleading visual state.
// Programmatic .value = … writes in applyKeyDetection don't fire change.
const rootSelectEl = document.getElementById('root-select');
const scaleSelectEl = document.getElementById('scale-select');
if (rootSelectEl) rootSelectEl.addEventListener('change', hideDetectionChip);
if (scaleSelectEl) scaleSelectEl.addEventListener('change', hideDetectionChip);

// Auto-populate the sidebar root/scale selects from a Temperley-profile
// detection on the imported pattern. Advisory only - the user can override
// before hitting SEND TO PROGRESSION. Returns a short status suffix
// describing what happened, so the caller can fold it into its own message.
function applyKeyDetection(pattern) {
    const rootSelect = document.getElementById('root-select');
    const scaleSelect = document.getElementById('scale-select');
    if (!rootSelect || !scaleSelect) return '';
    const detection = detectKey(pattern);
    if (!detection) { hideDetectionChip(); return ''; }
    rootSelect.value = String(detection.root);
    // Rank every scale against the pattern at the detected root and rebuild
    // the scale-select so the nearest fits (top 5) sit in a dedicated
    // optgroup at the top. The detected scale appears there alongside close
    // alternatives (pentatonics, dorian, etc.) for quick auditioning.
    const hist = buildPitchClassHistogram(pattern);
    const ranked = rankScales({ scales: getAllScales(), hist, root: detection.root });
    applyRankedOrder(scaleSelect, {
        ranked, topN: 5,
        tagGroups: getTagGroups(), allScales: getAllScales(),
    });
    // Only assign scaleId if the select actually knows it (defensive against
    // config drift). natural_minor + major are always present in scales-config.
    const hasScale = [...scaleSelect.options].some(o => o.value === detection.scaleId);
    if (hasScale) scaleSelect.value = detection.scaleId;
    showDetectionChip(detection);
    const label = formatKey(detection);
    const lowConf = detection.confidence < CONFIDENCE_HIGH;
    return lowConf ? ` - detected ${label} (low confidence)` : ` - detected ${label}`;
}

// Import pattern from file
const btnImport = document.getElementById('btn-import');
const fileImport = document.getElementById('file-import');

btnImport.addEventListener('click', () => {
    if (canonicalEditBlocked()) return;
    fileImport.click();
});

fileImport.addEventListener('change', async () => {
    const files = Array.from(fileImport.files || []);
    if (files.length === 0) return;

    const jobs = files.map((file) => ({ file, info: detectImportFormat(file.name) }));
    const unsupported = jobs.find(job => job.info.error);
    if (unsupported) {
        setStatus(`${unsupportedImportMessage()}: ${unsupported.file.name}`);
        fileImport.value = '';
        return;
    }

    try {
        let imported = 0;
        let firstPattern = null;
        let capHit = false;

        for (let fileIndex = 0; fileIndex < jobs.length; fileIndex++) {
            const { file, info } = jobs[fileIndex];
            const fmt = info.format;
            if (info.bank) {
                // Bank files (.sqs/.rbs) may hold up to 64 patterns. Use the
                // multi-select picker so each chosen pattern is appended at
                // the end of the multipattern list.
                setStatus(`Parsing ${file.name} (${fileIndex + 1}/${jobs.length})...`);
                const buf = await file.arrayBuffer();
                const bytes = Array.from(new Uint8Array(buf));
                const res = await api.parsePatternBank(bytes, fmt);
                await openImportBankPicker({
                    slots: res.slots,
                    title: `Import from ${file.name}`,
                    multi: true,
                    onImport: (patterns) => {
                        if (!Array.isArray(patterns) || patterns.length === 0) return;
                        let firstIdx = null;
                        let appended = 0;
                        for (const pat of patterns) {
                            const idx = state.appendPattern(pat);
                            if (idx == null) {
                                capHit = true;
                                break;
                            }
                            if (firstIdx === null) firstIdx = idx;
                            if (!firstPattern) firstPattern = pat;
                            appended++;
                            imported++;
                        }
                        if (firstIdx !== null) state.setFocused(firstIdx);
                        setStatus(`Imported ${appended} from ${file.name}`);
                    },
                });
            } else {
                setStatus(`Importing ${file.name} (${fileIndex + 1}/${jobs.length})...`);
                const payload = { format: fmt };
                if (info.binary) {
                    const buf = await file.arrayBuffer();
                    payload.bytes = Array.from(new Uint8Array(buf));
                } else {
                    payload.content = await file.text();
                }
                const res = await api.importPattern(payload);
                if (res.centibpm !== null && res.centibpm !== undefined) {
                    transport.applyImportedBpm(res.centibpm);
                }
                const idx = state.appendPattern(res.pattern);
                if (idx == null) {
                    capHit = true;
                    break;
                } else {
                    if (!firstPattern) firstPattern = res.pattern;
                    imported++;
                    setStatus(`Imported ${file.name}`);
                }
            }
            if (capHit) break;
        }

        const keyNote = firstPattern ? applyKeyDetection(firstPattern) : '';
        if (imported === 0 && capHit) {
            setStatus('Cannot import: 64-pattern cap reached');
        } else if (imported === 0) {
            setStatus('No patterns imported');
        } else if (capHit) {
            setStatus(`Imported ${imported} pattern${imported === 1 ? '' : 's'} (64-pattern cap reached)${keyNote}`);
        } else {
            setStatus(`Imported ${imported} pattern${imported === 1 ? '' : 's'} from ${jobs.length} file${jobs.length === 1 ? '' : 's'}${keyNote}`);
        }
    } catch (err) {
        setStatus('Import error: ' + err.message);
    }
    fileImport.value = '';
});

function patternExportNamePromptEnabled() {
    const value = getAppConfig()?.uiPatternExportNamePrompt;
    return value === undefined || value === null ? true : Boolean(value);
}

function patternExportBatchDelayMs() {
    const value = Number(getAppConfig()?.uiPatternExportBatchDelayMs);
    return Number.isInteger(value) && value >= 0 ? value : 2000;
}

// Export pattern - dropdown delegates per-format click to /api/pattern/export
// and triggers browser downloads. Each export captures one shared filename
// component so every selected pattern remains visibly part of the same set.
const exportPanel = document.getElementById('export-format-panel');
if (exportPanel) {
    exportPanel.addEventListener('click', async (ev) => {
        const btn = ev.target.closest('button[data-format]');
        if (!btn) return;
        const format = btn.dataset.format;
        const ext = btn.dataset.ext || format;
        const clickedAt = formatPatternExportTimestamp();
        try {
            let patternSetName = clickedAt;
            if (patternExportNamePromptEnabled()) {
                const enteredName = await promptModal({
                    title: 'Export Patterns',
                    label: 'Pattern set name',
                    placeholder: 'Enter a pattern set name',
                    okLabel: 'Export',
                    cancelLabel: 'Cancel',
                    inputId: 'pattern-export-name',
                    inputMaxLength: PATTERN_SET_NAME_INPUT_MAX_LENGTH,
                    cancelDanger: true,
                    cancelClassName: 'td3-toolbar-btn td3-toolbar-btn--secondary tactile-button',
                    okClassName: 'td3-toolbar-btn td3-toolbar-btn--primary pattern-export-submit tactile-button',
                    noScrim: false,
                    isValueAllowed: value => sanitizePatternSetName(value).length > 0,
                });
                if (enteredName === null) return;
                patternSetName = sanitizePatternSetName(enteredName);
            }

            setStatus(`Exporting ${format}...`);
            if (format === 'rbs') {
                const exportData = buildRbsExportPayload(
                    state.getPatterns(),
                    state.getCheckedArray(),
                    state.getAbMode(),
                );
                if (exportData.error) {
                    throw new Error(`RBS export failed: ${exportData.error}`);
                }
                const blob = await api.exportPattern(exportData.payload.pattern, format, {
                    patterns: exportData.payload.patterns,
                    rbs_mode: exportData.payload.rbs_mode,
                });
                const filename = buildRbsExportFilename(patternSetName, ext);
                downloadBlob(blob, filename);
                setStatus(`Exported ${filename}`);
            } else {
                const exportPlan = buildSingleFileExportPlan(
                    state.getPatterns(),
                    state.getCheckedArray(),
                    ext,
                    patternSetName,
                );
                if (exportPlan.error) {
                    throw new Error(`Export failed: ${exportPlan.error}`);
                }
                await runDownloadBatches(exportPlan.files, async (file) => {
                    const blob = await api.exportPattern(file.pattern, format, {
                        centibpm: Math.round(state.getBpm() * 100),
                    });
                    downloadBlob(blob, file.filename);
                }, patternExportBatchDelayMs());
                setStatus(exportPlan.count === 1
                    ? `Exported ${exportPlan.files[0].filename}`
                    : `Exported ${exportPlan.count} ${format} files`);
            }
        } catch (err) {
            setStatus('Export error: ' + err.message);
        }
    });
}

function downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
}

// LOAD / LOAD ALL / SAVE are owned by multipattern-device-io.js.
// The old single-pattern handlers read/wrote only the focused card's slot -
// the new module respects the selection model and adds LOAD ALL
//. See the module header for the full semantic contract.

// --- Backup status display ---

const backupStatus = document.getElementById('backup-status');

function showBackupStatus(success) {
    if (!backupStatus) return;
    backupStatus.textContent = success
        ? 'Device bank stored successfully'
        : 'Device backup incomplete';
    backupStatus.className = success
        ? 'text-[0.9rem] font-black tracking-wider text-[#4e8c45]'
        : 'text-[0.9rem] font-black tracking-wider text-[#dc143c]';
    setTimeout(() => { backupStatus.textContent = ''; }, 8000);
}

// Fetch env-driven UI defaults from server (best-effort). Stamp the defaults
// into the DOM and state module BEFORE other modules initialise so
// randomize.init/selectors.init see the env values instead of the original
// HTML placeholders, and so the first sequencer.render() paints with the
// env-driven BPM/triplet/live-update values.
const envCfg = await loadAppConfig();
applyUiDefaults(envCfg);
state.setDefaultsFromEnv(envCfg);

// Fetch scratch pattern from server
try {
    const s = await api.getScratchPattern();
    scratch.group = s.patgroup;
    scratch.pattern = s.pattern;
    scratch.side = s.side;
    scratch.label = s.label;
    const scratchEl = document.getElementById('scratch-label');
    if (scratchEl) scratchEl.textContent = 'SCRATCH ' + s.label;
    // Mirror into state so card badges + slotFor(idx) pick the scratch slot
    // up without reaching back into this module (see multipattern-row.js).
    state.setScratchSlot({
        group: scratch.group,
        pattern: scratch.pattern,
        side: scratch.side,
        label: scratch.label,
    });
} catch (err) {
    setStatus('Failed to fetch scratch pattern: ' + err.message);
}

// Init all modules
await history.open();
await history.initCursor('multipattern');
history.push('multipattern', state.getSnapshot());

// Preview controller needs scratch + setStatus before any card renders so
// the first paint can read the correct active-preview state (none at boot
// but harmless) and future clicks route through it.
multipatternPreview.init(setStatus, scratch);

// Build the multipattern card list + subscribe to state changes before any
// other init calls so the first notify() paints into a populated DOM.
multipatternList.init({
    onTripletPress: (idx, next, useMorph) => applyTripletPress([idx], next, useMorph),
    setStatus,
    onBankPattern: (idx) => multipatternBank.openSingleToBank(idx),
    // Drag-to-reorder during timeline playback - re-queue the new next
    // pattern into scratch so the device wraps into the right buffer.
    onStructuralChange: () => transport.rescratchUpcoming(),
    applyImportedBpm: (centibpm) => transport.applyImportedBpm(centibpm),
});
// Derived morph renderer subscribes after the list so its transforms are
// re-applied over every fresh card DOM.
tripletMorphView.init({ documentRef: document });
multipatternToolbar.init({ setStatus });
multipatternViewport.init({ setStatus });
multipatternBank.init({ state, bankApi, setStatus });
multipatternPush.init({ state, api, bankApi, setStatus });
multipatternDeviceIo.init({ state, api, setStatus });

selectors.init(state);
selectors.setScratch(scratch.group, scratch.pattern, scratch.side);
multipatternTimeline.init();
transport.init(setStatus, scratch);
await randomize.init();
deviceBackup.init(setStatus, showBackupStatus);
midiStatus.init(state, setStatus, async () => {
    // Mode switch: send current pattern to scratch slot so play starts correctly
    try {
        await api.savePattern(
            scratch.group, scratch.pattern, scratch.side,
            state.getPattern()
        );
        setStatus('Pattern sent to ' + scratch.label);
    } catch (err) {
        setStatus('Send error: ' + err.message);
    }
}, { autoConnect: !!envCfg && !!envCfg.uiAutoConnectToMidi });
keyboard.init(
    setStatus,
    () => document.getElementById('btn-randomize').click(),
    () => document.getElementById('btn-play').click(),
    toggleLiveUpdate,
);

// Initial chrome paint - the card list already rendered via
// multipatternList.init() above.
updateLiveBtn();
updateSlicerBtn();
updateKbToggles();
// updateBankDisplay();
slicerInput.value = state.getSliceText();
setStatus('Ready');

// Add-to-Control handoff. Drains the server-side queue once on boot, and
// subscribes to the BroadcastChannel so a Bank tab in the same browser can
// push new patterns into this canvas live without a reload. Each consume
// is atomic (the server clears the queue on GET), so concurrent boot +
// broadcast can't double-append.
async function drainControlQueue() {
    let res;
    try {
        res = await api.consumeControlQueue();
    } catch (err) {
        console.warn('[control-queue] consume failed:', err);
        return;
    }
    const incoming = (res && Array.isArray(res.patterns)) ? res.patterns : [];
    if (incoming.length === 0) return;
    let appended = 0;
    let dropped = 0;
    for (const pat of incoming) {
        const idx = state.appendPattern(pat);
        if (idx === null) dropped++;
        else appended++;
    }
    if (appended > 0) {
        const parts = [`Added ${appended} pattern${appended === 1 ? '' : 's'} from Bank`];
        if (dropped > 0) parts.push(`${dropped} dropped (canvas is full)`);
        setStatus(parts.join(' - '));
    } else if (dropped > 0) {
        setStatus(`Canvas is full - ${dropped} pattern${dropped === 1 ? '' : 's'} could not be added`);
    }
}
drainControlQueue();
subscribeControlQueue(() => { drainControlQueue(); });
