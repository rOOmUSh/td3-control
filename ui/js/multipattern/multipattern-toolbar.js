// Secondary toolbar wiring for the main Control page.
//
// Handlers for:
//   - ADD            - state.addPattern()
//   - DUPLICATE      - state.duplicatePattern()  (source = focused)
//   - DEL            - state.deletePattern()     (target = focused)
//   - RESET PATTERN  - resets the current selection:
//                      every checked pattern if non-empty, else focused.
//                      Button label flips between
//                      "RESET FOCUSED" and "RESET PATTERN (N)".
//
// SHIFT STEPS + TRNSPS are wired in main.js against their preserved IDs.
// They use the bulk variants (state.shiftStepsBulk / state.transposeBulk)
// with checkbox-aware fallback: ≥1 checked → just those, else ALL
// patterns. Per-card SHIFT/TRNSPS still target a single pattern.
//
// DUPLICATE and DEL are checkbox-aware too: ≥1 checked → bulk
// (state.duplicateCheckedToBottom / state.deleteCheckedPatterns), else
// the focused-only single-pattern path.

import * as state from './multipattern-state.js';

const MAX = 64;

export function init({ setStatus } = {}) {
    const status = setStatus || (() => {});

    const btnAdd       = document.getElementById('btn-mp-add');
    const btnDuplicate = document.getElementById('btn-mp-duplicate');
    const btnDel       = document.getElementById('btn-mp-del');
    const btnReset     = document.getElementById('btn-mp-reset-selection');

    if (!btnAdd || !btnDuplicate || !btnDel || !btnReset) {
        console.warn('[multipattern-toolbar] secondary toolbar buttons missing');
        return;
    }

    btnAdd.addEventListener('click', () => {
        if (!state.addPattern()) {
            status(`Can't add - already at max (${MAX})`);
            return;
        }
        status(`Added pattern P${state.getPatternCount()}`);
    });

    btnDuplicate.addEventListener('click', () => {
        const checkedCount = state.getCheckedSet().size;
        if (checkedCount > 0) {
            const before = state.getPatternCount();
            const added = state.duplicateCheckedToBottom();
            if (added === 0) {
                status(`Can't duplicate - already at max (${MAX})`);
                return;
            }
            const truncated = added < checkedCount;
            status(truncated
                ? `Duplicated ${added}/${checkedCount} (cap ${MAX})`
                : `Duplicated ${added} pattern${added === 1 ? '' : 's'} → P${before + 1}..P${before + added}`);
            return;
        }
        const src = state.getFocusedIdx();
        if (src === null) { status('Nothing focused to duplicate'); return; }
        if (!state.duplicatePattern()) {
            status(`Can't duplicate - already at max (${MAX})`);
            return;
        }
        // Focus lands on the copy.
        status(`Duplicated P${src + 1} → P${state.getFocusedIdx() + 1}`);
    });

    btnDel.addEventListener('click', () => {
        const checkedCount = state.getCheckedSet().size;
        if (checkedCount > 0) {
            const removed = state.deleteCheckedPatterns();
            status(removed === 0
                ? 'Nothing deleted'
                : `Deleted ${removed} pattern${removed === 1 ? '' : 's'}`);
            return;
        }
        const cur = state.getFocusedIdx();
        if (cur === null) { status('Nothing focused to delete'); return; }
        const wasOnly = state.getPatternCount() <= 1;
        state.deletePattern();
        status(wasOnly ? 'Reset sole pattern (N ≥ 1)'
                       : `Deleted P${cur + 1}`);
    });

    // RESET PATTERN (N): reset every index in the current selection.
    // All checked if non-empty, else focused.
    btnReset.addEventListener('click', () => {
        const sel = state.getSelectionIndexes();
        if (sel.length === 0) { status('Nothing selected to reset'); return; }
        for (const i of sel) state.resetPattern(i);
        status(sel.length === 1
            ? `Reset P${sel[0] + 1}`
            : `Reset ${sel.length} patterns`);
    });

    // Keep disabled state + labels in sync with state mutations.
    const syncChrome = () => {
        const n = state.getPatternCount();
        const focused = state.getFocusedIdx();
        const selSize = state.getCheckedSet().size;

        btnAdd.disabled       = n >= MAX;
        btnDuplicate.disabled = n >= MAX || focused === null;
        btnDel.disabled       = n === 0;
        btnReset.disabled     = state.getSelectionIndexes().length === 0;

        // RESET label: "RESET FOCUSED" when no checks (I21), otherwise
        // "RESET PATTERN (N)" where N = |checked|.
        btnReset.textContent = selSize === 0
            ? 'RESET FOCUSED'
            : `RESET PATTERN (${selSize})`;
    };

    state.onChange(syncChrome);
    syncChrome();
}
