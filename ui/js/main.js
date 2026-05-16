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
import * as transport from './transport.js';
import * as selectors from './selectors.js';
import * as randomize from './randomize.js';
import * as midiStatus from './midi-status.js';
import * as keyboard from './keyboard.js';
import * as history from './history.js';
import * as deviceBackup from './device-backup.js';
import { api } from './api.js';
import { bankApi } from './bank/bank-api.js';
import { subscribeControlQueue } from './shared/add-to-control.js';
import { loadAppConfig, applyUiDefaults } from './app-config.js';
import { openImportBankPicker } from './import-bank-picker.js';
import { detectKey, formatKey, buildPitchClassHistogram, CONFIDENCE_HIGH } from './key-detection.js';
import { rankScales, applyRankedOrder, resetToDefaultOrder } from './scale-ranking.js';
import { getAllScales, getTagGroups } from './scales.js';
import { formatPatternAsStepsTxt } from './shared/steps-txt-format.js';
import { parseStepsTxt, looksLikeStepsTxt } from './shared/steps-txt-parse.js';

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
        await navigator.clipboard.writeText(formatPatternAsStepsTxt(pat));
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
        const pat = parseStepsTxt(text);
        state.setPattern(focusedIdx, pat);
        return true;
    } catch (_) {
        return false;
    }
}

const statusLog = document.getElementById('status-log');
const activeStepsInput = document.getElementById('active-steps');
const tripletToggle = document.getElementById('triplet-toggle');
const btnReset = document.getElementById('btn-reset');
const btnLive = document.getElementById('btn-live');
const slicerInput = document.getElementById('slicer-input');
const btnSlicer = document.getElementById('btn-slicer');
const bankCount = document.getElementById('bank-count');
const bankSizeInput = document.getElementById('bank-size');
const btnSavePool = document.getElementById('btn-save-pool');
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
function scheduleLiveSave() {
    if (!state.isLiveUpdate() || !state.isConnected()) return;
    clearTimeout(liveTimer);
    liveTimer = setTimeout(async () => {
        try {
            await api.savePattern(
                scratch.group, scratch.pattern, scratch.side,
                state.getPattern()
            );
            setStatus('Live sent → ' + scratch.label);
        } catch (err) {
            setStatus('Live error: ' + err.message);
        }
    }, 150);
}

// Update the LIVE button appearance
function updateLiveBtn() {
    btnLive.classList.toggle('is-active', state.isLiveUpdate());
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

// Update bank counter display
// function updateBankDisplay() {
//     bankCount.textContent = state.getBankCount();
// }

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
    const tripletAllOn = isTripletAllOnForTargets();
    tripletToggle.textContent = tripletAllOn ? 'ON' : 'OFF';
    tripletToggle.classList.toggle('is-active', tripletAllOn);
    if (patternChanged) {
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
    state.setAllActiveSteps(parseInt(activeStepsInput.value) || 16);
});

