// Key detection for imported patterns.
//
// Given a 16-step pattern, returns {root, scaleId, confidence} by inspecting
// the active steps' pitch content. This is Tier 1: detects the *tonal center*
// and *major vs minor quality*, and maps to the nearest project scale
// ('major' or 'natural_minor'). Exotic/chromatic scales are not in scope
// here - Tier 2 would correlate against per-scale profiles for wider coverage.
//
// Advisory, not authoritative: callers should auto-populate the UI selects
// with the result but let the user override before acting on them. A low
// confidence score means "best guess is weak"; the UI can surface that so
// the user knows to verify.
//
// Three algorithms live in this module; only one is wired into `detectKey()`
// at a time. The others stay in the source as documented alternatives that
// a future reader can swap in with a one-line change.
//
//   Krumhansl-Schmuckler (1990, dormant) - derived from probe-tone listening
//     experiments on Western classical cadences. Strong for functional
//     tonal music; tends to mis-identify chromatic/modal basslines because
//     chromatic accidents align with high-profile slots of unrelated keys
//     (e.g. an acid line in C# minor gets pulled toward F minor by strong
//     C/F weight).
//
//   Temperley CBMS (2001, active) - derived from corpus statistics of
//     actual Western music. More tolerant of chromatic passing tones than
//     KS. Correlation-based method; passes a 24-key canonical generator
//     sweep cleanly and gets the tonal center right on real acid patterns
//     even when mode is ambiguous. Currently wired because it outperforms
//     the geometric alternatives on our pc-only inputs.
//
//   Chew Spiral Array (2000, dormant) - geometric model rather than a
//     statistical profile. Pitches placed on a helix where fifths are
//     adjacent; chord/key centroids are weighted means of tonic/dominant/
//     subdominant positions. Conceptually clean but struggles with our
//     pattern data: Chew's classical 4-fifths-per-turn helix is designed
//     for octave-aware input, and our pc-only patterns force every pc to
//     a canonical position in a single octave. Both a 2D 12-fifths-per-turn
//     variant (each pc gets a unique angle) and a 3D 4-fifths + modular-z
//     variant fail ~9/24 canonical generator patterns - they're geometric
//     artifacts of collapsing octaves, not tuning issues. Kept in source
//     as a documented exploration.

// --- Krumhansl-Schmuckler profiles (dormant) --------------------------------
// eslint-disable-next-line no-unused-vars
const MAJOR_PROFILE_KS = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
// eslint-disable-next-line no-unused-vars
const MINOR_PROFILE_KS = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

// --- Temperley CBMS profiles (active) ---------------------------------------
const MAJOR_PROFILE_TEMPERLEY = [0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400];
const MINOR_PROFILE_TEMPERLEY = [0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330];

// --- Chew Spiral Array, 2D circle-of-fifths variant (dormant) ---------------
// The classical Chew helix uses 4 fifths per full turn, relying on the
// z-axis to distinguish pitches a major third apart (which would otherwise
// share x,y coordinates). That geometry assumes octave-aware pitch input.
// Our pattern data is pitch-class-only, so every pc has to collapse to a
// single canonical position - under the 4-fifths-per-turn helix that means
// keys related by a major third become indistinguishable in x,y and
// unreliably distinguished by z alone.
//
// The variant here uses 12 fifths per full turn: each pitch class gets a
// unique angle on the circle of fifths. Unfortunately the major-3rd now
// sits 120° from the root, which spreads chord/key centroids far enough
// that relative keys swap on ~9/24 canonical generator patterns. Kept in
// the source as a documented exploration; not wired into detectKey().
// eslint-disable-next-line no-unused-vars
const SA_R = 1.0;
// eslint-disable-next-line no-unused-vars
const SA_CHORD_W = [0.536, 0.274, 0.190];
// eslint-disable-next-line no-unused-vars
const SA_KEY_W = [0.536, 0.274, 0.190];
// eslint-disable-next-line no-unused-vars
const SA_MINOR_MIX = 0.5;

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];

// Minimum number of active notes below which detection is refused. A pattern
// with 1-2 notes doesn't give the algorithm anything to latch onto.
const MIN_ACTIVE_NOTES = 3;

// Confidence threshold above which callers should treat the result as
// trustworthy. For Temperley this is the margin between the best correlation
// and the runner-up - a small margin means the pattern sits between two
// keys and the pick is fragile.
export const CONFIDENCE_HIGH = 0.05;

/**
 * Build a pitch-class histogram from a pattern's active steps.
 * Non-REST steps contribute 1 to their note's pitch class. Accented steps
 * get a small bonus since they emphasize the tonal role of that pc.
 *
 * Exported so scale-ranking can score every scale against the same histogram
 * that drives detection - keeps the two features locked to identical input.
 */
