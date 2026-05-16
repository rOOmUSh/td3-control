// Bootstrap for the progression page - wires all modules together.

import * as state from './progression-state.js';
import * as sequencer from './progression-sequencer.js';
import * as selectors from '../selectors.js';
import * as midiStatus from '../midi-status.js';
import { loadScales, getScale, populateScaleSelect, getAllScales, getTagGroups } from '../scales.js';
import { buildPitchClassHistogram } from '../key-detection.js';
import { rankScales, applyRankedOrder } from '../scale-ranking.js';
import {
    generateProgression, deriveSiblings,
    createRng, resolveProfile, chooseProgressionDegrees,
} from './progression-generator.js';
import { toast } from '../bank/bank-toast.js';
import { consumeSendToProgressionHandoff } from './send-to-progression-handoff.js';
import { buildHarmonicMap } from '../harmony-map.js';
import { generateAllBasslines } from './bassline/generator-v2.js';
import * as basslinePreview from './bassline-preview.js';
import * as packageDb from './progression-package-db.js';
import * as packageExport from './progression-package-export.js';
import * as timeline from './progression-timeline.js';
import * as transport from './progression-transport.js';
import { startPlayback, stopPlayback } from './progression-playback.js';
import * as history from '../history.js';
import * as deviceBackup from '../device-backup.js';
import * as clipboard from './progression-clipboard.js';
import { api } from '../api.js';
import { loadAppConfig, applyUiDefaults } from '../app-config.js';
import { renderAllPatternRows } from './progression-row.js';
import { openPushModal } from './progression-push.js';
import {
    buildSinglePatternSlots,
    buildProgressionSlots,
    formatSnapshotName,
} from './progression-bank-snapshot.js';
import { bankApi } from '../bank/bank-api.js';
import { wireMagicToggle, isMagicEnabled } from '../magic-randomizer/magic-state.js';
import {
    runMagicProgression,
    BUDGET_BULK,
} from '../magic-randomizer/magic-randomizer.js';

// Render the four pattern rows into their container before any other module
// reaches for row-pN / grid-pN / label-pN elements.
renderAllPatternRows(document.getElementById('prog-rows'));

// Wire the MAGIC toggle in the shared sidebar partial. Idempotent.
wireMagicToggle();

const statusLog = document.getElementById('status-log');
const midiPreviewVol = document.getElementById('midi-preview-vol');
const midiPreviewVolVal = document.getElementById('midi-preview-vol-val');
const btnRandomize = document.getElementById('btn-randomize');
const btnLive = document.getElementById('btn-live');
const bpmDisplay = document.getElementById('bpm-display');
const bpmKnob = document.getElementById('bpm-knob');
const knobIndicator = document.getElementById('knob-indicator');
const bpmFineToggle = document.getElementById('bpm-fine-toggle');

let bpmFineMode = false;
const btnPlay = document.getElementById('btn-play');
const bankCount = document.getElementById('bank-count');
const scaleSelect = document.getElementById('scale-select');
const btnTimeline = document.getElementById('btn-timeline');
const timelineModal = document.getElementById('timeline-modal');
const btnTimelineClose = document.getElementById('btn-timeline-close');
const timelineBackdrop = document.getElementById('timeline-backdrop');

const sliderNote = document.getElementById('slider-note');
const sliderSlide = document.getElementById('slider-slide');
const sliderAcc = document.getElementById('slider-acc');
const sliderUd = document.getElementById('slider-ud');
const sliderNoteVal = document.getElementById('slider-note-val');
const sliderSlideVal = document.getElementById('slider-slide-val');
const sliderAccVal = document.getElementById('slider-acc-val');
const sliderUdVal = document.getElementById('slider-ud-val');

// --- Status ---

export function setStatus(msg) {
    statusLog.textContent = msg;
    console.log('[TD3-PROG]', msg);
}

// --- Scale ranking ---

// Re-rank the scale-select using P1's pitch-class histogram at the given
// root. Mirrors the main-page behavior (scale-ranking.js) so scrolling the
// scale dropdown on the progression page surfaces the same alternatives
// the user saw on Control - and so RANDOMIZE 2-4 iterations land on scales
// that actually fit the locked P1 bassline.
function rankScaleSelectFromP1(p1, root) {
    if (!p1 || !Number.isInteger(root)) return;
    const hist = buildPitchClassHistogram(p1);
    if (hist.every(v => v === 0)) return;
    const ranked = rankScales({ scales: getAllScales(), hist, root });
    applyRankedOrder(scaleSelect, {
        ranked, topN: 5,
        tagGroups: getTagGroups(), allScales: getAllScales(),
    });
}

// --- Supporting basslines (generated alongside acid progression) ---

// Legacy name retained for restore-from-DB + status text only. The 5×4
// archetype set now lives in state.getBasslines() / state.getActiveBassline().
let basslineMeta = null;
let lastProgressionSeed = null;  // reserved for future replay; not persisted in V1
let lastPackageId = null;        // IndexedDB packageId of the last saved package
let activeMidiPreviewIdx = -1;       // WebAudio bassline preview, or -1
let activeBassPreviewIdx = -1;       // TD-3 bassline preview, or -1
let activePatternPreviewIdx = -1;    // TD-3 raw pattern preview (▶ Pn), or -1
let openBassMenuIdx = -1;            // which pattern row has its BASS dropdown open, or -1

function setBassMenuOpen(idx, open) {
    const menu = document.querySelector(`[data-bass-menu="${idx}"]`);
    if (!menu) return;
    menu.classList.toggle('hidden', !open);
}

function closeAllBassMenus() {
    if (openBassMenuIdx >= 0) {
        setBassMenuOpen(openBassMenuIdx, false);
        openBassMenuIdx = -1;
    }
}

function setPreviewButtonActive(action, idx, active) {
    const btn = document.querySelector(`[data-action="${action}"][data-pattern-idx="${idx}"]`);
    if (!btn) return;
    btn.classList.toggle('is-active', active);
}

