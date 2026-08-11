// Derived triplet morph renderer for the four progression pattern rows.
//
// Same contract as the Control renderer: the canonical pattern data is
// never mutated for visual convenience. Step columns are positioned
// along the four-beat row from the cached Rust plan's rational offsets
// plus the current amount, the losing cell shows merge state and
// disappears only at the 100% endpoint, and canonical grid editing is
// locked while the amount is above zero. At 0 every derived style is
// cleared.

import { api } from '../api.js';
import * as state from './progression-state.js';
import { canonicalPatternText } from '../shared/pattern-canonical.js';
// The rows use the same step column markup as the Control list, so the
// geometry pass is shared: both pages move a note card and its
// UP/DN/SL/AC control block together.
import {
    applyToCells,
    markEditableControls,
} from '../multipattern/triplet-morph-view.js';
import { isFullyLocked } from '../shared/triplet-morph-editing.js';

const PATTERN_COUNT = 4;

let doc = null;
const plansInFlight = new Set();

export { applyToCells };

/**
 * Request and cache the Rust plan for every acid pattern that lacks
 * one. A response whose session went stale is ignored without touching
 * the UI.
 */
export function ensurePlans() {
    if (!state.isTripletMorphActive()) return;
    const seen = new Set();
    for (let idx = 0; idx < PATTERN_COUNT; idx += 1) {
        const pattern = state.getPattern(idx);
        if (!pattern) continue;
        const text = canonicalPatternText(pattern);
        if (seen.has(text) || state.getTripletMorphPlan(text) || plansInFlight.has(text)) {
            continue;
        }
        seen.add(text);
        plansInFlight.add(text);
        api.tripletMorphPlan(pattern)
            .then((response) => {
                plansInFlight.delete(text);
                if (!state.isTripletMorphActive()) return;
                if (!response?.eligible || !response.plan) return;
                state.setTripletMorphPlan(text, response.plan);
                render();
            })
            .catch(() => {
                plansInFlight.delete(text);
            });
    }
}

/** Plan for the pattern at `idx`, or null while the request is pending. */
export function planForPattern(idx) {
    const pattern = state.getPattern(idx);
    if (!pattern) return null;
    return state.getTripletMorphPlan(canonicalPatternText(pattern));
}

export function render() {
    if (!doc) return;
    const amount = state.getTripletMorphPercent();
    if (amount > 0) ensurePlans();
    for (let idx = 0; idx < PATTERN_COUNT; idx += 1) {
        const grid = doc.getElementById(`grid-p${idx + 1}`);
        if (!grid) continue;
        const cells = [];
        for (let step = 0; step < 16; step += 1) {
            cells.push(grid.querySelector(`[data-step="${step}"]`));
        }
        const plan = amount > 0 ? planForPattern(idx) : null;
        applyToCells(cells, plan, amount);
        markEditableControls(grid, plan, amount);
        // Mid-transform every position is unsettled, so the grid stops
        // taking pointer input entirely. At the endpoint the surviving
        // cells are editable again and the per-step gate decides which.
        grid.style.pointerEvents = isFullyLocked(amount, plan) ? 'none' : '';
        if (amount > 0) grid.dataset.morphAmount = String(amount);
        else delete grid.dataset.morphAmount;
    }
}

export function init({ documentRef = globalThis.document } = {}) {
    doc = documentRef;
    state.onChange(() => render());
    render();
}
