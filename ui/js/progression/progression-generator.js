// Pattern progression generator - pure helpers for generating 4 related patterns.
//
// All helpers receive RNG explicitly. No function calls Math.random() directly.
// DOM and state interaction happens only in the caller (progression-main.js).

import { scaleNotes } from '../scales.js';
import { clamp } from '../shared/math.js';

// ---------------------------------------------------------------------------
// RNG wrapper - seedable for testing, Math.random for production
// ---------------------------------------------------------------------------

/** Create an RNG object. If seed is null, uses Math.random. */
export function createRng(seed) {
    if (seed == null) {
        return { next: () => Math.random() };
    }
    // Simple mulberry32 PRNG for deterministic tests
    let s = seed | 0;
    return {
        next() {
            s |= 0; s = s + 0x6D2B79F5 | 0;
            let t = Math.imul(s ^ s >>> 15, 1 | s);
            t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
            return ((t ^ t >>> 14) >>> 0) / 4294967296;
        }
    };
}

// ---------------------------------------------------------------------------
// Profile resolution
// ---------------------------------------------------------------------------

/**
 * Resolve the progression profile for a scale.
 * Priority: scale_profiles[scale.id] > tags matched via profile_priority > "safe"
 */
export function resolveProfile(scale, config) {
    // Direct override
    if (config.scale_profiles && config.scale_profiles[scale.id]) {
        return config.scale_profiles[scale.id];
    }
    // Tag-based resolution
    if (scale.tags && config.profile_priority) {
        for (const profile of config.profile_priority) {
            if (scale.tags.includes(profile)) return profile;
        }
    }
    return 'safe';
}

/**
 * Pick a random progression degree template for the given profile.
 * Returns array of 4 scale degrees, e.g. [1, 6, 7, 1].
 */
export function chooseProgressionDegrees(profile, config, rng) {
    const presets = config.presets && config.presets[profile];
    if (!presets || presets.length === 0) {
        return [1, 4, 5, 1]; // fallback
    }
    return presets[Math.floor(rng.next() * presets.length)];
}

// ---------------------------------------------------------------------------
// Note classification
// ---------------------------------------------------------------------------

/**
 * Compute the center pitch class for a given degree in a scale.
 * degree is 1-based. Returns pitch class 0-11.
 */
export function degreeToPitchClass(root, scale, degree) {
    const idx = (degree - 1) % scale.intervals.length;
    return (root + scale.intervals[idx]) % 12;
}

/**
 * Classify notes relative to a center into anchor / color / approach.
 * Returns { anchors: Set<pitchClass>, color: Set<pitchClass> }
 */
export function classifyCenterNotes(root, scale, degree) {
    const centerPc = degreeToPitchClass(root, scale, degree);
    const scaleIntervals = scale.intervals;
    const scalePcs = new Set(scaleIntervals.map(i => (root + i) % 12));

    const anchors = new Set();
    anchors.add(centerPc);

    // Find third (minor=3, major=4 semitones above center)
    const minor3rd = (centerPc + 3) % 12;
    const major3rd = (centerPc + 4) % 12;
    if (scalePcs.has(minor3rd)) anchors.add(minor3rd);
    else if (scalePcs.has(major3rd)) anchors.add(major3rd);

    // Find fifth (7 semitones above center)
    const fifth = (centerPc + 7) % 12;
    if (scalePcs.has(fifth)) anchors.add(fifth);

    const color = new Set();
    for (const pc of scalePcs) {
        if (!anchors.has(pc)) color.add(pc);
    }

    return { anchors, color, centerPc };
}

/**
 * Check if a TD-3 note index (0-12) is a center-supporting note.
 */
function isCenterSupporting(noteIdx, anchors) {
    return anchors.has(noteIdx % 12);
}

// ---------------------------------------------------------------------------
// Scale note helpers
// ---------------------------------------------------------------------------

/**
 * Find the nearest in-scale note index (0-12) to a target pitch class.
 * Prefers the spelling closest to sourceNoteIdx.
 */
function nearestScaleNote(targetPc, sourceNoteIdx, allowedNotes) {
    let best = allowedNotes[0];
    let bestDist = 999;
    for (const n of allowedNotes) {
        if (n % 12 === targetPc) {
            const d = Math.abs(n - sourceNoteIdx);
            if (d < bestDist) { bestDist = d; best = n; }
        }
    }
    // If exact pitch class not found, find closest allowed note
    if (bestDist === 999) {
        for (const n of allowedNotes) {
            const d = Math.abs(n - sourceNoteIdx);
            if (d < bestDist) { bestDist = d; best = n; }
        }
    }
    return best;
}

