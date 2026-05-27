// Key detection for imported patterns.
//
// Given a 16-step pattern, returns {root, scaleId, confidence} by inspecting
// the active steps' pitch content. The detector uses Temperley CBMS profiles
// to estimate tonal center and major/minor quality, then maps the result to
// the nearest project scale id, `major` or `natural_minor`.

// --- Temperley CBMS profiles (active) ---------------------------------------
const MAJOR_PROFILE_TEMPERLEY = [0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400];
const MINOR_PROFILE_TEMPERLEY = [0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330];

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
