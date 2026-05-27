// LOAD / LOAD ALL / SAVE on the main Control page.
//
// Rewires three existing toolbar buttons to the new multipattern state
// model. Each handler is purely a DOM/chrome thin layer on top of state
// and the backend MIDI API - the pure routing decisions live in
// `resolveSaveAction` (unit-tested) so future changes to selection
// semantics stay verifiable without a browser.
//
// Button semantics:
//
//   LOAD - Reads the sidebar-selected device slot and APPENDS it
//     as a new UI pattern at index N. Focus moves to the new card.
//     Disabled when MIDI is disconnected or N === 64 (cap reached).
//
//   LOAD ALL - Reads every populated device slot into the
//     UI in canonical A/B order. Scratch is INCLUDED (not excluded;
//     LOAD/PUSH round-trip, exclusion only kicks in on
//     PUSH). A confirm-plus-radio modal lets the user choose A/B mode
//     before the order walk so the new badges line up. If any existing
//     UI pattern is non-default, a warning paragraph flags that the
//     replacement will discard unsaved work.
//
//   SAVE - Context-sensitive on |selection|:
//     0 → disabled (nothing to write)
//     1 → single-slot write to the sidebar-selected slot, toast
//         `Saved P{i} → G1P1A` on success
//     ≥2 → opens the shared PUSH modal pre-populated with the checked
//         set only (not every UI pattern - that's what PUSH does) and
//         their scratch-excluded slot assignments.
//
// Note: LOAD ALL walks 64 slots sequentially via api.loadPattern. There
// is no bulk endpoint today. Progress flows through setStatus between
// reads so the user sees forward motion on slow links.

import { slotFor, orderedSlots } from '../shared/slot-targets.js';
import { openModal } from '../bank/bank-modal.js';
import { toast } from '../bank/bank-toast.js';
import { openPushToTd3Modal } from '../shared/push-to-td3-modal.js';

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested in multipattern-device-io.test.js)
// ---------------------------------------------------------------------------

/**
 * Decide what SAVE should do, given the current selection. Pure; no DOM,
 * no state mutation. Returns one of:
 *   { kind: 'none' }     - nothing selected, button must be disabled.
 *   { kind: 'single', index: i }
 *                        - write patterns[i] to the sidebar slot.
 *   { kind: 'multi', indexes: [i0, i1, ...] }
 *                        - open the push modal with those patterns.
 *
 * @param {number[]} selectionIndexes  from state.getSelectionIndexes()
 */
export function resolveSaveAction(selectionIndexes) {
    if (!Array.isArray(selectionIndexes) || selectionIndexes.length === 0) {
        return { kind: 'none' };
    }
    if (selectionIndexes.length === 1) {
        return { kind: 'single', index: selectionIndexes[0] };
    }
    return { kind: 'multi', indexes: [...selectionIndexes].sort((a, b) => a - b) };
}

/**
 * Build targets for the checked-set SAVE (kind=multi). Walks the canonical
 * A/B ordering starting at index 0 and emits targets for *only* the checked
 * indexes (not the whole list - that's what PUSH does). Returns
 * `{ targets, patternsToWrite, error }` where `patternsToWrite[i]` aligns
 * with `targets[i]`.
 *
 * @param {number[]} indexes           sorted checked indexes (from resolveSaveAction multi)
 * @param {Array<Pattern>} patterns    full pattern list from state.getPatterns()
 * @param {{group,pattern,side,label}|null} scratch
 * @param {'ALTERNATE'|'SERIAL'} mode
 */
export function buildSaveSelectionTargets(indexes, patterns, scratch, mode) {
    if (!Array.isArray(indexes) || indexes.length < 2) return { targets: null, patternsToWrite: null, error: 'bad-indexes' };
    if (!Array.isArray(patterns)) return { targets: null, patternsToWrite: null, error: 'bad-patterns' };
    const targets = [];
    const patternsToWrite = [];
    for (const i of indexes) {
        if (!Number.isInteger(i) || i < 0 || i >= patterns.length) {
            return { targets: null, patternsToWrite: null, error: 'index-out-of-range' };
        }
        const t = slotFor(i, scratch, mode);
        if (!t) {
            // Overflow (i===63 with scratch present) - SAVE's upper bound
            // is |selection| ≤ N-1 < 64 in practice, but guard anyway so a
            // 64-pattern bank with a full checked set fails loud rather
            // than silently dropping a slot.
            return { targets: null, patternsToWrite: null, error: 'overflow' };
        }
        targets.push(t);
        patternsToWrite.push(patterns[i]);
    }
    return { targets, patternsToWrite, error: null };
}

// ---------------------------------------------------------------------------
// DOM / runtime init
// ---------------------------------------------------------------------------

/**
 * @param {Object} opts
 * @param {Object} opts.state     Multipattern state module.
 * @param {Object} opts.api       Backend API - needs loadPattern, savePattern.
 * @param {Function} opts.setStatus
 */
