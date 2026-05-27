// Secondary toolbar wiring for the main Control page.
//
// Handlers for:
//   - ADD            - state.addPattern()
//   - DUPLICATE      - state.duplicatePattern()  (source = focused)
//   - DEL            - state.deletePattern()     (target = focused)
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

    const checkAll     = document.getElementById('mp-check-all');
    const btnAdd       = document.getElementById('btn-mp-add');
    const btnDuplicate = document.getElementById('btn-mp-duplicate');
    const btnDel       = document.getElementById('btn-mp-del');

    if (!checkAll || !btnAdd || !btnDuplicate || !btnDel) {
        console.warn('[multipattern-toolbar] secondary toolbar buttons missing');
        return;
    }

    checkAll.addEventListener('click', (event) => event.stopPropagation());
    checkAll.addEventListener('change', () => {
        state.setAllChecked(checkAll.checked);
        const checkedCount = state.getCheckedSet().size;
        status(checkedCount === 0
            ? 'Selection cleared'
            : `Checked ${checkedCount} pattern${checkedCount === 1 ? '' : 's'}`);
    });

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

    // Keep disabled state + labels in sync with state mutations.
    const syncChrome = () => {
        const n = state.getPatternCount();
        const focused = state.getFocusedIdx();
        const checkedCount = state.getCheckedSet().size;

        checkAll.disabled = n === 0;
        checkAll.checked = n > 0 && checkedCount === n;
        checkAll.indeterminate = checkedCount > 0 && checkedCount < n;
        btnAdd.disabled       = n >= MAX;
        btnDuplicate.disabled = n >= MAX || focused === null;
        btnDel.disabled       = n === 0;
    };

    state.onChange(syncChrome);
    syncChrome();
}
