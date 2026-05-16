// Pure per-step semitone transposition across the full TD-3 audible range.
//
// Each TD-3 step has a 13-slot note name (C..B plus a high "C^") and a
// transpose *zone* flag: `'DN'` (-12), `'NORMAL'` (0), or `'UP'` (+12). The
// previous version of this helper wrapped within a single zone, which
// capped the transpose feature at 12 semitones. The current rule instead
// lets ±1-semitone shifts walk across zones, so a pattern can traverse
// the whole audible range the hardware supports:
//
//   DN  zone  C..B..C^  (12 reachable + 1 alias at top)
//   NORM zone C..B..C^
//   UP  zone  C..B..C^
//
// The three zones are stitched together at the slot boundaries:
//
//   • `+1` from slot B (idx 11) crosses to the next zone's slot C (idx 0)
//     - because `(B, zone)` and `(C, zone+1)` are adjacent MIDI notes.
//   • `+1` from slot C^ (idx 12) crosses to the next zone's slot C# (idx 1)
//     - C^ is the "aliased top" of the zone and MIDI-equals `(C, zone+1)`,
//     so `+1` must land on `C#` of that next zone.
//   • `-1` from slot C (idx 0) crosses to the previous zone's slot B (idx 11)
//     - because `(C, zone)` and `(B, zone-1)` are adjacent MIDI notes
//     (the zone-1 C^ alias is skipped so we don't sit on an alias).
//
// At the hardware edges (DN's C at the floor, UP's C^ at the ceiling) a
// step cannot leave the zone - there is no zone beyond. In that case we
// fall back to a wrap *within* the current zone (C→B at the floor,
// C^→C# at the ceiling).
//
//   (C, DN)  -1 -> (B, DN)     floor wrap
//   (C^, UP) +1 -> (C#, UP)    ceiling wrap
//
// The step's `accent`, `slide`, and `time` fields are copied unchanged.

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const ZONES = ['DOWN', 'NORMAL', 'UP'];

// Canonicalise the zone flag. Runtime state stores `'DOWN'` (see
// state.js#toggleTranspose and progression-state.js#toggleTranspose); the
// display layer renders that as the two-letter label `'DN'`. We accept
// both forms on input so legacy fixtures (or the buggy earlier tests)
// don't blow up, but *always emit* the canonical `'DOWN'` so the flag
// lights up the manual DN button on the step card.
function normalizeZone(z) {
    if (z === 'UP') return 'UP';
    if (z === 'NORMAL' || z === undefined || z === null) return 'NORMAL';
    if (z === 'DOWN' || z === 'DN') return 'DOWN';
    throw new Error(`transposeStepNote: unknown transpose flag ${z}`);
}

/**
 * Return a new step object shifted by `delta` semitones across the full
 * DN/NORMAL/UP range. `delta` must be +1 or -1.
 *
 * Interior moves stay inside the current zone. Crossing between zones
 * happens at the B↔C boundary (and at the C^ alias). When a step is at
 * the DN floor (`(C, DN)`) or the UP ceiling (`(C^, UP)`) the shift
 * wraps within the current zone (`(B, DN)` or `(C#, UP)` respectively)
 * because there is no zone beyond.
 *
 * All other step fields (accent, slide, time) are copied unchanged.
 */
export function transposeStepNote(step, delta) {
    if (delta !== 1 && delta !== -1) {
        throw new Error(`transposeStepNote: delta must be +1 or -1, got ${delta}`);
    }
    const slotIdx = NOTE_NAMES.indexOf(step.note);
    if (slotIdx < 0) {
        throw new Error(`transposeStepNote: unknown note name ${step.note}`);
    }
    const zone = normalizeZone(step.transpose);
    const zoneIdx = ZONES.indexOf(zone);

    let newSlotIdx;
    let newZoneIdx;

    if (delta === 1) {
        if (slotIdx === 12) {
            // C^: land on C# of the next zone (because C^ aliases that zone's C).
            newSlotIdx = 1;
            newZoneIdx = zoneIdx < 2 ? zoneIdx + 1 : zoneIdx; // ceiling wrap
        } else if (slotIdx === 11) {
            // B: at the UP ceiling go to C^ (the top); elsewhere cross to next
            // zone's C. Crossing lets the user reach MIDI notes beyond the
            // current zone even though (C^, zone) and (C, zone+1) are aliases.
            if (zoneIdx < 2) {
                newSlotIdx = 0;
                newZoneIdx = zoneIdx + 1;
            } else {
                newSlotIdx = 12;
                newZoneIdx = zoneIdx;
            }
        } else {
            newSlotIdx = slotIdx + 1;
            newZoneIdx = zoneIdx;
        }
    } else {
        // delta === -1
        if (slotIdx === 0) {
            // C: cross to previous zone's B (the zone-1 C^ alias is skipped).
            // At the DN floor there is no prev zone → wrap within DN to B.
            newSlotIdx = 11;
            newZoneIdx = zoneIdx > 0 ? zoneIdx - 1 : zoneIdx;
        } else {
            newSlotIdx = slotIdx - 1;
            newZoneIdx = zoneIdx;
        }
    }

    return {
        ...step,
        note: NOTE_NAMES[newSlotIdx],
        transpose: ZONES[newZoneIdx],
    };
}

// Keep in lockstep with ui/js/progression/progression-row.js ARCHETYPE_CHIPS
// and the bassline-set storage shape described in
// ui/js/progression/progression-state.js.
export const BASSLINE_ARCHETYPE_KEYS = Object.freeze([
    'pedal',
    'rootPulse',
    'offbeat',
    'shadow',
    'arpeggio',
]);

/**
 * Mutate `steps` in place, replacing each entry with a copy shifted by
 * `delta` semitones. Iterates the strict ±1 single-step helper |delta|
 * times so the cross-zone / edge-wrap rules stay identical to the
 * single-step path. No-op on an empty/nullish array, non-integer delta,
 * or delta === 0. Exported so both pattern and bassline code paths share
 * the same iterator.
 */
export function transposeStepsInPlace(steps, delta) {
    if (!Array.isArray(steps)) return;
    if (!Number.isInteger(delta) || delta === 0) return;
    const direction = delta > 0 ? 1 : -1;
    const count = Math.abs(delta);
    for (let n = 0; n < count; n++) {
        for (let i = 0; i < steps.length; i++) {
            steps[i] = transposeStepNote(steps[i], direction);
        }
    }
}

/**
 * Transpose every archetype pattern inside a bassline set (the value stored
 * at `basslines[i]` in progression state). `null` / missing keys / missing
 * step arrays are all tolerated - silently skipped. When an acid pattern is
 * shifted, its basslines have to follow or they'd play in the wrong key.
 */
export function transposeBasslineSetInPlace(set, delta) {
    if (!set) return;
    for (const key of BASSLINE_ARCHETYPE_KEYS) {
        const pattern = set[key];
        if (pattern && Array.isArray(pattern.steps)) {
            transposeStepsInPlace(pattern.steps, delta);
        }
    }
}