export function init({ state, api, setStatus }) {
    const btnLoad = document.getElementById('btn-load');
    const btnLoadAll = document.getElementById('btn-load-all');
    const btnSave = document.getElementById('btn-save');

    if (btnLoad) wireLoad(btnLoad, { state, api, setStatus });
    if (btnLoadAll) wireLoadAll(btnLoadAll, { state, api, setStatus });
    if (btnSave) wireSave(btnSave, { state, api, setStatus });

    function updateChrome() {
        if (btnLoad) updateLoadChrome(btnLoad, state);
        if (btnLoadAll) updateLoadAllChrome(btnLoadAll, state);
        if (btnSave) updateSaveChrome(btnSave, state);
    }
    state.onChange(updateChrome);
    updateChrome();
}

// --- LOAD (single) ---------------------------------------------------------

function updateLoadChrome(btn, state) {
    const connected = state.isConnected();
    const n = state.getPatterns().length;

    let disabled = false;
    let title = 'Load the selected device slot as a new pattern (appended)';

    if (!connected) {
        disabled = true;
        title = 'Connect MIDI first';
    } else if (n >= 64) {
        disabled = true;
        title = 'Pattern list is at the 64 cap - DEL one before loading another';
    }

    btn.disabled = disabled;
    btn.title = title;
    btn.classList.toggle('opacity-50', disabled);
    btn.classList.toggle('cursor-not-allowed', disabled);
}

function wireLoad(btn, { state, api, setStatus }) {
    btn.addEventListener('click', async () => {
        if (btn.disabled) return;
        if (!state.isConnected()) { setStatus('Connect MIDI first'); return; }
        if (state.getPatterns().length >= 64) { setStatus('Pattern list at cap - DEL one first'); return; }

        const g = state.getGroup();
        const p = state.getPatternNum();
        const s = state.getSide();

        try {
            setStatus(`Loading G${g + 1}P${p + 1}${s === 1 ? 'B' : 'A'}…`);
            const res = await api.loadPattern(g, p, s);
            const newIdx = state.appendPattern(res.pattern);
            if (newIdx === null) {
                setStatus('Load failed: could not append (bad pattern or cap)');
                return;
            }
            setStatus(`Loaded ${res.address} → P${newIdx + 1}`);
        } catch (err) {
            setStatus(`Load error: ${err.message || err}`);
        }
    });
}

// --- LOAD ALL --------------------------------------------------------------

function updateLoadAllChrome(btn, state) {
    const connected = state.isConnected();
    let disabled = false;
    let title = 'Read all 64 device slots into the UI (confirm first)';
    if (!connected) { disabled = true; title = 'Connect MIDI first'; }
    btn.disabled = disabled;
    btn.title = title;
    btn.classList.toggle('opacity-50', disabled);
    btn.classList.toggle('cursor-not-allowed', disabled);
}

function wireLoadAll(btn, { state, api, setStatus }) {
    btn.addEventListener('click', async () => {
        if (btn.disabled) return;
        if (!state.isConnected()) { setStatus('Connect MIDI first'); return; }
        const choice = await openLoadAllConfirm({
            hasNonDefault: state.hasNonDefaultPatterns(),
            currentCount: state.getPatterns().length,
            currentMode: state.getAbMode(),
        });
        if (!choice) return; // user cancelled

        // Commit the mode pick BEFORE the order walk so badges (if the user
        // peeks at the card list during load) line up with the incoming
        // addresses.
        state.setAbMode(choice.mode);

        setStatus('Loading all 64 patterns…');
        const loaded = [];
        // Walk the canonical order matching the just-set mode; scratch NOT
        // excluded on LOAD.
        const order = orderedSlots(choice.mode);
        for (let i = 0; i < order.length; i++) {
            const s = order[i];
            try {
                setStatus(`Loading ${i + 1}/64 (${s.label})…`);
                const res = await api.loadPattern(s.group, s.pattern, s.side);
                loaded.push(res.pattern);
            } catch (err) {
                setStatus(`Load aborted at ${s.label}: ${err.message || err}`);
                return;
            }
        }
        const ok = state.replaceAllPatterns(loaded);
        if (!ok) {
            setStatus('Load finished but UI replace failed - patterns may be malformed');
            return;
        }
        setStatus(`Loaded 64 patterns (${choice.mode === 'ALTERNATE' ? 'A/B alternate' : 'serial'})`);
    });
}

/**
 * Render the LOAD ALL confirm + A/B mode picker.
 * @returns {Promise<{mode:'ALTERNATE'|'SERIAL'} | null>}
 */