// All three previews - MIDI (WebAudio), PREVIEW BASS (TD-3 bassline),
// and ▶ Pn (TD-3 raw pattern) - are mutually exclusive. Every handler
// calls stopAllPreviews before starting a new one so a click on any
// preview button silences whatever else was playing.
// Pass { keepMenu: true } when the archetype dropdown should stay open
// (e.g. the user is switching between archetypes inside the menu).
async function stopAllPreviews({ keepMenu = false } = {}) {
    if (basslinePreview.isPreviewing()) basslinePreview.stopPreview();
    if (activeMidiPreviewIdx >= 0) {
        setPreviewButtonActive('midi-preview', activeMidiPreviewIdx, false);
        activeMidiPreviewIdx = -1;
    }
    if (activeBassPreviewIdx >= 0) {
        try { await api.transportStop(); } catch { /* ignore */ }
        setPreviewButtonActive('bass-preview', activeBassPreviewIdx, false);
        activeBassPreviewIdx = -1;
    }
    if (activePatternPreviewIdx >= 0) {
        try { await api.transportStop(); } catch { /* ignore */ }
        setPreviewButtonActive('pattern-preview', activePatternPreviewIdx, false);
        activePatternPreviewIdx = -1;
    }
    if (!keepMenu) closeAllBassMenus();
}

async function handleMidiPreview(clickIdx) {
    const bl = state.getActiveBassline(clickIdx);
    if (!bl) {
        setStatus('Generate a progression first to preview the bassline');
        return;
    }
    const wasPlayingThis = activeMidiPreviewIdx === clickIdx && basslinePreview.isPreviewing();
    await stopAllPreviews();
    if (wasPlayingThis) {
        setStatus('MIDI preview stopped');
        return;
    }
    try {
        basslinePreview.previewBassline(bl, {
            bpm: state.getBpm(),
            onEnd: () => {
                if (activeMidiPreviewIdx === clickIdx) {
                    setPreviewButtonActive('midi-preview', clickIdx, false);
                    activeMidiPreviewIdx = -1;
                }
            },
        });
        activeMidiPreviewIdx = clickIdx;
        setPreviewButtonActive('midi-preview', clickIdx, true);
        setStatus(`MIDI preview: bassline P${clickIdx + 1}`);
    } catch (err) {
        setStatus('Preview error: ' + err.message);
    }
}

// BASSLINE button toggles the per-row archetype dropdown. Opening it does
// NOT start playback; it only reveals the 5 archetype chips. Clicking the
// button again (or clicking another preview) stops any active bassline
// playback and closes the menu. Actual playback is started by
// handleArchetypePlay() when the user picks a chip from the open dropdown.
async function handleBassPreview(clickIdx) {
    if (openBassMenuIdx === clickIdx) {
        await stopAllPreviews();
        setStatus('Bassline preview stopped');
        return;
    }
    if (!state.getBasslinesFor(clickIdx)) {
        setStatus('Generate a progression first to audition basslines');
        return;
    }
    await stopAllPreviews();
    setBassMenuOpen(clickIdx, true);
    openBassMenuIdx = clickIdx;
    setStatus(`P${clickIdx + 1}: pick a bassline archetype`);
}

async function handleArchetypePlay(clickIdx, key) {
    state.setActiveArchetype(clickIdx, key);
    const bl = state.getActiveBassline(clickIdx);
    if (!bl) {
        setStatus(`P${clickIdx + 1} bass → ${key}`);
        return;
    }
    if (state.isPlaying()) {
        setStatus('Stop acid transport before auditioning bassline');
        return;
    }
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }
    // Swap the playing bassline without closing the dropdown so the user
    // can A/B archetypes from the same menu.
    await stopAllPreviews({ keepMenu: true });
    try {
        await api.savePattern(
            scratch.group, scratch.pattern, scratch.side,
            bl
        );
        await api.transportStart(state.getBpm());
        activeBassPreviewIdx = clickIdx;
        setPreviewButtonActive('bass-preview', clickIdx, true);
        setStatus(`TD-3 preview: P${clickIdx + 1} ${key} → ${scratch.label}`);
    } catch (err) {
        setStatus('Preview error: ' + err.message);
    }
}

async function handlePatternPreview(clickIdx) {
    if (state.isPlaying()) {
        setStatus('Stop acid transport before auditioning patterns');
        return;
    }
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }
    const wasPlayingThis = activePatternPreviewIdx === clickIdx;
    await stopAllPreviews();
    if (wasPlayingThis) {
        setStatus(`TD-3 pattern preview stopped`);
        return;
    }
    try {
        await api.savePattern(
            scratch.group, scratch.pattern, scratch.side,
            state.getPattern(clickIdx)
        );
        await api.transportStart(state.getBpm());
        activePatternPreviewIdx = clickIdx;
        setPreviewButtonActive('pattern-preview', clickIdx, true);
        setStatus(`TD-3 preview: P${clickIdx + 1} → ${scratch.label}`);
    } catch (err) {
        setStatus('Preview error: ' + err.message);
    }
}

// --- Live update button ---

function updateLiveBtn() {
    btnLive.classList.toggle('is-active', state.isLiveUpdate());
}

btnLive.addEventListener('click', () => {
    state.setLiveUpdate(!state.isLiveUpdate());
    setStatus(state.isLiveUpdate() ? 'Live update ON' : 'Live update OFF');
});

// --- BPM knob ---

function updateBpmDisplay() {
    const bpm = state.getBpm();
    bpmDisplay.textContent = bpm.toFixed(bpmFineMode ? 2 : 0);
    const angle = ((bpm - 20) / 280) * 300 - 150;
    knobIndicator.style.transform = `rotate(${angle}deg)`;
}

bpmKnob.addEventListener('wheel', (e) => {
    e.preventDefault();
    const step = bpmFineMode ? 0.01 : 1;
    state.setBpm(state.getBpm() + (e.deltaY < 0 ? step : -step));
    updateBpmDisplay();
    transport.restartTimer();
});

if (bpmFineToggle) {
    bpmFineToggle.addEventListener('click', () => {
        bpmFineMode = !bpmFineMode;
        bpmFineToggle.classList.toggle('sync-pill--active', bpmFineMode);
        bpmFineToggle.setAttribute('aria-pressed', bpmFineMode ? 'true' : 'false');
        if (!bpmFineMode) {
            state.setBpm(Math.trunc(state.getBpm()));
        }
        updateBpmDisplay();
        transport.restartTimer();
    });
}

