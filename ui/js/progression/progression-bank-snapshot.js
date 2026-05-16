// Build snapshot payloads for the Bank API from the progression page.
//
// Two entry points:
//
//   - buildSinglePatternSlots(idx, pattern, basslineSet)
//     Used by the per-row BANK button. Lays one acid pattern into G1-P1A and
//     its 5 archetype basslines into G1-P1B..G1-P5B, naming the basslines
//     "G1P1A {ARCH} BSL" so a later bank-side filter can pair them with the
//     acid lead. The acid slot keeps its slot_key as the display_name (no
//     custom label needed - the slot_key carries enough context).
//
//   - buildProgressionSlots(patterns, basslinesByPattern)
//     Used by the SAVE PACKAGE → BANK option. Lays the 4 acid patterns into
//     G1-P1A..G1-P4A and the 4×5=20 archetype basslines into the B-sides of
//     G1..G3 in position-major × archetype-minor order:
//
//        G1-P1B = pos0.pedal     ("G1P1A PEDAL BSL")
//        G1-P2B = pos0.rootPulse ("G1P1A PULSE BSL")
//        ...
//        G1-P5B = pos0.arpeggio  ("G1P1A ARP BSL")
//        G1-P6B = pos1.pedal     ("G1P2A PEDAL BSL")
//        ...
//        G3-P4B = pos3.arpeggio  ("G1P4A ARP BSL")
//
// Both helpers are pure: they take frontend pattern objects and return the
// `slots` array that `bankApi.createSnapshotFromPatterns` expects. The
// snapshot name is built separately via `formatSnapshotName(date)` so callers
// can stamp a deterministic timestamp at the moment of upload.

// Frontend bassline archetype keys are (pedal, rootPulse, offbeat, shadow,
// arpeggio). The visible labels in the snapshot grid use the shorter forms
// the user already sees on the per-row PREVIEW chips (PULSE for rootPulse,
// ARP for arpeggio), so the snapshot names line up with the UI.
export const ARCHETYPE_KEYS_ORDERED = Object.freeze([
    'pedal', 'rootPulse', 'offbeat', 'shadow', 'arpeggio',
]);

export const ARCHETYPE_LABELS = Object.freeze({
    pedal:     'PEDAL',
    rootPulse: 'PULSE',
    offbeat:   'OFFBEAT',
    shadow:    'SHADOW',
    arpeggio:  'ARP',
});

/**
 * Format a Date as the deterministic `SN_YYYY-MM-DD_HH-MM-SS` snapshot name
 * the user specified - local time, no timezone suffix. Pure.
 */
export function formatSnapshotName(date) {
    const d = date || new Date();
    const pad = (n) => String(n).padStart(2, '0');
    return (
        'SN_' +
        String(d.getFullYear()) +
        '-' +
        pad(d.getMonth() + 1) +
        '-' +
        pad(d.getDate()) +
        '_' +
        pad(d.getHours()) +
        '-' +
        pad(d.getMinutes()) +
        '-' +
        pad(d.getSeconds())
    );
}

/**
 * Build the 6-slot snapshot payload for a single progression row.
 *
 *   slot 0 = G1-P1A   (acid pattern, display_name = "G1P1A")
 *   slot 1 = G1-P1B   ("G1P1A PEDAL BSL")
 *   slot 2 = G1-P2B   ("G1P1A PULSE BSL")
 *   slot 3 = G1-P3B   ("G1P1A OFFBEAT BSL")
 *   slot 4 = G1-P4B   ("G1P1A SHADOW BSL")
 *   slot 5 = G1-P5B   ("G1P1A ARP BSL")
 *
 * `idx` is informational - the lead always lands in G1-P1A regardless of
 * which row was pushed (single-pattern bank push is independent of the
 * progression position).
 *
 * Returns `{ slots, error }`. `error` is non-null on shape failure (missing
 * pattern, missing archetype) so callers can refuse the upload before it
 * leaves the page rather than ship a half-built snapshot.
 */
