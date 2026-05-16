// Supporting bassline generator - pure, deterministic given a seeded RNG.
//
// Contract:
//   - No DOM access, no state writes, no IndexedDB writes, no fetch.
//   - No Math.random(). All randomness flows through the injected rng.
//   - Output patterns follow the same schema as acid patterns
//     (see progression-state.js defaultPattern).
//
// Four rhythm templates are supported:
//   A. four_on_floor - anchors only {0,4,8,12}
//   B. offbeat_8     - eighth-note pulse {0,2,4,6,8,10,12,14}
//   C. anchor_fill   - anchors + 2 fills, preferring acid-REST positions
//   D. acid_follow   - subset of acid's own active steps
// All patterns in a package share ONE rhythm template (picked once per package).
// Pitches are re-rooted to each pattern's centerPc.

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const ANCHOR_STEPS = [0, 4, 8, 12];
const FILL_CANDIDATES = [2, 6, 10, 14];

// ---------------------------------------------------------------------------
// Public entry
// ---------------------------------------------------------------------------

/**
 * Generate 4 supporting bassline patterns, one per acid pattern.
 *
 * @param {Object} params
 * @param {Array} params.acidPatterns   - length 4
 * @param {Object} params.harmonicMap   - from buildHarmonicMap()
 * @param {Object} params.harmonyConfig - parsed harmony-config.json
 * @param {{next:()=>number}} params.rng
 * @returns {{basslines: Array, meta: {rhythmMode: string, derivationLog: Array}}}
 */
export function generateSupportingBasslines({ acidPatterns, harmonicMap, harmonyConfig, rng }) {
    validateInputs(acidPatterns, harmonicMap, harmonyConfig, rng);

    const rhythmMode = resolveRhythmMode(harmonicMap.profile, harmonyConfig);
    const derivationLog = [];
    const basslines = new Array(4);

    // All four basslines share the SAME rhythm template, chosen
    // once per package. For modes that depend on the acid pattern
    // (anchor_fill's REST-avoidance, acid_follow's subset pick), anchor the
    // mask on P1. P2..P4 keep the same active-step mask.
    const masterMask = buildRhythmMask({
        mode: rhythmMode,
        acidPattern: acidPatterns[0],
        harmonyConfig,
        rng,
    });

    for (let i = 0; i < 4; i++) {
        const center = harmonicMap.centers[i];
        const nextCenter = harmonicMap.centers[(i + 1) % 4];
        const acidPattern = acidPatterns[i];

        const pattern = populatePattern({
            mask: masterMask,
            mode: rhythmMode,
            centerPc: center.centerPc,
            scaleIntervals: harmonicMap.scaleIntervals,
            root: harmonicMap.root,
            acidPattern,
            harmonyConfig,
            rng,
        });

        const rewrite = applyEndingRewriteMaybe(pattern, center, nextCenter, harmonyConfig);

        assertDensityInvariants(pattern, acidPattern, i);

        derivationLog.push({
            patternIndex: i,
            degree: center.degree,
            centerPc: center.centerPc,
            rhythmMode,
            endingRewrite: rewrite,
            activeCount: countActive(pattern),
        });

        basslines[i] = pattern;
    }

    return { basslines, meta: { rhythmMode, derivationLog } };
}

function validateInputs(acidPatterns, harmonicMap, harmonyConfig, rng) {
    if (!Array.isArray(acidPatterns) || acidPatterns.length !== 4) {
        throw new Error('generateSupportingBasslines: acidPatterns must be length 4');
    }
    for (let i = 0; i < 4; i++) {
        const p = acidPatterns[i];
        if (!p || !Array.isArray(p.steps) || p.steps.length !== 16) {
            throw new Error(`generateSupportingBasslines: acidPatterns[${i}] must have 16 steps`);
        }
    }
    if (!harmonicMap || !Array.isArray(harmonicMap.centers) || harmonicMap.centers.length !== 4) {
        throw new Error('generateSupportingBasslines: harmonicMap.centers must have length 4');
    }
    if (!Array.isArray(harmonicMap.scaleIntervals) || harmonicMap.scaleIntervals.length === 0) {
        throw new Error('generateSupportingBasslines: harmonicMap.scaleIntervals missing');
    }
    if (!harmonyConfig) {
        throw new Error('generateSupportingBasslines: harmonyConfig is required');
    }
    if (!rng || typeof rng.next !== 'function') {
        throw new Error('generateSupportingBasslines: rng.next() is required');
    }
}

// ---------------------------------------------------------------------------
// Rhythm mode resolution
// ---------------------------------------------------------------------------