function openLoadAllConfirm({ hasNonDefault, currentCount, currentMode }) {
    return new Promise((resolve) => {
        const body = document.createElement('div');
        body.className = 'bank-confirm-body';

        const p1 = document.createElement('p');
        p1.textContent = `Replace all ${currentCount} pattern${currentCount === 1 ? '' : 's'} in the UI with 64 patterns from the device?`;
        body.appendChild(p1);

        if (hasNonDefault) {
            const warn = document.createElement('p');
            warn.className = 'bank-warn';
            warn.textContent = 'This discards unsaved edits in the current pattern list.';
            body.appendChild(warn);
        }

        const pickerLabel = document.createElement('p');
        pickerLabel.innerHTML = '<strong>A/B mode</strong> for the 64-slot read order:';
        body.appendChild(pickerLabel);

        const picker = document.createElement('div');
        picker.className = 'bank-radio-group';
        picker.style.display = 'flex';
        picker.style.flexDirection = 'column';
        picker.style.gap = '0.25rem';
        picker.style.marginBottom = '0.5rem';

        const mkRadio = (value, label, checked) => {
            const row = document.createElement('label');
            row.style.display = 'flex';
            row.style.alignItems = 'center';
            row.style.gap = '0.5rem';
            const input = document.createElement('input');
            input.type = 'radio';
            input.name = 'load-all-mode';
            input.value = value;
            if (checked) input.checked = true;
            const txt = document.createElement('span');
            txt.textContent = label;
            row.appendChild(input);
            row.appendChild(txt);
            return { row, input };
        };

        const alt = mkRadio('ALTERNATE', 'A/B alternate - G1P1A, G1P1B, G1P2A, G1P2B, …', currentMode !== 'SERIAL');
        const ser = mkRadio('SERIAL', 'As then Bs - G1P1A, G1P2A, …, G4P8A, G1P1B, …', currentMode === 'SERIAL');
        picker.appendChild(alt.row);
        picker.appendChild(ser.row);
        body.appendChild(picker);

        openModal({
            title: 'Load all 64 patterns from TD-3',
            body,
            primaryLabel: 'Load all',
            secondaryLabel: 'Cancel',
            danger: hasNonDefault,
            onPrimary: () => {
                const mode = ser.input.checked ? 'SERIAL' : 'ALTERNATE';
                resolve({ mode });
            },
            onSecondary: () => resolve(null),
        });
    });
}

// --- SAVE ------------------------------------------------------------------

function updateSaveChrome(btn, state) {
    const connected = state.isConnected();
    const action = resolveSaveAction(state.getSelectionIndexes());

    let disabled = false;
    let title;

    if (!connected) {
        disabled = true;
        title = 'Connect MIDI first';
    } else if (action.kind === 'none') {
        disabled = true;
        title = 'Nothing selected - check patterns or focus one';
    } else if (action.kind === 'single') {
        title = `Save P${action.index + 1} to the sidebar-selected slot`;
    } else {
        title = `Save ${action.indexes.length} checked pattern(s) to their assigned slots`;
    }

    btn.disabled = disabled;
    btn.title = title;
    btn.classList.toggle('opacity-50', disabled);
    btn.classList.toggle('cursor-not-allowed', disabled);
}

function wireSave(btn, { state, api, setStatus }) {
    btn.addEventListener('click', async () => {
        if (btn.disabled) return;
        if (!state.isConnected()) { setStatus('Connect MIDI first'); return; }

        const action = resolveSaveAction(state.getSelectionIndexes());
        if (action.kind === 'none') { setStatus('Nothing to save'); return; }

        if (action.kind === 'single') {
            const g = state.getGroup();
            const p = state.getPatternNum();
            const s = state.getSide();
            const pattern = state.getPattern(action.index);
            if (!pattern) { setStatus(`Save failed: P${action.index + 1} missing`); return; }
            try {
                setStatus(`Saving P${action.index + 1}…`);
                const res = await api.savePattern(g, p, s, pattern);
                setStatus(`Saved P${action.index + 1} → ${res.address}`);
                toast(`Saved P${action.index + 1} → ${res.address}`);
            } catch (err) {
                setStatus(`Save error: ${err.message || err}`);
            }
            return;
        }

        // Multi: reuse the shared PUSH modal with scratch-excluded targets
        // computed for *only* the checked indexes. Scratch must be present
        // before we can hand off - the modal walks targets[i] directly and
        // we can't compute them without it.
        const scratch = state.getScratchSlot();
        if (!scratch) { setStatus('Scratch slot not yet known - retry in a moment'); return; }
        const mode = state.getAbMode();
        const patterns = state.getPatterns();

        const { targets, patternsToWrite, error } = buildSaveSelectionTargets(
            action.indexes, patterns, scratch, mode,
        );
        if (error || !targets) {
            setStatus(`Save aborted: ${error || 'no targets'}`);
            return;
        }

        openPushToTd3Modal({
            title: `Save ${action.indexes.length} checked patterns to TD-3`,
            introText:
                `This will overwrite the following ${action.indexes.length} device slot(s) `
                + `with the checked patterns (in UI-index order):`,
            patterns: patternsToWrite,
            targets,
            api,
            scratchLabel: scratch.label,
            setStatus,
        });
    });
}