// Scroll wheel over the STEPS input nudges the count by 1 (clamped to
// 1..=16) and applies it to every pattern. preventDefault keeps the page
// from scrolling while the pointer sits over the input.
activeStepsInput.addEventListener('wheel', (e) => {
    e.preventDefault();
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
tripletToggle.addEventListener('click', () => {
    const targets = bulkTargets();
    if (targets.length === 0) return;
    const next = !targets.every((i) => state.getTriplet(i));
    state.setTripletBulk(targets, next);
    setStatus(bulkLabel(`Triplet ${next ? 'ON' : 'OFF'}`));
});

function isTripletAllOnForTargets() {
    const targets = bulkTargets();
    if (targets.length === 0) return false;
    return targets.every((i) => state.getTriplet(i));
}

// Live update toggle
btnLive.addEventListener('click', () => {
    state.setLiveUpdate(!state.isLiveUpdate());
    setStatus(state.isLiveUpdate() ? 'Live update ON' : 'Live update OFF');
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
btnRandRst.addEventListener('click', () => { randomize.randomizeCategory('rst'); setStatus('Randomized rests'); });
btnRandSl.addEventListener('click',  () => { randomize.randomizeCategory('sl');  setStatus('Randomized slides'); });
btnRandAcc.addEventListener('click', () => { randomize.randomizeCategory('ac');  setStatus('Randomized accents'); });
if (btnRandUd) {
    btnRandUd.addEventListener('click', () => { randomize.randomizeCategory('ud'); setStatus('Randomized UP/DOWN'); });
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
btnShiftBack4.addEventListener('click', () => { state.shiftStepsBulk(bulkTargets(), -4); setStatus(bulkLabel('Shifted back 4')); });
btnShiftBack2.addEventListener('click', () => { state.shiftStepsBulk(bulkTargets(), -2); setStatus(bulkLabel('Shifted back 2')); });
btnShiftBack1.addEventListener('click', () => { state.shiftStepsBulk(bulkTargets(), -1); setStatus(bulkLabel('Shifted back 1')); });
btnShiftFwd1.addEventListener('click',  () => { state.shiftStepsBulk(bulkTargets(),  1); setStatus(bulkLabel('Shifted forward 1')); });
btnShiftFwd2.addEventListener('click',  () => { state.shiftStepsBulk(bulkTargets(),  2); setStatus(bulkLabel('Shifted forward 2')); });
btnShiftFwd4.addEventListener('click',  () => { state.shiftStepsBulk(bulkTargets(),  4); setStatus(bulkLabel('Shifted forward 4')); });
if (btnShuffleAll) {
    btnShuffleAll.addEventListener('click', () => {
        state.shuffleStepsBulk(bulkTargets());
        setStatus(bulkLabel('Shuffled steps'));
    });
}

// Transpose ±1 / ±12 semitones - mutates step.note only, preserves
// step.transpose. Same checked-or-all semantics as SHIFT.
btnTrnspsUp.addEventListener('click',   () => { state.transposeBulk(bulkTargets(), +1);  setStatus(bulkLabel('Transposed +1')); });
btnTrnspsDn.addEventListener('click',   () => { state.transposeBulk(bulkTargets(), -1);  setStatus(bulkLabel('Transposed −1')); });
btnTrnspsUp12.addEventListener('click', () => { state.transposeBulk(bulkTargets(), +12); setStatus(bulkLabel('Transposed +12')); });
btnTrnspsDn12.addEventListener('click', () => { state.transposeBulk(bulkTargets(), -12); setStatus(bulkLabel('Transposed −12')); });

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
// bankSizeInput.addEventListener('change', () => {
//     state.setBankSize(parseInt(bankSizeInput.value) || 100);
// });

// RESET ALL PATTERNS - destructive, clears every card in the multipattern
// list back to the factory default. The secondary toolbar's narrower
// RESET PATTERN (N) button targets only the current selection.
btnReset.addEventListener('click', () => {
    state.resetAllPatterns();
    setStatus('All patterns reset');
});

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

btnImport.addEventListener('click', () => fileImport.click());

fileImport.addEventListener('change', async () => {
    const file = fileImport.files[0];
    if (!file) return;
    const name = file.name.toLowerCase();
    // fmt → backend format key. `binary` selects file.arrayBuffer() vs
    // file.text() and routes the payload into `bytes` vs `content`.
    // Bank formats (.sqs/.rbs) hold up to 64 patterns and take the picker
    // path below instead of routing through /api/pattern/import.
    let fmt, binary = false, bank = false;
    if (name.endsWith('.toml')) fmt = 'toml';
    else if (name.endsWith('.json')) fmt = 'json';
    else if (name.endsWith('.steps.txt') || name.endsWith('.txt')) fmt = 'steps';
    else if (name.endsWith('.pat')) fmt = 'pat';
    else if (name.endsWith('.seq')) { fmt = 'seq'; binary = true; }
    else if (name.endsWith('.mid') || name.endsWith('.midi')) { fmt = 'mid'; binary = true; }
    else if (name.endsWith('.sqs')) { fmt = 'sqs'; bank = true; }
    else if (name.endsWith('.rbs')) { fmt = 'rbs'; bank = true; }
    else {
        setStatus('Unsupported file type (use .toml, .json, .steps.txt, .pat, .seq, .mid, .sqs, or .rbs)');
        fileImport.value = '';
        return;
    }
    try {
        if (bank) {
            // Bank files (.sqs/.rbs) may hold up to 64 patterns. Use the
            // multi-select picker - Ctrl/Shift click picks an arbitrary
            // subset, each chosen pattern is appended at the end of the
            // multipattern list. Focus lands on the first imported
            // pattern so the card list scrolls to the newcomer.
            setStatus(`Parsing ${file.name}...`);
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
                        if (idx == null) break;      // hit 64-cap
                        if (firstIdx === null) firstIdx = idx;
                        appended++;
                    }
                    if (firstIdx !== null) state.setFocused(firstIdx);
                    const keyNote = firstIdx !== null
                        ? applyKeyDetection(patterns[0])
                        : '';
                    if (appended < patterns.length) {
                        setStatus(`Imported ${appended}/${patterns.length} from ${file.name} (64-pattern cap reached)${keyNote}`);
                    } else {
                        setStatus(`Imported ${appended} from ${file.name}${keyNote}`);
                    }
                },
            });
        } else {
            setStatus('Importing...');
            const payload = { format: fmt };
            if (binary) {
                const buf = await file.arrayBuffer();
                payload.bytes = Array.from(new Uint8Array(buf));
            } else {
                payload.content = await file.text();
            }
            const res = await api.importPattern(payload);
            // Single-format import appends one new pattern at the end of
            // the multipattern list. Focus follows the newcomer.
            const idx = state.appendPattern(res.pattern);
            if (idx == null) {
                setStatus('Cannot import: 64-pattern cap reached');
            } else {
                const keyNote = applyKeyDetection(res.pattern);
                setStatus(`Imported ${file.name}${keyNote}`);
            }
        }
    } catch (err) {
        setStatus('Import error: ' + err.message);
    }
    fileImport.value = '';
});

// Export pattern - dropdown delegates per-format click to /api/pattern/export
// and triggers a browser download. Filename encodes current sidebar selection
// so round-tripped files stay distinguishable.
const exportPanel = document.getElementById('export-format-panel');
if (exportPanel) {
    exportPanel.addEventListener('click', async (ev) => {
        const btn = ev.target.closest('button[data-format]');
        if (!btn) return;
        const format = btn.dataset.format;
        const ext = btn.dataset.ext || format;
        try {
            setStatus(`Exporting ${format}...`);
            const blob = await api.exportPattern(state.getPattern(), format);
            const g = state.getGroup();
            const p = state.getPatternNum();
            const s = state.getSide();
            const filename = `pattern_G${g}P${p}${s}.${ext}`;
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            document.body.appendChild(a);
            a.click();
            a.remove();
            URL.revokeObjectURL(url);
            setStatus(`Exported ${filename}`);
        } catch (err) {
            setStatus('Export error: ' + err.message);
        }
    });
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
    setStatus,
    onBankPattern: (idx) => multipatternBank.openSingleToBank(idx),
    // Drag-to-reorder during timeline playback - re-queue the new next
    // pattern into scratch so the device wraps into the right buffer.
    onStructuralChange: () => transport.rescratchUpcoming(),
});
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
    () => document.getElementById('btn-play').click()
);

// Initial chrome paint - the card list already rendered via
// multipatternList.init() above.
updateLiveBtn();
updateSlicerBtn();
updateKbToggles();
// updateBankDisplay();
slicerInput.value = state.getSliceText();
// bankSizeInput.value = state.getBankSize();
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