/**
 * Find the nearest in-scale note to a target index, preserving contour.
 */
function nearestContourNote(sourceNoteIdx, delta, allowedNotes) {
    const target = sourceNoteIdx + delta;
    let best = allowedNotes[0];
    let bestDist = 999;
    for (const n of allowedNotes) {
        const d = Math.abs(n - target);
        if (d < bestDist) { bestDist = d; best = n; }
    }
    return Math.max(0, Math.min(12, best));
}

// ---------------------------------------------------------------------------
// P1 generation - DNA source pattern
// ---------------------------------------------------------------------------

/**
 * Generate the base pattern (P1) with anchor-aware rules.
 *
 * @param {object} opts
 * @param {number} opts.root - Root note 0-11
 * @param {object} opts.scale - Scale object with .intervals, .id, .tags
 * @param {number} opts.degree - Scale degree for this pattern's center (1-based)
 * @param {number} opts.notePercent - Note density 0-1
 * @param {number} opts.slidePercent - Slide density 0-1
 * @param {number} opts.accPercent - Accent density 0-1
 * @param {number[]} opts.anchorSteps - Anchor step positions [0,4,8,12]
 * @param {object} opts.rng - RNG object with .next()
 * @param {string} opts.profile - Progression profile (safe/dark/tension/jazz)
 * @returns {object} Pattern object {active_steps, triplet, steps}
 */
export function generateBasePattern(opts) {
    const { root, scale, degree, notePercent, slidePercent, accPercent, anchorSteps, rng, profile } = opts;
    const notes = scaleNotes(root, scale);
    if (notes.length === 0) return null;

    const { anchors, centerPc } = classifyCenterNotes(root, scale, degree);
    const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
    const noteName = (idx) => NOTE_NAMES[clamp(idx, 0, 12)];

    const totalSteps = 16;

    // Determine active count (ensure anchors are mostly active)
    const activeCount = Math.max(4, Math.round(totalSteps * notePercent));

    // Build active/rest mask - anchors are strongly biased to be active
    const activeSet = new Set();
    for (const a of anchorSteps) {
        if (rng.next() < 0.9) activeSet.add(a);
    }
    // Fill remaining active slots randomly
    const nonAnchor = [];
    for (let i = 0; i < totalSteps; i++) {
        if (!anchorSteps.includes(i)) nonAnchor.push(i);
    }
    shuffle(nonAnchor, rng);
    for (const i of nonAnchor) {
        if (activeSet.size >= activeCount) break;
        activeSet.add(i);
    }
    // Ensure we don't exceed rest limit (0-3 rests)
    while (totalSteps - activeSet.size > 3 && nonAnchor.length > 0) {
        const idx = nonAnchor.pop();
        activeSet.add(idx);
    }

    // Assign slides and accents
    const activePositions = [...activeSet];
    shuffle(activePositions, rng);
    const slideCount = Math.min(3, Math.round(activePositions.length * slidePercent));
    const accCount = Math.round(activePositions.length * accPercent);
    const slideSet = new Set(activePositions.slice(0, slideCount));
    shuffle(activePositions, rng);
    const accSet = new Set(activePositions.slice(0, accCount));

    // Generate note sequence
    const steps = [];
    let prevNoteIdx = findCenterNoteIdx(centerPc, notes);
    let leapCount = 0;

    for (let i = 0; i < totalSteps; i++) {
        if (!activeSet.has(i)) {
            steps.push({
                note: noteName(notes[prevNoteIdx] || 0),
                transpose: 'NORMAL', accent: false, slide: false, time: 'REST',
            });
            continue;
        }

        let noteIdx;
        const isAnchor = anchorSteps.includes(i);

        if (isAnchor) {
            // Anchor: pick a center-supporting note
            const anchorNotes = notes.filter(n => isCenterSupporting(n, anchors));
            if (anchorNotes.length > 0) {
                noteIdx = notes.indexOf(anchorNotes[Math.floor(rng.next() * anchorNotes.length)]);
                if (noteIdx === -1) noteIdx = prevNoteIdx;
            } else {
                noteIdx = prevNoteIdx;
            }
        } else {
            // Non-anchor: melodic motion
            noteIdx = chooseNextNoteForBase(notes, prevNoteIdx, i, anchors, rng, leapCount);
        }

        // Track leaps
        if (Math.abs(noteIdx - prevNoteIdx) > 3) {
            leapCount++;
            // Cap large leaps to 2
            if (leapCount > 2) {
                const dir = noteIdx > prevNoteIdx ? 1 : -1;
                noteIdx = clamp(prevNoteIdx + dir, 0, notes.length - 1);
            }
        }

        prevNoteIdx = noteIdx;

        let transpose = 'NORMAL';
        const r = rng.next();
        if (r < 0.10) transpose = 'UP';
        else if (r < 0.20) transpose = 'DOWN';

        steps.push({
            note: noteName(notes[noteIdx]),
            transpose,
            accent: accSet.has(i),
            slide: slideSet.has(i),
            time: 'NORMAL',
        });
    }

    // Verify center confirmation: at least 2 anchor slots have center-supporting notes
    let centerCount = 0;
    for (const a of anchorSteps) {
        if (steps[a].time !== 'REST' && steps[a].time !== 'TIE_REST') {
            const ni = NOTE_NAMES.indexOf(steps[a].note);
            if (isCenterSupporting(ni, anchors)) centerCount++;
        }
    }
    // If insufficient, fix up the first anchor slots
    if (centerCount < 2) {
        const anchorNotes = notes.filter(n => isCenterSupporting(n, anchors));
        if (anchorNotes.length > 0) {
            for (const a of anchorSteps) {
                if (centerCount >= 2) break;
                if (steps[a].time === 'REST' || steps[a].time === 'TIE_REST') continue;
                const ni = NOTE_NAMES.indexOf(steps[a].note);
                if (!isCenterSupporting(ni, anchors)) {
                    steps[a].note = noteName(anchorNotes[Math.floor(rng.next() * anchorNotes.length)]);
                    centerCount++;
                }
            }
        }
    }

    return { active_steps: 16, triplet: false, steps };
}