export function buildSinglePatternSlots(_idx, pattern, basslineSet) {
    if (!pattern || !Array.isArray(pattern.steps)) {
        return { slots: null, error: 'missing-pattern' };
    }
    if (!basslineSet || typeof basslineSet !== 'object') {
        return { slots: null, error: 'missing-basslines' };
    }
    for (const k of ARCHETYPE_KEYS_ORDERED) {
        if (!basslineSet[k] || !Array.isArray(basslineSet[k].steps)) {
            return { slots: null, error: `missing-archetype:${k}` };
        }
    }
    const acidLabel = 'G1P1A';
    const slots = [
        { slot_key: 'G1-P1A', pattern: pattern, display_name: acidLabel },
    ];
    for (let i = 0; i < ARCHETYPE_KEYS_ORDERED.length; i++) {
        const key = ARCHETYPE_KEYS_ORDERED[i];
        const slotKey = `G1-P${i + 1}B`;
        const name = `${acidLabel} ${ARCHETYPE_LABELS[key]} BSL`;
        slots.push({ slot_key: slotKey, pattern: basslineSet[key], display_name: name });
    }
    return { slots, error: null };
}

/**
 * Map a 1..20 linear bassline index (1 = G1-P1B, 8 = G1-P8B, 9 = G2-P1B,
 * ..., 20 = G3-P4B) to the dashed slot key. Returns null when the index is
 * out of range. Pure helper exported for tests.
 */
export function basslineSlotForIndex(linearIdx) {
    if (!Number.isInteger(linearIdx) || linearIdx < 1 || linearIdx > 20) return null;
    const zero = linearIdx - 1;
    const group = Math.floor(zero / 8) + 1;
    const pat = (zero % 8) + 1;
    return `G${group}-P${pat}B`;
}

/**
 * Build the 24-slot snapshot payload for the full progression.
 *
 *   G1-P1A..G1-P4A = patterns[0..3]              (display_name = "G1P{n}A")
 *   G1-P1B..G3-P4B = 5 archetypes × 4 positions  (display_name "G1P{n}A {ARCH} BSL")
 *
 * Layout matches the 20-entry flatten the package builder already uses
 * (position-major × archetype-minor). Returns `{ slots, error }` shaped like
 * `buildSinglePatternSlots`.
 */
export function buildProgressionSlots(patterns, basslinesByPattern) {
    if (!Array.isArray(patterns) || patterns.length !== 4) {
        return { slots: null, error: 'bad-patterns-shape' };
    }
    if (!Array.isArray(basslinesByPattern) || basslinesByPattern.length !== 4) {
        return { slots: null, error: 'bad-basslines-shape' };
    }
    for (let i = 0; i < 4; i++) {
        if (!patterns[i] || !Array.isArray(patterns[i].steps)) {
            return { slots: null, error: `missing-pattern:${i}` };
        }
        const set = basslinesByPattern[i];
        if (!set) return { slots: null, error: `missing-basslines:${i}` };
        for (const k of ARCHETYPE_KEYS_ORDERED) {
            if (!set[k] || !Array.isArray(set[k].steps)) {
                return { slots: null, error: `missing-archetype:${i}:${k}` };
            }
        }
    }
    const slots = [];
    for (let i = 0; i < 4; i++) {
        const slotKey = `G1-P${i + 1}A`;
        const acidLabel = `G1P${i + 1}A`;
        slots.push({ slot_key: slotKey, pattern: patterns[i], display_name: acidLabel });
    }
    let linear = 1;
    for (let i = 0; i < 4; i++) {
        const set = basslinesByPattern[i];
        const acidLabel = `G1P${i + 1}A`;
        for (const key of ARCHETYPE_KEYS_ORDERED) {
            const slotKey = basslineSlotForIndex(linear);
            if (!slotKey) return { slots: null, error: `linear-overflow:${linear}` };
            const name = `${acidLabel} ${ARCHETYPE_LABELS[key]} BSL`;
            slots.push({ slot_key: slotKey, pattern: set[key], display_name: name });
            linear++;
        }
    }
    return { slots, error: null };
}