let dragging = false, dragStartY = 0, dragStartBpm = 0;
bpmKnob.addEventListener('mousedown', (e) => {
    dragging = true;
    dragStartY = e.clientY;
    dragStartBpm = state.getBpm();
    e.preventDefault();
});
document.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    state.setBpm(dragStartBpm + Math.round((dragStartY - e.clientY) / 3));
    updateBpmDisplay();
    transport.restartTimer();
});
document.addEventListener('mouseup', () => { dragging = false; });

// --- Play button - wired to progression transport ---

function updatePlayButton() {
    const icon = btnPlay.querySelector('.material-symbols-outlined');
    if (state.isPlaying()) {
        icon.textContent = 'stop';
        btnPlay.classList.add('led-glow-green');
    } else {
        icon.textContent = 'play_arrow';
        btnPlay.classList.remove('led-glow-green');
    }
}

btnPlay.addEventListener('click', async () => {
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }
    try {
        if (state.isPlaying()) {
            await stopPlayback({
                api,
                transport,
                resetTimeline: () => state.resetTimeline(),
                setPlaying: (v) => state.setPlaying(v),
                setStatus,
            });
        } else {
            state.resetTimeline();
            await startPlayback({
                api,
                timeline: state.getTimeline(),
                getPattern: (idx) => state.getPattern(idx),
                scratch,
                bpm: state.getBpm(),
                transport,
                stopAllPreviews,
                setPlaying: (v) => state.setPlaying(v),
                setStatus,
            });
        }
        updatePlayButton();
    } catch (err) {
        setStatus('Transport error: ' + err.message);
    }
});

// --- Push to TD-3 ---

const btnPushTd3 = document.getElementById('btn-push-td3');
if (btnPushTd3) {
    btnPushTd3.addEventListener('click', () => {
        openPushModal({ state, scratch, api, setStatus });
    });
}

// --- SAVE PACKAGE → BANK (push full progression as snapshot) ---
//
// Lives in the SAVE PACKAGE dropdown next to the format checkboxes. Enable
// gating mirrors the SAVE PACKAGE button: requires a generated package
// (4 patterns + 5×4 archetype basslines). The button refreshes its disabled
// state from the same state listener that updates the rest of the page.

const btnPkgBank = document.getElementById('btn-pkg-bank');
let bankPushInFlight = false;

function refreshBankPushButton() {
    if (!btnPkgBank) return;
    const ready = state.getBasslines().every(Boolean) && !bankPushInFlight;
    btnPkgBank.disabled = !ready;
    if (bankPushInFlight) btnPkgBank.title = 'Pushing…';
    else if (ready) btnPkgBank.title = 'Push 4 acid + 20 archetype basslines as a bank snapshot';
    else btnPkgBank.title = 'Generate a progression first';
}

if (btnPkgBank) {
    btnPkgBank.addEventListener('click', async () => {
        if (bankPushInFlight) return;
        bankPushInFlight = true;
        refreshBankPushButton();
        try {
            await pushFullProgressionToBankSnapshot();
        } finally {
            bankPushInFlight = false;
            refreshBankPushButton();
        }
    });
}

// --- Bank display ---

// function updateBankDisplay() {
//     bankCount.textContent = state.getBankCount();
// }

// --- Per-pattern action buttons (delegated) ---

function getSlidePercent() { return parseInt(sliderSlide.value) / 100; }
function getAccPercent() { return parseInt(sliderAcc.value) / 100; }
function getNotePercent() { return parseInt(sliderNote.value) / 100; }
function getUdPercent() { return sliderUd ? parseInt(sliderUd.value) / 100 : 0; }

// --- BANK snapshot push ---
//
// The per-row BANK button creates a fresh SQLite bank snapshot named with a
// `SN_YYYY-MM-DD_HH-MM-SS` local timestamp containing the acid pattern in G1-P1A and its
// 5 archetype basslines in G1-P1B..G1-P5B. SAVE PACKAGE → BANK reuses the
// same flow but with the full progression payload (4 acid + 20 basslines).
//
// Failure handling: shape errors short-circuit before any HTTP call;
// network / backend errors reach the catch and surface via setStatus +
// toast. We never partially-create a snapshot - atomic creation is the
// backend's contract.

async function pushSinglePatternToBankSnapshot(idx) {
    const pattern = state.getPattern(idx);
    const basslineSet = state.getBasslinesFor(idx);
    if (!basslineSet) {
        const msg = `P${idx + 1}: generate a progression first to populate basslines`;
        setStatus(msg);
        toast(msg, 'error');
        return;
    }
    const { slots, error } = buildSinglePatternSlots(idx, pattern, basslineSet);
    if (error || !slots) {
        const msg = `Bank push aborted: ${error || 'unknown shape error'}`;
        setStatus(msg);
        toast(msg, 'error');
        return;
    }
    const name = formatSnapshotName();
    setStatus(`Pushing P${idx + 1} → bank snapshot '${name}'…`);
    try {
        const created = await bankApi.createSnapshotFromPatterns({
            name,
            description: `Progression P${idx + 1} + 5 archetype basslines`,
            slots,
        });
        const finalName = created && created.snapshot ? created.snapshot.name : name;
        const msg = `P${idx + 1} pushed → bank snapshot '${finalName}'`;
        setStatus(msg);
        toast(msg, 'success');
    } catch (err) {
        const msg = `Bank push failed: ${err.message || err}`;
        setStatus(msg);
        toast(msg, 'error');
    }
}

async function pushFullProgressionToBankSnapshot() {
    const patterns = state.getPatterns();
    const basslines = state.getBasslines();
    const { slots, error } = buildProgressionSlots(patterns, basslines);
    if (error || !slots) {
        const msg = `Bank push aborted: ${error || 'unknown shape error'}`;
        setStatus(msg);
        toast(msg, 'error');
        return false;
    }
    const name = formatSnapshotName();
    const label = state.getProgressionLabel() || '';
    setStatus(`Pushing progression → bank snapshot '${name}'…`);
    try {
        const created = await bankApi.createSnapshotFromPatterns({
            name,
            description: label
                ? `Progression "${label}" - 4 acid + 20 archetype basslines`
                : `Progression - 4 acid + 20 archetype basslines`,
            slots,
        });
        const finalName = created && created.snapshot ? created.snapshot.name : name;
        const msg = `Progression pushed → bank snapshot '${finalName}'`;
        setStatus(msg);
        toast(msg, 'success');
        return true;
    } catch (err) {
        const msg = `Bank push failed: ${err.message || err}`;
        setStatus(msg);
        toast(msg, 'error');
        return false;
    }
}