function chooseNextNoteForBase(notes, prevIdx, stepIndex, anchors, rng, leapCount) {
    const len = notes.length;
    // Strong beat bias toward anchor notes
    if (stepIndex % 4 === 0 && rng.next() < 0.5) {
        const anchorIndices = [];
        for (let i = 0; i < notes.length; i++) {
            if (isCenterSupporting(notes[i], anchors)) anchorIndices.push(i);
        }
        if (anchorIndices.length > 0) {
            return anchorIndices[Math.floor(rng.next() * anchorIndices.length)];
        }
    }
    const r = rng.next();
    if (r < 0.55) {
        // Step motion
        const dir = rng.next() < 0.5 ? 1 : -1;
        return clamp(prevIdx + dir, 0, len - 1);
    } else if (r < 0.80) {
        // Repeat
        return prevIdx;
    } else {
        // Leap (limited)
        const leap = (Math.floor(rng.next() * 3) + 2) * (rng.next() < 0.5 ? 1 : -1);
        return clamp(prevIdx + leap, 0, len - 1);
    }
}

function findCenterNoteIdx(centerPc, notes) {
    for (let i = 0; i < notes.length; i++) {
        if (notes[i] % 12 === centerPc) return i;
    }
    return Math.floor(notes.length / 2);
}

// ---------------------------------------------------------------------------
// P2-P4 derivation - remap + mutate + ending rewrite
// ---------------------------------------------------------------------------

/**
 * Derive a new pattern from the previous one with a shifted tonal center.
 *
 * @param {object} prevPattern - Previous pattern {active_steps, triplet, steps}
 * @param {object} opts
 * @param {number} opts.root
 * @param {object} opts.scale
 * @param {number} opts.prevDegree - Previous pattern's degree
 * @param {number} opts.degree - This pattern's degree
 * @param {number} opts.nextDegree - Next pattern's degree (for ending rewrite)
 * @param {number[]} opts.anchorSteps
 * @param {object} opts.config - mutation config
 * @param {object} opts.rng
 * @param {string} opts.profile
 * @returns {object} New pattern
 */
