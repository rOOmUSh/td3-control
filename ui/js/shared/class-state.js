// State-class toggler. HTML partials own structural classes (sizing,
// layout, typography, rounded/tactile affordances); this helper only
// swaps the state-dependent subset (active/inactive, scratch/active/
// default, connected/disconnected). Using classList instead of
// `el.className = "<full string>"` keeps HTML authoritative for
// structure so JS can't trample, say, h-8 with h-10 on the next repaint.

/**
 * Apply one state from a states map. Classes listed under the active
 * key are added to the element; classes listed under every other key
 * are removed. Shared classes between states (e.g. `font-black` appears
 * in both scratch and active) are preserved because the removal of
 * non-active keys runs first, then the active key's classes are added.
 *
 * @param {HTMLElement} el
 * @param {Object<string, string[]>} statesMap - { stateName: [cls, ...] }
 * @param {string} activeKey - key into statesMap whose classes should be
 *                             present; all other keys' classes are removed.
 */
export function applyState(el, statesMap, activeKey) {
    for (const key in statesMap) {
        if (key !== activeKey) {
            for (const cls of statesMap[key]) el.classList.remove(cls);
        }
    }
    const active = statesMap[activeKey];
    if (active) {
        for (const cls of active) el.classList.add(cls);
    }
}