document.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-action]');
    if (!btn) return;
    const idx = parseInt(btn.dataset.patternIdx);
    const action = btn.dataset.action;

    if (action === 'copy') {
        const kind = btn.dataset.kind;
        if (!kind) return;
        if (kind === 'full') {
            // FULL copy bridges to main-page state via sessionStorage so the
            // main Control page picks it up on its next load. Identical
            // channel as the legacy per-row COPY button.
            clipboard.copy('full', state.getPattern(idx));
            setStatus(`P${idx + 1} full → clipboard (main page)`);
        } else if (kind === 'rest' || kind === 'slide' || kind === 'accent') {
            clipboard.copy(kind, state.getPattern(idx));
            setStatus(`P${idx + 1} ${kind} → clipboard`);
        }
    } else if (action === 'paste') {
        const kind = btn.dataset.kind;
        if (!kind) return;
        const pat = state.getPattern(idx);
        const applied = clipboard.paste(kind, pat);
        if (applied) {
            // setPattern re-notifies so the sequencer re-renders, live-send
            // fires, and undo history captures the paste.
            state.setPattern(idx, pat);
            setStatus(`P${idx + 1} ${kind} ← clipboard`);
        } else {
            setStatus(`Clipboard has no ${kind} payload`);
        }
    } else if (action === 'bank') {
        void pushSinglePatternToBankSnapshot(idx);
    } else if (action === 'rand-sl') {
        state.randomizeSlides(idx, getSlidePercent());
        setStatus(`P${idx + 1} slides randomized`);
    } else if (action === 'rand-acc') {
        state.randomizeAccents(idx, getAccPercent());
        setStatus(`P${idx + 1} accents randomized`);
    } else if (action === 'rand-rst') {
        state.randomizeRests(idx, getNotePercent());
        setStatus(`P${idx + 1} rests randomized`);
    } else if (action === 'rand-ud') {
        state.randomizeUd(idx, getUdPercent());
        setStatus(`P${idx + 1} UP/DOWN randomized`);
    } else if (action === 'shift') {
        const n = parseInt(btn.dataset.shift);
        state.shiftPatternSteps(idx, n);
        setStatus(`P${idx + 1} shifted ${n > 0 ? '+' : ''}${n}`);
    } else if (action === 'transpose') {
        const delta = parseInt(btn.dataset.delta);
        state.transposePatternAt(idx, delta);
        setStatus(`P${idx + 1} transposed ${delta > 0 ? '+' : ''}${delta}`);
    } else if (action === 'midi-preview') {
        handleMidiPreview(idx);
    } else if (action === 'bass-preview') {
        handleBassPreview(idx);
    } else if (action === 'pattern-preview') {
        handlePatternPreview(idx);
    } else if (action === 'archetype') {
        const key = btn.dataset.archetype;
        if (key) handleArchetypePlay(idx, key);
    }
});

// Flatten a 4-length array of { pedal, rootPulse, offbeat, shadow, arpeggio }
// archetype maps into the 20-entry position-major × archetype-minor array
// the backend expects for combined SQS/RBS exports. Returns `undefined` if
// any archetype is missing - safer to fall back to the 4-bassline layout
// than ship a malformed 20-slot matrix.
function flattenBasslineMatrix(byPattern) {
    if (!Array.isArray(byPattern) || byPattern.length !== 4) return undefined;
    const flat = [];
    for (let i = 0; i < 4; i++) {
        const set = byPattern[i];
        if (!set) return undefined;
        for (const key of packageDb.ARCHETYPE_KEYS) {
            const pat = set[key];
            if (!pat) return undefined;
            flat.push(pat);
        }
    }
    return flat;
}

// Shared bassline generation + package persistence. Called from both
// RANDOMIZE (full generate flow) and the SEND TO PROGRESSION handoff so both
// entry points land in the same post-state: 4 patterns with 5 archetype
// basslines each, a persisted IndexedDB package, and a primed export module
// so SAVE PACKAGE is enabled. Returns true when the package persisted.
async function generateBasslinesAndPersistPackage({
    patterns, root, scale, profile, degrees, label,
}) {
    state.clearBasslines();
    basslineMeta = null;
    try {
        const harmonicMap = buildHarmonicMap({
            packageId: null,
            seed: lastProgressionSeed,
            root,
            scale,
            profile,
            degrees,
            timeline: state.getTimeline(),
        });
        const bassResult = generateAllBasslines({
            acidPatterns: patterns,
            harmonicMap,
            rng: { next: () => Math.random() },
        });
        state.setBasslines(bassResult.basslinesByPattern, bassResult.defaultArchetypeByPattern);
        basslineMeta = { archetypes: bassResult.defaultArchetypeByPattern, features: bassResult.features };

        try {
            const activeBasslines = bassResult.basslinesByPattern.map((set, i) =>
                set[bassResult.defaultArchetypeByPattern[i]]
            );
            const basslinesFull = flattenBasslineMatrix(bassResult.basslinesByPattern);
            const rows = packageDb.buildRows({
                seed: lastProgressionSeed,
                root,
                scaleId: scale.id ?? '',
                scaleName: scale.name ?? '',
                profile,
                degrees,
                label,
                timeline: state.getTimeline(),
                rhythmMode: bassResult.defaultArchetypeByPattern.join(','),
                acidPatterns: patterns,
                basslinesByPattern: bassResult.basslinesByPattern,
                defaultArchetypeByPattern: bassResult.defaultArchetypeByPattern,
                harmonicMap,
            });
            lastPackageId = await packageDb.savePackage(rows.pkg, rows.patternRows, rows.basslineRows);
            packageExport.setPackage({
                packageId: lastPackageId,
                label,
                scaleName: scale.name ?? '',
                acidPatterns: patterns,
                basslines: activeBasslines,
                basslinesFull,
            });
            return true;
        } catch (err) {
            // Persistence failure must not block a successful in-memory
            // generation - the user can still preview this session.
            console.warn('[TD3-PROG] package persist failed:', err);
            packageExport.clearPackage();
            return false;
        }
    } catch (err) {
        setStatus(`Bassline generation failed: ${err.message}`);
        packageExport.clearPackage();
        return false;
    }
}