/**
 * Resolve rhythm mode from harmony profile. Falls back to four_on_floor
 * if the profile isn't mapped.
 */
export function resolveRhythmMode(profile, config) {
    const modes = (config && config.bass_rhythm_modes) || {};
    return modes[profile] || 'four_on_floor';
}

// ---------------------------------------------------------------------------
// Rhythm mask (boolean[16] of active steps)
// ---------------------------------------------------------------------------

export function buildRhythmMask({ mode, acidPattern, harmonyConfig, rng }) {
    switch (mode) {
        case 'four_on_floor': return maskFourOnFloor();
        case 'offbeat_8':     return maskOffbeat8();
        case 'anchor_fill':   return maskAnchorFill(acidPattern, harmonyConfig, rng);
        case 'acid_follow':   return maskAcidFollow(acidPattern, harmonyConfig, rng);
        default:
            throw new Error(`buildRhythmMask: unknown mode '${mode}'`);
    }
}

function maskFourOnFloor() {
    const mask = new Array(16).fill(false);
    for (const s of ANCHOR_STEPS) mask[s] = true;
    return mask;
}

function maskOffbeat8() {
    const mask = new Array(16).fill(false);
    for (let s = 0; s < 16; s += 2) mask[s] = true;
    return mask;
}

function maskAnchorFill(acidPattern, config, rng) {
    const mask = new Array(16).fill(false);
    for (const s of ANCHOR_STEPS) mask[s] = true;

    const targetFillCount = (config.anchor_fill && config.anchor_fill.target_fill_count) ?? 2;
    const avoidOverlap = (config.anchor_fill && config.anchor_fill.avoid_acid_overlap) ?? true;

    const restFillPositions = FILL_CANDIDATES.filter(s => isRest(acidPattern.steps[s]));

    let picked;
    if (avoidOverlap && restFillPositions.length >= targetFillCount) {
        picked = pickRandom(restFillPositions, targetFillCount, rng);
    } else {
        // Fallback when acid is too dense to honor the avoid-overlap rule.
        // Pick from the full fill-candidate set at random.
        picked = pickRandom(FILL_CANDIDATES, Math.min(targetFillCount, FILL_CANDIDATES.length), rng);
    }
    for (const s of picked) mask[s] = true;
    return mask;
}

function maskAcidFollow(acidPattern, config, rng) {
    const mask = new Array(16).fill(false);
    mask[0] = true; // always keep step 0

    const fraction = (config.acid_follow && config.acid_follow.density_fraction) ?? 0.5;
    const minAdditional = (config.acid_follow && config.acid_follow.min_additional_active) ?? 3;

    // Acid active excluding step 0
    const acidActive = [];
    for (let s = 1; s < 16; s++) {
        if (!isRest(acidPattern.steps[s])) acidActive.push(s);
    }

    const baseCount = Math.floor(acidActive.length * fraction);
    const desired = Math.max(minAdditional, baseCount);
    const actual = Math.min(desired, acidActive.length);

    const picked = pickRandom(acidActive, actual, rng);
    for (const s of picked) mask[s] = true;
    return mask;
}

/** Uniform random sample without replacement from `pool`, size k. */
function pickRandom(pool, k, rng) {
    const copy = pool.slice();
    const out = [];
    for (let i = 0; i < k && copy.length > 0; i++) {
        const idx = Math.floor(rng.next() * copy.length);
        out.push(copy.splice(idx, 1)[0]);
    }
    return out;
}

// ---------------------------------------------------------------------------
// Pattern population - combine rhythm mask + center into a Pattern object
// ---------------------------------------------------------------------------

function populatePattern({ mask, mode, centerPc, scaleIntervals, root, acidPattern, harmonyConfig, rng }) {
    const rootName = NOTE_NAMES[centerPc];
    const scalePcs = new Set(scaleIntervals.map(i => (root + i) % 12));

    const pitchCfg = harmonyConfig.pitch_layer || {};
    const rootProb = pitchCfg.root_probability_nonanchor ?? 0.8;
    const fallbackOrder = pitchCfg.fifth_fallback_order || ['fifth', 'fourth', 'third'];

    const secondaryPc = pickSecondaryPc(centerPc, scalePcs, fallbackOrder);
    const secondaryName = NOTE_NAMES[secondaryPc];

    const anchorSet = new Set(ANCHOR_STEPS);
    const steps = new Array(16);

    for (let s = 0; s < 16; s++) {
        if (!mask[s]) {
            steps[s] = restStep(rootName);
            continue;
        }
        const isAnchor = anchorSet.has(s);
        const note = isAnchor
            ? rootName
            : (rng.next() < rootProb ? rootName : secondaryName);
        steps[s] = {
            note,
            transpose: 'NORMAL',
            accent: computeAccent(mode, s, mask, acidPattern),
            slide: false, // V1: strip all slides
            time: 'NORMAL', // V1: no ties
        };
    }

    return {
        active_steps: 16,
        triplet: !!acidPattern.triplet,
        steps,
    };
}

