// Transient triplet morph audition session, shared by the Control and
// Progression pages.
//
// The session owns the morph amount, the exact canonical source
// snapshots that make it valid, and the cached Rust plans. It is
// derived state: it never enters a page's own state blob, pattern
// history, import/export, packages, snapshots, or bank storage. It is
// persisted only as its own versioned payload so a reload can restore
// the amount when every canonical source still matches byte for byte.
//
// A page's state module owns notification; this module only reports
// whether something changed.

import { normalizeTripletMorphPercent } from './triplet-morph-control.js';
import { canonicalPatternListText } from './pattern-canonical.js';
import { isMorphEligiblePattern } from './triplet-morph-timing.js';

export const TRIPLET_MORPH_SESSION_VERSION = 1;

/**
 * @param {object} options
 * @param {string} options.storageKey  versioned sessionStorage key
 * @param {() => object[]} options.getPatterns  every pattern that can
 *   become audible on the page; all must be eligible for a positive
 *   amount to be accepted
 */
export function createTripletMorphSession({ storageKey, getPatterns }) {
    let percent = 0;
    let session = null;

    function storage() {
        return globalThis.sessionStorage;
    }

    function currentSources() {
        return canonicalPatternListText(getPatterns() || []);
    }

    function sourcesChanged() {
        if (!session) return true;
        const current = currentSources();
        const stored = session.canonicalSources;
        if (!Array.isArray(stored) || stored.length !== current.length) return true;
        for (let i = 0; i < current.length; i += 1) {
            if (stored[i] !== current[i]) return true;
        }
        return false;
    }

    function clear() {
        percent = 0;
        session = null;
        try { storage()?.removeItem(storageKey); }
        catch (_) { /* unavailable */ }
    }

    function persist() {
        if (!session) return;
        try { storage()?.setItem(storageKey, JSON.stringify(session)); }
        catch (_) { /* quota */ }
    }

    function begin(sourceSnapshots, plans) {
        session = {
            version: TRIPLET_MORPH_SESSION_VERSION,
            amount: percent,
            canonicalSources: Array.isArray(sourceSnapshots) ? sourceSnapshots : [],
            plans: plans && typeof plans === 'object' ? plans : {},
        };
        persist();
    }

    /**
     * The knob is enabled only when every pattern that can become
     * audible is straight with 4, 8, 12, or 16 active steps.
     */
    function isSourceEligible() {
        const patterns = getPatterns() || [];
        if (patterns.length === 0) return false;
        return patterns.every(isMorphEligiblePattern);
    }

    /**
     * Set the amount. A positive amount is refused while any source is
     * ineligible. Returns true when the stored amount changed, so the
     * caller knows whether to notify observers.
     */
    function setPercent(value) {
        const next = normalizeTripletMorphPercent(value);
        if (next === percent) return false;
        if (next > 0 && !isSourceEligible()) return false;
        percent = next;
        if (next === 0) {
            clear();
        } else if (session) {
            session.amount = next;
            persist();
        } else {
            begin(currentSources(), {});
        }
        return true;
    }

    /**
     * Adopt the current sources as the session's new identity, keeping
     * the amount. Used when the user edits at the 100 endpoint, where
     * editing surviving notes is deliberate: the session stays open and
     * simply re-baselines. Cached plans are keyed by canonical text, so
     * untouched patterns keep theirs and an edited one re-fetches.
     * Falls back to clearing if the edit made a source ineligible.
     */
    function rebaseline() {
        if (!session) return false;
        if (!isSourceEligible()) {
            clear();
            return false;
        }
        session.canonicalSources = currentSources();
        persist();
        return true;
    }

    /**
     * Reload restore: a stored session is honored only when its shape
     * and amount validate and every canonical source text matches the
     * current patterns exactly. Any mismatch deletes the payload.
     */
    function restore() {
        let payload = null;
        try {
            const raw = storage()?.getItem(storageKey);
            if (!raw) return;
            payload = JSON.parse(raw);
        } catch (_) {
            payload = null;
        }
        const amount = payload?.amount;
        const valid = payload
            && payload.version === TRIPLET_MORPH_SESSION_VERSION
            && Number.isInteger(amount) && amount >= 1 && amount <= 100
            && Array.isArray(payload.canonicalSources)
            && payload.plans && typeof payload.plans === 'object';
        if (valid) {
            const current = currentSources();
            const stored = payload.canonicalSources;
            const matches = stored.length === current.length
                && stored.every((text, i) => text === current[i]);
            if (matches && isSourceEligible()) {
                percent = amount;
                session = payload;
                return;
            }
        }
        try { storage()?.removeItem(storageKey); }
        catch (_) { /* unavailable */ }
    }

    return {
        getPercent: () => percent,
        isActive: () => percent > 0,
        getSession: () => session,
        isSourceEligible,
        setPercent,
        sourcesChanged,
        clear,
        begin,
        rebaseline,
        restore,
        getPlan(canonicalText) {
            return session?.plans?.[canonicalText] ?? null;
        },
        setPlan(canonicalText, plan) {
            if (!session || typeof canonicalText !== 'string' || !plan) return;
            session.plans[canonicalText] = plan;
            persist();
        },
    };
}