// Paint the per-row BASS chip row: highlight the active archetype, dim the
// rest. Called from the state.onChange listener so switching or regeneration
// keeps the UI in sync with state.getActiveArchetype(idx).
function refreshArchetypeChips() {
    const active = state.getActiveArchetypes();
    const hasBasslines = state.getBasslines().some(Boolean);
    for (let i = 0; i < 4; i++) {
        const buttons = document.querySelectorAll(
            `[data-action="archetype"][data-pattern-idx="${i}"]`
        );
        for (const btn of buttons) {
            const isActive = hasBasslines && btn.dataset.archetype === active[i];
            btn.classList.toggle('is-active', isActive);
            btn.classList.toggle('opacity-50', !hasBasslines);
        }
    }
}

// Scratch pattern - the device slot used for play/live-send.
let scratch = { group: 1, pattern: 1, side: 'A', label: 'G1-P1A' };

// --- Global SL/ACC and SHIFT ALL ---

function sendActivePattern() {
    if (!state.isPlaying() || !state.isLiveUpdate() || !state.isConnected()) return;
    const idx = state.getActivePatternIndex();
    api.savePattern(
        scratch.group, scratch.pattern, scratch.side,
        state.getPattern(idx)
    ).then(() => {
        setStatus(`P${idx + 1} sent → ${scratch.label}`);
    }).catch(err => {
        setStatus(`Live send error: ${err.message}`);
    });
}

document.getElementById('btn-rand-sl').addEventListener('click', () => {
    state.randomizeSlidesAll(getSlidePercent());
    setStatus('All slides randomized');
});
document.getElementById('btn-rand-acc').addEventListener('click', () => {
    state.randomizeAccentsAll(getAccPercent());
    setStatus('All accents randomized');
});

document.getElementById('btn-rand-rst').addEventListener('click', () => {
    state.randomizeRestsAll(getNotePercent());
    setStatus('All accents randomized');
});

{
    const btn = document.getElementById('btn-rand-ud');
    if (btn) {
        btn.addEventListener('click', () => {
            state.randomizeUdAll(getUdPercent());
            setStatus('All UP/DOWN randomized');
        });
    }
}


document.getElementById('btn-gshift-b4').addEventListener('click', () => { state.shiftAllSteps(-4); setStatus('All shifted -4'); });
document.getElementById('btn-gshift-b2').addEventListener('click', () => { state.shiftAllSteps(-2); setStatus('All shifted -2'); });
document.getElementById('btn-gshift-b1').addEventListener('click', () => { state.shiftAllSteps(-1); setStatus('All shifted -1'); });
document.getElementById('btn-gshift-f1').addEventListener('click', () => { state.shiftAllSteps(1);  setStatus('All shifted +1'); });
document.getElementById('btn-gshift-f2').addEventListener('click', () => { state.shiftAllSteps(2);  setStatus('All shifted +2'); });
document.getElementById('btn-gshift-f4').addEventListener('click', () => { state.shiftAllSteps(4);  setStatus('All shifted +4'); });

document.getElementById('btn-gtrnsps-up').addEventListener('click',   () => { state.transposeAllPatterns(+1);  setStatus('All transposed +1'); });
document.getElementById('btn-gtrnsps-dn').addEventListener('click',   () => { state.transposeAllPatterns(-1);  setStatus('All transposed −1'); });
document.getElementById('btn-gtrnsps-up12').addEventListener('click', () => { state.transposeAllPatterns(+12); setStatus('All transposed +12'); });
document.getElementById('btn-gtrnsps-dn12').addEventListener('click', () => { state.transposeAllPatterns(-12); setStatus('All transposed −12'); });

// --- Slider labels ---

sliderNote.addEventListener('input', () => { sliderNoteVal.textContent = sliderNote.value + '%'; });
sliderSlide.addEventListener('input', () => { sliderSlideVal.textContent = sliderSlide.value + '%'; });
sliderAcc.addEventListener('input', () => { sliderAccVal.textContent = sliderAcc.value + '%'; });
if (sliderUd && sliderUdVal) {
    sliderUdVal.textContent = sliderUd.value + '%';
    sliderUd.addEventListener('input', () => { sliderUdVal.textContent = sliderUd.value + '%'; });
}

// --- MIDI preview volume (shared across all WebAudio bassline previews) ---

const MIDI_VOL_STORAGE_KEY = 'td3_midi_preview_vol';

function initMidiPreviewVol() {
    if (!midiPreviewVol || !midiPreviewVolVal) return;
    const stored = parseFloat(sessionStorage.getItem(MIDI_VOL_STORAGE_KEY));
    const pct = (isFinite(stored) && stored >= 0 && stored <= 100) ? stored : 30;
    midiPreviewVol.value = String(pct);
    midiPreviewVolVal.textContent = `${Math.round(pct)}%`;
    basslinePreview.setPreviewGain(pct / 100);
}

if (midiPreviewVol && midiPreviewVolVal) {
    midiPreviewVol.addEventListener('input', () => {
        const pct = parseFloat(midiPreviewVol.value);
        midiPreviewVolVal.textContent = `${Math.round(pct)}%`;
        basslinePreview.setPreviewGain(pct / 100);
        sessionStorage.setItem(MIDI_VOL_STORAGE_KEY, String(pct));
    });
}

// --- Randomize progression ---

const rootSelect = document.getElementById('root-select');
const btnRandomize24 = document.getElementById('btn-randomize-2-4');

const NOTE_NAMES_ROOT = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];