export function buildPitchClassHistogram(pattern) {
    const hist = new Array(12).fill(0);
    if (!pattern || !Array.isArray(pattern.steps)) return hist;
    for (const step of pattern.steps) {
        if (!step || step.time === 'REST' || step.time === 'TIE_REST') continue;
        const idx = NOTE_NAMES.indexOf(step.note);
        if (idx < 0) continue;
        const pc = idx % 12;
        const weight = step.accent ? 1.5 : 1.0;
        hist[pc] += weight;
    }
    return hist;
}

function countActiveNotes(pattern) {
    return pattern?.steps?.filter(s =>
        s && s.time !== 'REST' && s.time !== 'TIE_REST'
    ).length || 0;
}

// --- Spiral Array helpers (dormant) -----------------------------------------
// Kept alongside the active Temperley detector as a documented alternative.
// See the top-of-file comment for why this path isn't wired into detectKey.

/** Pitch-class → circle-of-fifths index (0=C, 1=G, 2=D, …, 7=C#, …, 11=F). */
// eslint-disable-next-line no-unused-vars
function pcToCof(pc) { return (pc * 7) % 12; }

// eslint-disable-next-line no-unused-vars
function saPos(k) {
    const kw = ((k % 12) + 12) % 12;
    return [
        SA_R * Math.sin(kw * Math.PI / 6),
        SA_R * Math.cos(kw * Math.PI / 6),
        0,
    ];
}

// --- Temperley detection (active) -------------------------------------------

/**
 * Pearson correlation between two equal-length arrays. Returns 0 when either
 * array has no variance (degenerate cases like an all-zero histogram, which
 * callers have already filtered out via MIN_ACTIVE_NOTES).
 */
function pearson(a, b) {
    const n = a.length;
    let ma = 0, mb = 0;
    for (let i = 0; i < n; i++) { ma += a[i]; mb += b[i]; }
    ma /= n; mb /= n;
    let num = 0, da = 0, db = 0;
    for (let i = 0; i < n; i++) {
        const xa = a[i] - ma, xb = b[i] - mb;
        num += xa * xb;
        da += xa * xa;
        db += xb * xb;
    }
    const denom = Math.sqrt(da * db);
    return denom === 0 ? 0 : num / denom;
}

/** Rotate a 12-length profile so index 0 lands at pitch class `root`. */
function rotateProfile(profile, root) {
    const out = new Array(12);
    for (let i = 0; i < 12; i++) out[i] = profile[(i - root + 12) % 12];
    return out;
}

/**
 * Detect the most likely key of a pattern by correlating its pitch-class
 * histogram against rotated major/minor Temperley profiles.
 *
 * @param {object} pattern - { steps: [{note, time, accent, ...}, ...] }
 * @returns {{root: number, scaleId: string, mode: 'major'|'minor',
 *            confidence: number, noteCount: number} | null}
 *   null when the pattern has fewer than MIN_ACTIVE_NOTES active notes.
 *   confidence is the margin between the best and runner-up correlation -
 *   callers can compare against CONFIDENCE_HIGH to decide whether to prompt
 *   the user or silently apply the detection.
 */
export function detectKey(pattern) {
    const hist = buildPitchClassHistogram(pattern);
    const noteCount = countActiveNotes(pattern);
    if (noteCount < MIN_ACTIVE_NOTES) return null;

    let best = { score: -Infinity, root: 0, mode: 'major' };
    let second = -Infinity;
    for (let pc = 0; pc < 12; pc++) {
        const sMaj = pearson(hist, rotateProfile(MAJOR_PROFILE_TEMPERLEY, pc));
        const sMin = pearson(hist, rotateProfile(MINOR_PROFILE_TEMPERLEY, pc));
        if (sMaj > best.score) {
            second = best.score;
            best = { score: sMaj, root: pc, mode: 'major' };
        } else if (sMaj > second) {
            second = sMaj;
        }
        if (sMin > best.score) {
            second = best.score;
            best = { score: sMin, root: pc, mode: 'minor' };
        } else if (sMin > second) {
            second = sMin;
        }
    }

    const scaleId = best.mode === 'major' ? 'major' : 'natural_minor';
    const confidence = second === -Infinity ? 0 : (best.score - second);
    return {
        root: best.root,
        scaleId,
        mode: best.mode,
        confidence,
        noteCount,
    };
}

/** Human-readable label for a detection result (e.g. "A minor"). */
export function formatKey(detection) {
    if (!detection) return '';
    const root = NOTE_NAMES[detection.root] || '?';
    return `${root} ${detection.mode}`;
}