export function derivePattern(prevPattern, opts) {
    const { root, scale, degree, nextDegree, anchorSteps, config, rng, profile } = opts;
    const notes = scaleNotes(root, scale);
    if (notes.length === 0) return JSON.parse(JSON.stringify(prevPattern));

    const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
    const noteName = (idx) => NOTE_NAMES[clamp(idx, 0, 12)];
    const noteIndex = (name) => NOTE_NAMES.indexOf(name);

    const { anchors: newAnchors, centerPc: newCenter } = classifyCenterNotes(root, scale, degree);
    const nextCenter = nextDegree != null ? degreeToPitchClass(root, scale, nextDegree) : newCenter;

    // Deep copy steps
    const steps = prevPattern.steps.map(s => ({ ...s }));
    const mutConfig = config.mutation || { target_changes: 3, min_changes: 2, max_changes: 4 };

    // --- Step 1: Remap anchor positions to new center ---
    for (const a of anchorSteps) {
        if (steps[a].time === 'REST' || steps[a].time === 'TIE_REST') continue;
        const ni = noteIndex(steps[a].note);
        const newNote = nearestScaleNote(newCenter, ni, notes);
        steps[a].note = noteName(newNote);
    }

    // --- Step 2: Verify center confirmation (>=2 anchors on center-supporting notes) ---
    let confirmed = 0;
    for (const a of anchorSteps) {
        if (steps[a].time === 'REST' || steps[a].time === 'TIE_REST') continue;
        if (isCenterSupporting(noteIndex(steps[a].note), newAnchors)) confirmed++;
    }
    // Fix up if needed
    if (confirmed < 2) {
        const anchorNotes = notes.filter(n => isCenterSupporting(n, newAnchors));
        if (anchorNotes.length > 0) {
            for (const a of anchorSteps) {
                if (confirmed >= 2) break;
                if (steps[a].time === 'REST' || steps[a].time === 'TIE_REST') continue;
                if (!isCenterSupporting(noteIndex(steps[a].note), newAnchors)) {
                    steps[a].note = noteName(anchorNotes[Math.floor(rng.next() * anchorNotes.length)]);
                    confirmed++;
                }
            }
        }
    }

    // --- Step 3: Mutate non-anchor body notes ---
    // Find mutable positions (active, non-anchor, not in ending window)
    const activeIndices = [];
    for (let i = 0; i < 16; i++) {
        if (steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST') {
            activeIndices.push(i);
        }
    }

    // Ending window: last 2 active notes
    const endingWindow = new Set();
    const lastTwo = activeIndices.slice(-2);
    lastTwo.forEach(i => endingWindow.add(i));

    const anchorSet = new Set(anchorSteps);
    const mutablePositions = activeIndices.filter(i => !anchorSet.has(i) && !endingWindow.has(i));
    shuffle(mutablePositions, rng);

    // Apply mutations
    const targetChanges = mutConfig.target_changes || 3;
    const maxChanges = Math.min(mutConfig.max_changes || 4, mutablePositions.length);
    const minChanges = Math.min(mutConfig.min_changes || 2, maxChanges);
    const numChanges = clamp(targetChanges, minChanges, maxChanges);

    for (let m = 0; m < numChanges && m < mutablePositions.length; m++) {
        const i = mutablePositions[m];
        const prevNi = noteIndex(steps[i].note);
        // Prefer small motion for contour preservation
        const dir = rng.next() < 0.5 ? 1 : -1;
        const step = Math.floor(rng.next() * 3) + 1; // 1-3 scale steps
        let newNi = clamp(prevNi + dir * step, 0, 12);
        // Snap to scale
        newNi = nearestScaleNote(newNi % 12, prevNi, notes);
        steps[i].note = noteName(newNi);
    }

    // --- Step 4: Ending rewrite - last 2 active notes lead toward next center ---
    rewriteEnding(steps, activeIndices, nextCenter, notes, rng, profile);

    // --- Step 5: Slide/accent variation - at most 1 toggle each ---
    if (activeIndices.length > 0) {
        // Toggle one slide
        const slideCandidate = activeIndices[Math.floor(rng.next() * activeIndices.length)];
        if (rng.next() < 0.5) {
            steps[slideCandidate].slide = !steps[slideCandidate].slide;
        }
        // Toggle one accent
        const accCandidate = activeIndices[Math.floor(rng.next() * activeIndices.length)];
        if (rng.next() < 0.5) {
            steps[accCandidate].accent = !steps[accCandidate].accent;
        }
    }

    return { active_steps: prevPattern.active_steps, triplet: prevPattern.triplet, steps };
}