// Shared progression-apply flow. Takes a generator function that produces
// `{ patterns, degrees, profile, label }` - same shape generateProgression
// emits - and handles the surrounding work: preview teardown, bassline +
// package persistence, timeline reset, device send. Both RANDOMIZE 1-4
// (full regen) and RANDOMIZE 2-4 (keep P1, re-derive siblings) funnel
// through here so the post-generation behavior stays identical.
//
// `skipDeviceSendP1` suppresses the final savePattern call when the caller
// knows P1 didn't change - RANDOMIZE 2-4 locks P1 verbatim, so pushing it
// again is wasteful and would pointlessly tap the device.
async function runProgressionFlow(generateFn, { skipDeviceSendP1 = false } = {}) {
    const root = parseInt(rootSelect.value);
    const scale = getScale(scaleSelect.value);

    try {
        // Regeneration invalidates any active preview.
        await stopAllPreviews();
        // Stale package must not survive across a re-randomize - re-enabled
        // only after the new bassline set persists successfully.
        packageExport.clearPackage();

        const config = await api.getProgressionConfig();
        const result = generateFn(config, { root, scale });

        if (!result) {
            setStatus('Generation failed - check scale selection');
            return;
        }

        state.setPatterns(result.patterns);
        state.setProgressionLabel(result.label);
        state.setProgressionDegrees(result.degrees);
        state.setProgressionRoot(root);
        state.setProgressionScaleId(scale?.id || scaleSelect.value || null);

        // Re-rank the scale dropdown against the freshly-installed P1 so
        // the user's next iteration (or RANDOMIZE 2-4) sees scales ordered
        // by fit. Preserves the current selection if the chosen scale still
        // exists - which it does, since ranking doesn't filter, only sorts.
        rankScaleSelectFromP1(result.patterns[0], root);

        // ---- Supporting basslines (v2: 5 archetypes per pattern) ----
        await generateBasslinesAndPersistPackage({
            patterns: result.patterns,
            root,
            scale,
            profile: result.profile,
            degrees: result.degrees,
            label: result.label,
        });

        // Set default timeline from config
        if (config.default_timeline) {
            state.setTimeline([...config.default_timeline]);
        }

        // Push all 4 to bank
        for (const p of result.patterns) {
            state.pushToBank(p);
        }

        // Update role labels
        const roleLabels = ['home', 'move away', 'tension', 'resolve'];
        for (let i = 0; i < 4; i++) {
            const el = document.getElementById(`label-p${i + 1}`);
            if (el) el.textContent = roleLabels[i];
        }

        // Reset timeline to beginning and send P1 to device.
        // When playback is live, defer the UI timeline jump until the next
        // pattern wrap - the device keeps looping its current pattern until
        // its own internal wrap, so an immediate UI reset would desync the
        // step highlight from what the device is actually playing.
        if (state.isPlaying()) {
            transport.queueRandomizeReset();
        } else {
            state.resetTimeline();
        }
        if (!skipDeviceSendP1 && state.isLiveUpdate() && state.isConnected()) {
            try {
                await api.savePattern(
                    scratch.group, scratch.pattern, scratch.side,
                    result.patterns[0]
                );
                setStatus(`Generated: ${result.label} - P1 sent → ${scratch.label}`);
            } catch (err) {
                setStatus(`Generated: ${result.label} - send error: ${err.message}`);
            }
        } else {
            setStatus(`Generated: ${result.label}`);
        }
    } catch (err) {
        setStatus('Generation error: ' + err.message);
    }
}

// RANDOMIZE 1-4 - full fresh generation (P1..P4 all new).
//
// Legacy path (MAGIC unchecked) calls generateProgression() exactly as
// before - byte-for-byte unchanged. Magic path calls
// runMagicProgression() per pattern with each pattern's centerPc derived
// from the chosen degrees.
btnRandomize.addEventListener('click', () => {
    const notePercent = parseInt(sliderNote.value) / 100;
    const slidePercent = parseInt(sliderSlide.value) / 100;
    const accPercent = parseInt(sliderAcc.value) / 100;
    if (isMagicEnabled()) {
        return runProgressionFlow((config, { root, scale }) =>
            magicGenerateProgression({ config, root, scale, notePercent, slidePercent, accPercent }));
    }
    return runProgressionFlow((config, { root, scale }) => generateProgression({
        root, scale, notePercent, slidePercent, accPercent,
        progressionConfig: config,
    }));
});

// RANDOMIZE 2-4 - keep the current P1, re-derive P2..P4 via the same
// sibling chain SEND TO PROGRESSION uses. Lets the user hold a locked P1
// and iterate on degree path / scale choice to find a progression that
// frames their bassline. Fresh rng + fresh chooseProgressionDegrees on
// each click, so every press reshuffles P2..P4 even with identical
// root/scale selections.
if (btnRandomize24) {
    btnRandomize24.addEventListener('click', () => {
        const notePercent = parseInt(sliderNote.value) / 100;
        const slidePercent = parseInt(sliderSlide.value) / 100;
        const accPercent = parseInt(sliderAcc.value) / 100;
        if (isMagicEnabled()) {
            return runProgressionFlow((config, { root, scale }) => {
                const patterns = state.getPatterns();
                const p1 = patterns && patterns[0];
                if (!p1) { setStatus('No P1 to derive from - run RANDOMIZE 1-4 or SEND TO PROGRESSION first'); return null; }
                const rng = createRng(null);
                const profile = resolveProfile(scale, config);
                const degrees = chooseProgressionDegrees(profile, config, rng);
                const result = magicGenerateSiblings({
                    p1, root, scale, degrees,
                    notePercent, slidePercent, accPercent,
                });
                const rootName = NOTE_NAMES_ROOT[root] || '?';
                result.label = `${rootName} ${scale.name} - P1 locked, P2..P4 magic-derived`;
                result.profile = profile;
                return result;
            }, { skipDeviceSendP1: true });
        }
        return runProgressionFlow((config, { root, scale }) => {
            const patterns = state.getPatterns();
            const p1 = patterns && patterns[0];
            if (!p1) { setStatus('No P1 to derive from - run RANDOMIZE 1-4 or SEND TO PROGRESSION first'); return null; }
            const rng = createRng(null);
            const profile = resolveProfile(scale, config);
            const degrees = chooseProgressionDegrees(profile, config, rng);
            const anchorSteps = config.anchor_steps || [0, 4, 8, 12];
            const derived = deriveSiblings(p1, {
                root, scale, degrees, anchorSteps, config, rng, profile,
            });
            const rootName = NOTE_NAMES_ROOT[root] || '?';
            const label = `${rootName} ${scale.name} - P1 locked, P2..P4 derived`;
            return { patterns: derived, degrees, profile, label };
        }, { skipDeviceSendP1: true });
    });
}

// ---------------------------------------------------------------------------
// MAGIC progression helpers
//
// Both helpers return the same shape runProgressionFlow expects:
//   { patterns, degrees, profile, label }
// ---------------------------------------------------------------------------

