// TRIPLET button behaviour when the morph-send checkbox is on.
//
// Switching TRIPLET on normally sets the native triplet flag and leaves
// the sixteen cells alone, so the device reinterprets the same steps at
// triplet spacing. With morph send on it instead replaces the pattern
// with the twelve-cell projection the morph planner produces at its
// endpoint: the same winner and loser rules the TRIPLET knob sweeps
// toward, committed in one step.
//
// The result is an ordinary twelve-step triplet pattern. Every path that
// follows - LIVE send, bank save, export, upload - carries it with no
// marker of where it came from.
//
// The toggle is reversible. Each projection remembers the exact source
// it replaced, so switching TRIPLET back off restores the original
// sixteen cells with their notes at their original indexes.
//
// The memory is keyed by pattern index, and the canonical text of the
// projection is what makes that safe. Deleting, reordering or editing a
// pattern leaves the index holding something the guard does not
// recognise, so the restore refuses and the press falls back to clearing
// the flag rather than writing a stale source over live content. The
// guard is the protection, not any bookkeeping on list changes: a
// pattern only ever receives back the source it actually gave up.
//
// The memory outlives the page. Leaving for the progression page and
// coming back is a full reload, and a projection that only lived in this
// module took the sixteen-step source with it, leaving the twelve-step
// projection as the only copy. It is stored in sessionStorage under a
// versioned key, beside the morph session that survives the same trip
// for the same reason. Storage is hostile input: every entry is
// validated on load and anything malformed is dropped, which costs an
// undo and never a live pattern, because the canonical guard still has
// to match before a restore writes anything.

import { api } from '../api.js';
import { canonicalPatternText } from '../shared/pattern-canonical.js';
import { clonePattern } from './pattern-default.js';
import { endpointPatternFromPlan } from '../shared/triplet-endpoint-pattern.js';
import { isMorphEligiblePattern } from '../shared/triplet-morph-timing.js';

const STORAGE_KEY = 'td3_triplet_morph_send_v1';
const STORAGE_VERSION = 1;
const PATTERN_STEP_COUNT = 16;

/** index -> { source, projectedText } for every live projection. */
const projections = new Map();

/** A stored source is only usable if it is a whole sixteen-step pattern. */
function isRestorableSource(source) {
    return !!source
        && typeof source === 'object'
        && Array.isArray(source.steps)
        && source.steps.length === PATTERN_STEP_COUNT
        && source.steps.every((step) => step && typeof step === 'object');
}

function persist(storage) {
    try {
        const store = storage ?? globalThis.sessionStorage;
        if (!store) return;
        if (projections.size === 0) {
            store.removeItem(STORAGE_KEY);
            return;
        }
        store.setItem(STORAGE_KEY, JSON.stringify({
            version: STORAGE_VERSION,
            entries: [...projections.entries()].map(([index, remembered]) => ({
                index,
                source: remembered.source,
                projectedText: remembered.projectedText,
            })),
        }));
    } catch (_) { /* quota or unavailable storage */ }
}

/**
 * Rehydrate the remembered sources written by an earlier page load.
 * Returns the number of entries adopted. Called once at module load and
 * exposed so tests can drive it against an injected storage.
 */
export function loadProjections(storage) {
    projections.clear();
    let raw = null;
    try {
        const store = storage ?? globalThis.sessionStorage;
        raw = store ? store.getItem(STORAGE_KEY) : null;
    } catch (_) {
        return 0;
    }
    if (!raw) return 0;
    let parsed = null;
    try {
        parsed = JSON.parse(raw);
    } catch (_) {
        return 0;
    }
    if (parsed?.version !== STORAGE_VERSION || !Array.isArray(parsed.entries)) return 0;
    for (const entry of parsed.entries) {
        const index = Number(entry?.index);
        if (!Number.isInteger(index) || index < 0) continue;
        if (typeof entry?.projectedText !== 'string' || !entry.projectedText) continue;
        if (!isRestorableSource(entry.source)) continue;
        projections.set(index, {
            source: clonePattern(entry.source),
            projectedText: entry.projectedText,
        });
    }
    return projections.size;
}

/** Forget every remembered source. */
export function forgetProjections(storage) {
    projections.clear();
    persist(storage);
}

loadProjections();

/** Indices that currently hold a projection this module can undo. */
export function restorableIndices(state, indices) {
    return indices.filter((index) => {
        const remembered = projections.get(index);
        if (!remembered) return false;
        const current = state.getPattern(index);
        return !!current && canonicalPatternText(current) === remembered.projectedText;
    });
}

/**
 * Replace every eligible index with its endpoint projection.
 *
 * Returns `{ morphed, skipped }` index lists. A pattern that is not
 * morph eligible, or whose plan is unavailable, is reported in `skipped`
 * for the caller to handle with the plain flag, so a planner failure can
 * never silently drop a TRIPLET press.
 */
export async function applyEndpointProjection(state, indices) {
    const morphed = [];
    const skipped = [];
    for (const index of indices) {
        const source = state.getPattern(index);
        if (!isMorphEligiblePattern(source)) {
            skipped.push(index);
            continue;
        }
        const sourceSnapshot = clonePattern(source);
        let projected = null;
        try {
            const response = await api.tripletMorphPlan(source);
            if (response?.eligible && response.plan) {
                projected = endpointPatternFromPlan(response.plan);
            }
        } catch (_) {
            projected = null;
        }
        // The source can change while the plan is in flight; only commit
        // onto the pattern that was actually planned.
        const current = state.getPattern(index);
        if (!projected || !current
            || canonicalPatternText(current) !== canonicalPatternText(sourceSnapshot)) {
            skipped.push(index);
            continue;
        }
        state.setPattern(index, projected);
        projections.set(index, {
            source: sourceSnapshot,
            projectedText: canonicalPatternText(projected),
        });
        morphed.push(index);
    }
    if (morphed.length) persist();
    return { morphed, skipped };
}

/**
 * Put back the source each projection replaced, notes at their original
 * indexes. Returns the indices actually restored; anything whose
 * projection no longer matches is dropped and left to the caller.
 */
export function restoreProjectedSources(state, indices) {
    const restored = [];
    for (const index of restorableIndices(state, indices)) {
        const remembered = projections.get(index);
        projections.delete(index);
        state.setPattern(index, clonePattern(remembered.source));
        restored.push(index);
    }
    for (const index of indices) {
        if (!restored.includes(index)) projections.delete(index);
    }
    persist();
    return restored;
}
