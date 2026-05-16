// Pure selection helpers for the bank-import picker (sqs/rbs).
//
// Slice 10 extends the picker with `multi: true` - the grid accepts
// Ctrl+click / Shift+click ranges and commits an array of WebPattern.
// This module isolates the click-math + pattern-extraction so the DOM
// wiring in `import-bank-picker.js` stays thin and the selection rules
// are testable without a DOM.
//
// Rules (multi: true):
// - bare click   → reset selection to the clicked slot (anchor = clicked).
// - ctrl/meta    → toggle membership of the clicked slot (anchor = clicked).
// - shift+anchor → select every populated slot in grid order between the
//                  anchor and the clicked slot, inclusive. Anchor unchanged.
// - shift w/o anchor → treated as bare click.
// - empty slots are never selectable (they have no pattern payload).
//
// Rules (multi: false - default, backward compat):
// - any click on a populated slot → selection = just that slot.

/**
 * Compute the next selection state for a click in the bank picker grid.
 *
 * @param {Object} opts
 * @param {Array<{slot_key:string, empty?:boolean}>} opts.slots
 *          The slot list in grid order (64 cells for a parsed bank).
 * @param {Set<string>} opts.currentKeys
 *          The currently selected slot_keys (a fresh Set is returned; the
 *          caller's Set is not mutated).
 * @param {string|null} opts.anchorKey
 *          The anchor slot_key for shift+click ranges, or null when none.
 * @param {string} opts.clickedKey
 *          The slot_key the user just clicked.
 * @param {boolean} [opts.shiftKey=false]
 * @param {boolean} [opts.ctrlKey=false]
 *          Accepts ctrl or meta (the caller should OR them together).
 * @param {boolean} [opts.multi=false]
 *          When false, any click produces a single-element selection.
 * @returns {{ keys: Set<string>, anchorKey: string|null }}
 *          New selection set and the new anchor key.
 */
export function applyClick({
    slots,
    currentKeys,
    anchorKey,
    clickedKey,
    shiftKey = false,
    ctrlKey = false,
    multi = false,
}) {
    if (!Array.isArray(slots)) return { keys: new Set(), anchorKey: null };

    const clickedSlot = slots.find((s) => s && s.slot_key === clickedKey);
    if (!clickedSlot || clickedSlot.empty) {
        // Ignore clicks on empty/unknown slots - no state change.
        return { keys: new Set(currentKeys || []), anchorKey };
    }

    if (!multi) {
        return { keys: new Set([clickedKey]), anchorKey: clickedKey };
    }

    // --- multi mode --------------------------------------------------------

    if (shiftKey && anchorKey && anchorKey !== clickedKey) {
        const a = slots.findIndex((s) => s && s.slot_key === anchorKey);
        const b = slots.findIndex((s) => s && s.slot_key === clickedKey);
        if (a < 0 || b < 0) {
            return { keys: new Set(currentKeys || []), anchorKey };
        }
        const [lo, hi] = a <= b ? [a, b] : [b, a];
        const next = new Set(currentKeys || []);
        for (let i = lo; i <= hi; i++) {
            const s = slots[i];
            if (s && !s.empty) next.add(s.slot_key);
        }
        // Anchor stays put - a range extends from the last plain click.
        return { keys: next, anchorKey };
    }

    if (ctrlKey) {
        const next = new Set(currentKeys || []);
        if (next.has(clickedKey)) next.delete(clickedKey);
        else                      next.add(clickedKey);
        return { keys: next, anchorKey: clickedKey };
    }

    // Bare click in multi mode: reset to just this slot.
    return { keys: new Set([clickedKey]), anchorKey: clickedKey };
}

/**
 * Extract the WebPattern payloads from a selection, preserving grid order.
 * Empty slots and slots missing a `pattern` field are filtered out.
 *
 * @param {Array<{slot_key:string, empty?:boolean, pattern?:object}>} slots
 * @param {Set<string>|Iterable<string>} selectedKeys
 * @returns {Array<object>} WebPattern[] in slot (grid) order.
 */
export function patternsFromSelection(slots, selectedKeys) {
    if (!Array.isArray(slots)) return [];
    const keys = selectedKeys instanceof Set
        ? selectedKeys
        : new Set(selectedKeys || []);
    if (keys.size === 0) return [];
    const out = [];
    for (const s of slots) {
        if (!s || s.empty || !s.pattern) continue;
        if (keys.has(s.slot_key)) out.push(s.pattern);
    }
    return out;
}