function magicGenerateProgression({ config, root, scale, notePercent, slidePercent, accPercent }) {
    if (!scale || !Array.isArray(scale.intervals) || scale.intervals.length === 0) return null;
    const rng = createRng(null);
    const profile = resolveProfile(scale, config);
    const degrees = chooseProgressionDegrees(profile, config, rng);

    const patterns = degrees.map((deg) => {
        const centerPc = degreeToPc(root, scale, deg);
        const result = runMagicProgression({
            root, scale, centerPc,
            registerCenter: 6,
            notePercent, slidePercent, accPercent,
            attempts: BUDGET_BULK,
        });
        return { active_steps: result.active_steps, triplet: result.triplet, steps: result.steps };
    });

    const rootName = NOTE_NAMES_ROOT[root] || '?';
    const degreeNames = degrees.map(d => NOTE_NAMES_ROOT[degreeToPc(root, scale, d)]);
    const label = `${rootName} ${scale.name} (magic) - ${degreeNames.join(' → ')}`;
    return { patterns, degrees, profile, label };
}

function magicGenerateSiblings({ p1, root, scale, degrees, notePercent, slidePercent, accPercent }) {
    if (!scale || !Array.isArray(scale.intervals) || scale.intervals.length === 0) return null;
    const patterns = [JSON.parse(JSON.stringify(p1))];
    for (let i = 1; i < degrees.length; i++) {
        const centerPc = degreeToPc(root, scale, degrees[i]);
        const result = runMagicProgression({
            root, scale, centerPc,
            registerCenter: 6,
            notePercent, slidePercent, accPercent,
            attempts: BUDGET_BULK,
        });
        patterns.push({ active_steps: result.active_steps, triplet: result.triplet, steps: result.steps });
    }
    return { patterns, degrees };
}

function degreeToPc(root, scale, degree) {
    const idx = (degree - 1) % scale.intervals.length;
    return ((root + scale.intervals[idx]) % 12 + 12) % 12;
}

// --- Timeline modal ---

btnTimeline.addEventListener('click', () => {
    timelineModal.classList.remove('hidden');
    timeline.render();
});
btnTimelineClose.addEventListener('click', () => {
    timelineModal.classList.add('hidden');
});
timelineBackdrop.addEventListener('click', () => {
    timelineModal.classList.add('hidden');
});

// --- Undo/Redo with debounce ---

let historyDebounce = null;
let isRestoring = false; // flag to skip recording during undo/redo restore

function recordHistory() {
    if (isRestoring) return;
    clearTimeout(historyDebounce);
    historyDebounce = setTimeout(() => {
        history.push('progression', state.getSnapshot());
    }, 300);
}

// --- Paste-button enable/disable driven by clipboard presence ---

function refreshPasteButtons() {
    const buttons = document.querySelectorAll('[data-action="paste"]');
    for (const btn of buttons) {
        const kind = btn.dataset.kind;
        const on = clipboard.has(kind);
        btn.disabled = !on;
        btn.classList.toggle('opacity-40', !on);
        btn.classList.toggle('cursor-not-allowed', !on);
    }
}
clipboard.subscribe(refreshPasteButtons);

// --- State change listener ---

state.onChange((patternChanged) => {
    sequencer.render();
    updateLiveBtn();
    updateBpmDisplay();
    updatePlayButton();
//     updateBankDisplay();
    refreshArchetypeChips();
    refreshBankPushButton();
    // Any step-level edit on the active pattern → send to device immediately
    if (patternChanged) {
        sendActivePattern();
        recordHistory();
    }
});

// --- Ctrl+Z / Ctrl+Y ---

document.addEventListener('keydown', async (e) => {
    if (!e.ctrlKey && !e.metaKey) return;
    if (e.key === 'z' && !e.shiftKey) {
        e.preventDefault();
        const snap = await history.undo('progression');
        if (snap) {
            isRestoring = true;
            state.restoreSnapshot(snap);
            isRestoring = false;
            sendActivePattern();
            setStatus('Undo');
        } else {
            setStatus('Nothing to undo');
        }
    } else if (e.key === 'y' || (e.key === 'z' && e.shiftKey)) {
        e.preventDefault();
        const snap = await history.redo('progression');
        if (snap) {
            isRestoring = true;
            state.restoreSnapshot(snap);
            isRestoring = false;
            sendActivePattern();
            setStatus('Redo');
        } else {
            setStatus('Nothing to redo');
        }
    }
});

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
    // Fade out after 8 seconds
    setTimeout(() => { backupStatus.textContent = ''; }, 8000);
}

// --- Init ---

// Fetch env-driven UI defaults from server (best-effort). Stamp into DOM and
// state module before selectors.init/populateScaleSelect run so they see the
// env values and the first render reflects env-driven BPM/triplet/live-update.
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
} catch (err) {
    setStatus('Failed to fetch scratch pattern: ' + err.message);
}

await history.open();
await history.initCursor('progression');
// Record initial state so first undo has something to go back to
history.push('progression', state.getSnapshot());

selectors.init(state);
selectors.setScratch(scratch.group, scratch.pattern, scratch.side);
await loadScales();
populateScaleSelect(scaleSelect);
// Apply env scale default now that <option>s exist. The persisted-package
// branch below may override it - that's fine, package restore is canonical.
{
    const defaultScale = scaleSelect.dataset.defaultScale;
    if (defaultScale && [...scaleSelect.options].some(o => o.value === defaultScale)) {
        scaleSelect.value = defaultScale;
    }
}