/**
 * Derive a 4-pattern progression chain from an existing P1.
 *
 * P1 is passed through verbatim (deep-cloned into index 0). P2..P4 are
 * generated by chaining `derivePattern` - each sibling derived from the
 * previous one - using the caller-supplied `degrees` as the chord path.
 * This is the shared chain used by both `generateProgression` (after it
 * creates P1 from scratch) and by SEND TO PROGRESSION (which wants to
 * keep an externally-authored P1 and only derive the siblings).
 *
 * @param {object} p1 - Pre-existing P1 {active_steps, triplet, steps}
 * @param {object} opts
 * @param {number} opts.root
 * @param {object} opts.scale
 * @param {number[]} opts.degrees - 4 scale degrees, one per pattern
 * @param {number[]} opts.anchorSteps
 * @param {object} opts.config - progression config (mutation rules etc.)
 * @param {object} opts.rng
 * @param {string} opts.profile
 * @returns {object[]} 4 patterns - [clone(p1), p2, p3, p4]
 */
export function deriveSiblings(p1, opts) {
    const { root, scale, degrees, anchorSteps, config, rng, profile } = opts;
    const patterns = [JSON.parse(JSON.stringify(p1))];
    for (let i = 1; i < 4; i++) {
        const derived = derivePattern(patterns[i - 1], {
            root, scale,
            prevDegree: degrees[i - 1],
            degree: degrees[i],
            nextDegree: degrees[(i + 1) % 4],
            anchorSteps,
            config,
            rng,
            profile,
        });
        patterns.push(derived);
    }
    return patterns;
}

/**
 * Rewrite the last 1-2 active notes to lead into the next center.
 */
function rewriteEnding(steps, activeIndices, nextCenterPc, notes, rng, profile) {
    const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
    const noteName = (idx) => NOTE_NAMES[clamp(idx, 0, 12)];

    if (activeIndices.length === 0) return;

    const lastActive = activeIndices[activeIndices.length - 1];
    const targetNote = nearestScaleNote(nextCenterPc, NOTE_NAMES.indexOf(steps[lastActive].note), notes);

    // Last note: resolve to center or neighbor
    steps[lastActive].note = noteName(targetNote);

    // Second-to-last: approach note (diatonic neighbor)
    if (activeIndices.length >= 2) {
        const secondLast = activeIndices[activeIndices.length - 2];
        const approachDir = rng.next() < 0.5 ? -1 : 1; // below or above
        let approachNote = targetNote + approachDir;

        // For dark/tension profiles, allow chromatic approach
        if ((profile === 'tension' || profile === 'dark') && rng.next() < 0.3) {
            approachNote = targetNote + approachDir; // semitone approach
        } else {
            // Diatonic: find nearest scale note in the approach direction
            approachNote = nearestScaleNote((clamp(targetNote + approachDir, 0, 12)) % 12, targetNote, notes);
        }
        steps[secondLast].note = noteName(clamp(approachNote, 0, 12));
    }
}

// ---------------------------------------------------------------------------
// Full progression generation
// ---------------------------------------------------------------------------

/**
 * Generate a complete 4-pattern progression.
 *
 * @param {object} opts
 * @param {number} opts.root - Root note 0-11
 * @param {object} opts.scale - Scale object
 * @param {number} opts.notePercent - 0-1
 * @param {number} opts.slidePercent - 0-1
 * @param {number} opts.accPercent - 0-1
 * @param {object} opts.progressionConfig - Config from progression-config.json
 * @param {object} [opts.rng] - Optional RNG (defaults to Math.random)
 * @returns {{ patterns: object[], degrees: number[], profile: string, label: string }}
 */
export function generateProgression(opts) {
    const { root, scale, notePercent, slidePercent, accPercent, progressionConfig } = opts;
    const rng = opts.rng || createRng(null);
    const config = progressionConfig;

    const anchorSteps = config.anchor_steps || [0, 4, 8, 12];
    const profile = resolveProfile(scale, config);
    const degrees = chooseProgressionDegrees(profile, config, rng);

    // P1: base pattern
    const p1 = generateBasePattern({
        root, scale, degree: degrees[0],
        notePercent, slidePercent, accPercent,
        anchorSteps, rng, profile,
    });
    if (!p1) return null;

    // P2-P4 derived from P1 using the shared sibling-derivation chain.
    const patterns = deriveSiblings(p1, {
        root, scale, degrees, anchorSteps, config, rng, profile,
    });

    // Build label
    const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const rootName = NOTE_NAMES[root];
    const degreeNames = degrees.map(d => {
        const pc = degreeToPitchClass(root, scale, d);
        return NOTE_NAMES[pc];
    });
    const label = `${rootName} ${scale.name} - ${degreeNames.join(' → ')}`;

    return { patterns, degrees, profile, label };
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

function shuffle(arr, rng) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(rng.next() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
}