function restStep(noteName) {
    // Inactive slots still carry a valid note name so UI/serializers never
    // see undefined. Root is a safe, predictable placeholder.
    return {
        note: noteName,
        transpose: 'NORMAL',
        accent: false,
        slide: false,
        time: 'REST',
    };
}

/**
 * Pick the secondary pitch class used at non-anchor active steps (the
 * 20% branch of the pitch layer). Walks the fallback order, returning
 * the first interval whose pitch class is in the scale.
 *
 * Final safety net: if no candidate is in scale (extreme scales like
 * whole_tone with odd intervals), return the raw fifth so the function
 * is total and the caller always gets a valid NOTE_NAMES index.
 */
export function pickSecondaryPc(centerPc, scalePcs, order) {
    for (const kind of order) {
        if (kind === 'fifth') {
            const pc = (centerPc + 7) % 12;
            if (scalePcs.has(pc)) return pc;
        } else if (kind === 'fourth') {
            const pc = (centerPc + 5) % 12;
            if (scalePcs.has(pc)) return pc;
        } else if (kind === 'third') {
            const minor3 = (centerPc + 3) % 12;
            if (scalePcs.has(minor3)) return minor3;
            const major3 = (centerPc + 4) % 12;
            if (scalePcs.has(major3)) return major3;
        }
    }
    return (centerPc + 7) % 12;
}

/**
 * Per-mode accent rule. Step 0 is always accented.
 */
export function computeAccent(mode, stepIdx, mask, acidPattern) {
    if (stepIdx === 0) return true;
    if (mode === 'four_on_floor') return false;
    if (mode === 'offbeat_8') return stepIdx === 8;
    if (mode === 'anchor_fill') return stepIdx === 8 && mask[8];
    if (mode === 'acid_follow') return !!(acidPattern && acidPattern.steps[stepIdx] && acidPattern.steps[stepIdx].accent);
    return false;
}

// ---------------------------------------------------------------------------
// Ending rewrite - overwrite step 14 with next pattern's root if the
// center changes. Only applies when step 14 is an active NORMAL step.
// ---------------------------------------------------------------------------

export function applyEndingRewriteMaybe(pattern, center, nextCenter, config) {
    const cfg = config.ending_rewrite;
    if (!cfg || !cfg.enabled) return null;
    const stepIdx = cfg.step_index ?? 14;
    const step = pattern.steps[stepIdx];
    if (!step || step.time !== 'NORMAL') return null;       // step is REST: nothing to rewrite
    if (center.centerPc === nextCenter.centerPc) return null; // same center: no lead-in needed
    const newName = NOTE_NAMES[nextCenter.centerPc];
    step.note = newName;
    return { stepIndex: stepIdx, note: newName, fromCenter: center.centerPc, toCenter: nextCenter.centerPc };
}

// ---------------------------------------------------------------------------
// Density invariants - refuse to ship a broken pattern.
// ---------------------------------------------------------------------------

export function assertDensityInvariants(pattern, acidPattern, patternIndex) {
    const label = `P${patternIndex + 1}_BASSLINE`;
    const active = countActive(pattern);
    const acidActive = countActive(acidPattern);

    if (active < 4) {
        throw new Error(`density invariant failed [${label}]: active=${active} < 4`);
    }
    if (active > acidActive) {
        throw new Error(`density invariant failed [${label}]: bassline active=${active} > acid active=${acidActive}`);
    }
    const hasAnchor = ANCHOR_STEPS.some(s => !isRest(pattern.steps[s]));
    if (!hasAnchor) {
        throw new Error(`density invariant failed [${label}]: no anchor step active`);
    }
    if (isRest(pattern.steps[0])) {
        throw new Error(`density invariant failed [${label}]: step 0 is not active`);
    }
    if (!pattern.steps[0].accent) {
        throw new Error(`density invariant failed [${label}]: step 0 is not accented`);
    }
}

// ---------------------------------------------------------------------------
// Small helpers (also exported for tests)
// ---------------------------------------------------------------------------

export function isRest(step) {
    return step && (step.time === 'REST' || step.time === 'TIE_REST');
}

export function countActive(pattern) {
    if (!pattern || !Array.isArray(pattern.steps)) return 0;
    let n = 0;
    for (const s of pattern.steps) if (!isRest(s)) n++;
    return n;
}