// Restore the most recent persisted package so bassline preview buttons keep
// working after a page reload. Only applies the basslines if the stored
// package's label matches the current in-memory progression label - that
// guards against restoring stale basslines when the user has overwritten the
// acid patterns between sessions. Must run after populateScaleSelect so the
// scale <option>s exist before we try to select one by id.
try {
    await packageDb.open();
    const latest = await packageDb.getLatestPackage();
    if (latest && latest.package.label === state.getProgressionLabel()) {
        basslineMeta = { rhythmMode: latest.package.rhythmMode, derivationLog: [] };
        lastPackageId = latest.package.packageId;

        // Push the package's root + scaleId back into the selector DOM so the
        // visible controls agree with the restored label.
        const pkgRoot = Number.isInteger(latest.package.root) ? latest.package.root : null;
        if (pkgRoot !== null && pkgRoot >= 0 && pkgRoot <= 11) {
            rootSelect.value = String(pkgRoot);
        }
        const pkgScaleId = latest.package.scaleId;
        if (typeof pkgScaleId === 'string' && pkgScaleId) {
            const hasOption = Array.from(scaleSelect.options).some(o => o.value === pkgScaleId);
            if (hasOption) scaleSelect.value = pkgScaleId;
        }

        // v3 restore: if the package has 20 bassline rows, rebuild the full
        // 5×4 archetype map and push it into state. Legacy packages (4 rows,
        // no archetype field) fall through - state's sessionStorage copy of
        // basslines is already authoritative across reloads anyway.
        const reshape = packageDb.reshapeBasslines(latest.basslines);
        if (reshape) {
            const defaults = Array.isArray(latest.package.defaultArchetypeByPattern)
                && latest.package.defaultArchetypeByPattern.length === 4
                ? latest.package.defaultArchetypeByPattern
                : ['rootPulse', 'rootPulse', 'rootPulse', 'rootPulse'];
            state.setBasslines(reshape.byPattern, defaults);
        }

        // Rehydrate the export module so Save Package works immediately after
        // a reload without a fresh generation. For v3 packages we pass both
        // the active basslines (one per position) AND the flat 20-entry
        // matrix so combined SQS/RBS exports still get every archetype.
        // Legacy 4-row packages only restore the active slice.
        const acidPatterns = (latest.acidPatterns || []).map(row => row.pattern);
        const exportBasslines = reshape
            ? reshape.byPattern.map((set, i) => {
                const defaults = latest.package.defaultArchetypeByPattern || [];
                const key = defaults[i] || 'rootPulse';
                return set[key] || set.rootPulse;
            })
            : latest.basslines.map(b => b.pattern);
        const exportBasslinesFull = reshape
            ? flattenBasslineMatrix(reshape.byPattern)
            : undefined;
        if (acidPatterns.length === 4 && exportBasslines.length === 4) {
            packageExport.setPackage({
                packageId: lastPackageId,
                label: latest.package.label || '',
                scaleName: latest.package.scaleName || '',
                acidPatterns,
                basslines: exportBasslines,
                basslinesFull: exportBasslinesFull,
            });
        }
    }
} catch (err) {
    console.warn('[TD3-PROG] package restore failed:', err);
}

// Fallback sync: if sessionStorage retained a progression label but the
// IndexedDB restore above didn't fire (cleared DB, label mismatch), push the
// session-persisted root/scaleId into the selector DOM so the visible
// controls still match the active progression.
{
    const savedRoot = state.getProgressionRoot();
    if (Number.isInteger(savedRoot) && savedRoot >= 0 && savedRoot <= 11) {
        rootSelect.value = String(savedRoot);
    }
    const savedScaleId = state.getProgressionScaleId();
    if (typeof savedScaleId === 'string' && savedScaleId) {
        const hasOption = Array.from(scaleSelect.options).some(o => o.value === savedScaleId);
        if (hasOption) scaleSelect.value = savedScaleId;
    }
}

deviceBackup.init(setStatus, showBackupStatus);
midiStatus.init(state, setStatus, async () => {
    // Mode switch: send first timeline pattern to device so play starts correctly
    const tl = state.getTimeline();
    const firstPatIdx = (tl.length > 0 ? tl[0] : 1) - 1;
    try {
        await api.savePattern(
            scratch.group, scratch.pattern, scratch.side,
            state.getPattern(firstPatIdx)
        );
        setStatus(`P${firstPatIdx + 1} sent → ${scratch.label}`);
    } catch (err) {
        setStatus('Send error: ' + err.message);
    }
}, { autoConnect: !!envCfg && !!envCfg.uiAutoConnectToMidi });
timeline.init();
transport.init(setStatus, scratch);
packageExport.init({
    setStatus,
    exportFn: (payload) => api.exportProgressionPackage(payload),
});

// SEND TO PROGRESSION handoff - if the main page wrote a pattern into the
// sessionStorage handoff slot, install it as P1 and derive P2..P4 using
// the progression page's own config before the first render.
try {
    const progCfg = await api.getProgressionConfig();
    const consumed = consumeSendToProgressionHandoff({
        state, getScale, progressionConfig: progCfg,
        deriveSiblings, createRng, resolveProfile, chooseProgressionDegrees,
        toast,
    });
    if (consumed) {
        // Sync visible selectors so root/scale match the derived chain.
        const blobRoot = state.getProgressionRoot();
        if (Number.isInteger(blobRoot)) rootSelect.value = String(blobRoot);
        // Rank scales against P1 BEFORE assigning scaleSelect.value so the
        // detected scale ends up in the "Nearest to key" optgroup when set.
        const handoffPatterns = state.getPatterns();
        if (handoffPatterns && handoffPatterns[0] && Number.isInteger(blobRoot)) {
            rankScaleSelectFromP1(handoffPatterns[0], blobRoot);
        }
        const blobScaleId = state.getProgressionScaleId();
        if (typeof blobScaleId === 'string' && blobScaleId) {
            if (Array.from(scaleSelect.options).some(o => o.value === blobScaleId)) {
                scaleSelect.value = blobScaleId;
            }
        }
        // Stale persisted package must not survive a fresh chain - a new one
        // is built below via the shared bassline + package pipeline.
        packageExport.clearPackage();
        // Generate basslines and persist a package so SAVE PACKAGE is enabled
        // without requiring the user to press RANDOMIZE first. Any prior
        // RANDOMIZE-driven package was already cleared above.
        await generateBasslinesAndPersistPackage(consumed);
    }
} catch (err) {
    console.warn('[TD3-PROG] SEND TO PROGRESSION handoff failed:', err);
}

sequencer.render();
updateLiveBtn();
updateBpmDisplay();
updatePlayButton();
// updateBankDisplay();
refreshPasteButtons();
refreshArchetypeChips();
refreshBankPushButton();
initMidiPreviewVol();

// Resume playback animation if device was already playing (page switch)
if (state.isPlaying()) {
    transport.start().then(() => {
        updatePlayButton();
        setStatus('Resumed playback');
    }).catch(() => {});
} else {
    setStatus('Ready');
}
