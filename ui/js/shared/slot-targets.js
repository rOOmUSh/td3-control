// Pure A/B slot-assignment helpers. Used by:
//   - the main Control page to render per-card slot badges;
//   - PUSH TO TD-3 on the main page to compute device write targets;
//   - LOAD ALL on the main page to walk device slots in the right order.
//
// Two canonical orderings of 64 device slots exist:
//
//   ALTERNATE (A/B ALT):
//     G1P1A, G1P1B, G1P2A, G1P2B, …, G1P8A, G1P8B,
//     G2P1A, G2P1B, …, G4P8A, G4P8B
//
//   SERIAL (As→Bs SER):
//     G1P1A, G1P2A, …, G1P8A, G2P1A, …, G4P8A,
//     G1P1B, G1P2B, …, G4P8B
//
// Scratch exclusion:
//   Build the ordered list, drop the slot that matches the scratch address
//   (same { group, pattern, side } triple), yielding exactly 63 slots.
//   `slotFor(idx)` returns:
//     - the ordered, scratch-stripped slot at `idx` for idx < 63;
//     - null for idx === 63 (overflow - snapshot-only).
//
// Start-slot rotation (sidebar-selector anchor):
//   The sidebar group/pattern/side selector defines where the UI list
//   begins on the device. When a `startSlot` is supplied the canonical
//   list is rotated so `startSlot` sits at index 0 *before* scratch is
//   removed, so P1 lands on the selector and subsequent patterns walk
//   forward in the configured mode (wrapping past G4P8B back to G1P1A).
//   The A/B mode still controls ONLY the ordering (interleaved vs
//   serialized); it no longer implies a fixed G1P1A anchor.
//
// This module is pure (no DOM, no state imports). All callers pass the
// scratch descriptor explicitly so the same helper serves both slot badges
// and push flows. Label format matches bank convention `G{g}P{p}{side}` (no
// dash) so badges line up with bank slot keys.

export const SLOT_COUNT_TOTAL = 64;
export const SLOT_COUNT_AFTER_SCRATCH = 63;

/**
 * Build the canonical 64-entry slot list for the given A/B mode.
 *
 * @param {'ALTERNATE'|'SERIAL'} mode
 * @returns {Array<{group:number, pattern:number, side:'A'|'B', label:string}>}
 */
export function orderedSlots(mode) {
    if (mode !== 'ALTERNATE' && mode !== 'SERIAL') {
        throw new Error(`orderedSlots: unknown mode "${mode}"`);
    }
    const out = [];
    if (mode === 'ALTERNATE') {
        // 4 groups × 8 patterns × (A, B) interleaved - 16 slots per group.
        for (let g = 1; g <= 4; g++) {
            for (let p = 1; p <= 8; p++) {
                out.push(makeSlot(g, p, 'A'));
                out.push(makeSlot(g, p, 'B'));
            }
        }
    } else {
        // SERIAL - all 32 A-side slots first, then all 32 B-side slots.
        for (const side of ['A', 'B']) {
            for (let g = 1; g <= 4; g++) {
                for (let p = 1; p <= 8; p++) {
                    out.push(makeSlot(g, p, side));
                }
            }
        }
    }
    return out;
}

/**
 * Rotate a canonical 64-slot list so the slot matching `startSlot` sits at
 * index 0. When `startSlot` is null/undefined or doesn't match any entry
 * the list is returned unchanged - preserves the historical G1P1A anchor
 * for callers that haven't opted into the selector-anchored walk yet.
 *
 * @param {Array<{group,pattern,side}>} list
 * @param {{group,pattern,side}|null|undefined} startSlot
 * @returns {Array<{group,pattern,side,label}>}
 */
export function rotateToStart(list, startSlot) {
    if (!Array.isArray(list) || list.length === 0) return list;
    if (!startSlot) return list;
    const pivot = list.findIndex((s) => sameAddress(s, startSlot));
    if (pivot <= 0) return list;
    return list.slice(pivot).concat(list.slice(0, pivot));
}

/**
 * Ordered slots rotated so `startSlot` is first, with the scratch address
 * removed. Length is 63 when scratch matches a slot (and scratch ≠ startSlot
 * - if they collide, scratch still gets filtered so P1 lands on the first
 * non-scratch slot at or after the selector). Otherwise length stays at 64.
 *
 * @param {'ALTERNATE'|'SERIAL'} mode
 * @param {{group:number, pattern:number, side:string}|null|undefined} scratch
 * @param {{group:number, pattern:number, side:string}|null|undefined} [startSlot]
 */
export function orderedSlotsExcludingScratch(mode, scratch, startSlot) {
    const rotated = rotateToStart(orderedSlots(mode), startSlot);
    if (!scratch) return rotated;
    return rotated.filter((s) => !sameAddress(s, scratch));
}

/**
 * Map a UI pattern index to its assigned device slot (after start-slot
 * rotation and scratch removal).
 *
 * @param {number} idx           0..63
 * @param {{group,pattern,side}|null|undefined} scratch
 * @param {'ALTERNATE'|'SERIAL'} mode
 * @param {{group,pattern,side}|null|undefined} [startSlot]  sidebar selector;
 *   when provided, P1 (idx 0) lands on this slot and subsequent patterns
 *   walk forward in `mode`. Defaults to the canonical G1P1A anchor.
 * @returns {{group,pattern,side,label}|null}
 *     null when `idx === 63` (overflow; scratch present) - the UI calls this
 *     "the snapshot-only pattern" and surfaces it as the no-device badge.
 */
export function slotFor(idx, scratch, mode, startSlot) {
    if (!Number.isInteger(idx) || idx < 0) return null;
    const list = orderedSlotsExcludingScratch(mode, scratch, startSlot);
    if (idx >= list.length) return null;
    return list[idx];
}

/**
 * Parse a slot label like `G1P1A` into its triple. Returns null on bad input.
 */
export function parseSlotLabel(label) {
    if (typeof label !== 'string') return null;
    const m = /^G([1-4])P([1-8])([AB])$/.exec(label);
    if (!m) return null;
    return {
        group: Number(m[1]),
        pattern: Number(m[2]),
        side: m[3],
        label,
    };
}

/** True when slot's (group, pattern, side) equals scratch's. */
function sameAddress(a, b) {
    return a.group === b.group && a.pattern === b.pattern && a.side === b.side;
}

function makeSlot(group, pattern, side) {
    return { group, pattern, side, label: `G${group}P${pattern}${side}` };
}
