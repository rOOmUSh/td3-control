// Absolute-pitch ↔ TD-3 (note, transpose) encoding.
//
// TD-3 step pitch is split across two fields:
//   - note     : one of 13 keyboard labels  C, C#, D, ..., B, C^   → index 0..12
//   - transpose: 'DOWN' | 'NORMAL' | 'UP'                          → octave shift
//
// In absolute-semitone space, the keyboard's NORMAL row covers 0..12 (with
// C=0 as the bottom and C^=12 as the top of that octave). DOWN shifts that
// row down by 12 semitones (range −12..0) and UP shifts it up by 12 (range
// 12..24). Total addressable range is therefore [−12, 24] - about three
// octaves.
//
// The randomizer chooses an absolute pitch first, then encodes it back to
// (note, transpose). Do not pick
// a note and then random-roll the transpose, because that decouples the
// melody from its register.
//
// Some pitches have two equivalent encodings:
//   12 → (C^, NORMAL)  or  (C,  UP)
//    0 → (C,  NORMAL)  or  (C^, DOWN)
// We always pick the NORMAL row when both encodings are valid so step
// cards read consistently. Out-of-range pitches return null.

export const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];

export const TD3_PITCH_MIN = -12;
export const TD3_PITCH_MAX = 24;

/** True if `p` is inside the TD-3 addressable range. */
export function isPitchInRange(p) {
    return Number.isInteger(p) && p >= TD3_PITCH_MIN && p <= TD3_PITCH_MAX;
}

/**
 * Encode an absolute semitone pitch into TD-3 (note, transpose).
 * Returns { note, transpose, noteIdx } on success, or null if out of range.
 *
 * Encoding preference, in order:
 *   1. NORMAL row (0..12)   - always preferred
 *   2. DOWN   row (−12..−1)
 *   3. UP     row (13..24)
 */
export function encodePitch(p) {
    if (!isPitchInRange(p)) return null;
    if (p >= 0 && p <= 12) {
        return { note: NOTE_NAMES[p], transpose: 'NORMAL', noteIdx: p };
    }
    if (p < 0) {
        const noteIdx = p + 12;          // -12..-1 → 0..11
        return { note: NOTE_NAMES[noteIdx], transpose: 'DOWN', noteIdx };
    }
    // p > 12
    const noteIdx = p - 12;              // 13..24 → 1..12
    return { note: NOTE_NAMES[noteIdx], transpose: 'UP', noteIdx };
}

/**
 * Decode a (note, transpose) pair into absolute semitone pitch. Returns
 * `null` if the inputs are unrecognised.
 *
 * Accepts either a step object {note, transpose} or two positional args.
 */
export function decodePitch(noteOrStep, transpose) {
    let note;
    if (noteOrStep && typeof noteOrStep === 'object') {
        note = noteOrStep.note;
        transpose = noteOrStep.transpose;
    } else {
        note = noteOrStep;
    }
    const noteIdx = NOTE_NAMES.indexOf(note);
    if (noteIdx < 0) return null;
    const shift = transpose === 'UP' ? 12 : transpose === 'DOWN' ? -12 : transpose === 'NORMAL' ? 0 : null;
    if (shift === null) return null;
    const p = noteIdx + shift;
    return isPitchInRange(p) ? p : null;
}

/**
 * Build the full set of in-scale absolute pitches that fit on the TD-3.
 * Input is a scale.intervals array (semitones from root, 0..11) and a root
 * pitch class (0..11). Output is a sorted ascending array of absolute
 * pitches in [−12, 24] whose pitch class belongs to the scale.
 *
 * The result spans three octaves so the generator can pick a register
 * intentionally without falling back to random transpose post-processing.
 */
export function buildScalePitches(root, scale) {
    if (!scale || !Array.isArray(scale.intervals)) return [];
    const allowed = new Set();
    for (const iv of scale.intervals) {
        allowed.add(((root + iv) % 12 + 12) % 12);
    }
    const out = [];
    for (let p = TD3_PITCH_MIN; p <= TD3_PITCH_MAX; p++) {
        const pc = ((p % 12) + 12) % 12;
        if (allowed.has(pc)) out.push(p);
    }
    return out;
}

/**
 * Find the absolute pitch in `pitches` whose value is closest to `target`.
 * Ties go to the lower pitch (smaller absolute value when equidistant) so
 * downward motion is slightly preferred - matches the existing randomizer's
 * implicit bias.
 */
export function nearestPitch(target, pitches) {
    if (!pitches || pitches.length === 0) return null;
    let best = pitches[0];
    let bestDist = Math.abs(best - target);
    for (let i = 1; i < pitches.length; i++) {
        const d = Math.abs(pitches[i] - target);
        if (d < bestDist) { bestDist = d; best = pitches[i]; }
    }
    return best;
}
