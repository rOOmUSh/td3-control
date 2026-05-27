// Melody-aware slide and accent placement.
//
// Slider density is preserved exactly - the SL/AC sliders still control
// HOW MANY active steps get a slide / accent. The melody-aware logic
// chooses WHICH active steps to mark.
//
// Slide eligibility (higher = better candidate for a slide):
//   + step-motion to neighbor (|delta| ≤ 2 semitones)
//   + upward motion
//   + tension → stable resolution
//   + before an accented destination (set after accents are picked)
//   - repeated pitch (no glide to do)
//   - before REST (slide goes nowhere)
//   - after REST without continuous phrase
//   - across very large leap (slide on a leap rarely sounds good)
//
// Accent eligibility (higher = better candidate for an accent):
//   + on 16th off-beats (1, 3, 5, 7, 9, 11, 13, 15) - classic acid
//     groove placement; the accent sits between the kick and the
//     hat-aligned 8ths
//   + on the "and" of beats (2, 6, 10, 14) - syncopation
//   + on color / tension tones - they earn the spotlight
//   + after REST (re-entry feels like an attack)
//   + on phrase peaks (highest active pitch)
//   - on downbeats (0, 4, 8, 12) - the kick already accents these
//     positions and an accented 303 note here gets swallowed in the mix
//     and fights mastering. Off-beat accent placement is a deliberate
//     acid-music choice, not a software shortcut.
//
// The downbeat penalty is soft: a downbeat with a tension tone, REST
// pickup, or phrase peak can still beat an unremarkable off-beat note,
// so "occasional on-beat accent" still happens - it just isn't the rule.
//
// We pick the top-N highest-scoring eligible steps where N is the
// slider-driven count. This preserves the user's density expectation
// while pushing those marks to musically sensible positions.

import { decodePitch } from './magic-pitch-encoding.js';

const TOTAL_STEPS = 16;

/**
 * Apply melody-aware slides and accents on top of a step array.
 *
 * @param {object[]} steps  16-element step array (mutated)
 * @param {object} analysis from analyzeScale
 * @param {number} slidePercent  0..1
 * @param {number} accPercent    0..1
 * @returns {object} debug info - picked indices
 */
export function applySlidesAndAccents(steps, analysis, slidePercent, accPercent) {
    if (!Array.isArray(steps) || steps.length !== TOTAL_STEPS) return { slides: [], accents: [] };

    // Reset existing slide/accent flags so this is the single source of
    // truth for melody-aware placement.
    for (const s of steps) { s.slide = false; s.accent = false; }

    const activeIdx = [];
    for (let i = 0; i < TOTAL_STEPS; i++) {
        if (steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST') activeIdx.push(i);
    }
    if (activeIdx.length === 0) return { slides: [], accents: [] };

    // --- Accents first - slides can then peek at "is the next step accented" ---
    const accentScores = scoreAccents(steps, activeIdx, analysis);
    const accCount = Math.round(activeIdx.length * accPercent);
    const accents = pickTopNIndices(accentScores, accCount);
    for (const i of accents) steps[i].accent = true;

    // --- Slides ---
    const slideScores = scoreSlides(steps, activeIdx, analysis);
    const slideCount = Math.round(activeIdx.length * slidePercent);
    const slides = pickTopNIndices(slideScores, slideCount);
    for (const i of slides) steps[i].slide = true;

    return { slides, accents };
}

// ---------------------------------------------------------------------------
// Scoring per active step (returns an array {idx, score} for picking).
// ---------------------------------------------------------------------------

function scoreSlides(steps, activeIdx, analysis) {
    const scores = [];
    for (let n = 0; n < activeIdx.length; n++) {
        const i = activeIdx[n];
        const next = activeIdx[n + 1];
        const nextStep = next != null ? steps[next] : null;
        const hereP = decodePitch(steps[i]);
        const nextP = nextStep ? decodePitch(nextStep) : null;
        let s = 1.0;

        if (nextP == null) { s -= 0.5; }       // last active or before REST
        else {
            const delta = nextP - hereP;
            const absD = Math.abs(delta);
            if (absD === 0) s -= 0.6;          // glide to same pitch
            else if (absD <= 2) s += 0.6;      // step motion = ideal slide
            else if (absD <= 4) s += 0.2;      // small leap
            else if (absD <= 7) s -= 0.1;      // medium leap
            else                s -= 0.5;      // large leap
            if (delta > 0) s += 0.15;          // upward bias
            // Tension → stable resolution
            if (analysis.isTensionPitch(hereP) && analysis.isStablePitch(nextP)) s += 0.4;
            // Sliding into an accent feels intentional
            if (nextStep && nextStep.accent) s += 0.25;
        }
        // Avoid sliding over a REST gap
        if (next != null && next > i + 1) s -= 0.3;
        scores.push({ idx: i, score: s });
    }
    return scores;
}

function scoreAccents(steps, activeIdx, analysis) {
    const scores = [];
    // Pre-compute phrase peak.
    const pitches = activeIdx.map(i => decodePitch(steps[i]));
    const peak = pitches.length ? Math.max(...pitches) : 0;

    for (let n = 0; n < activeIdx.length; n++) {
        const i = activeIdx[n];
        const p = decodePitch(steps[i]);
        let s = 1.0;

        // Position weight - the heart of the acid groove rule. Off-beat
        // 16ths are the most musical accent slots; the "and" of each
        // beat is next; downbeats are penalized so the accent doesn't
        // collide with the kick drum on every bar.
        if (i % 4 === 0)        s -= 0.45;        // downbeat - kick collision
        else if (i % 4 === 2)   s += 0.35;        // "and" of beat
        else                    s += 0.50;        // 16th off-beat (classic acid)

        // Pitch interest. Root/stable get only a token nudge - they
        // already cluster on downbeats from generator bias, and we do
        // not want to indirectly reintroduce the on-beat-accent rule.
        if (analysis.isTensionPitch(p))     s += 0.30;
        else if (analysis.isColorPitch(p))  s += 0.20;
        if (analysis.isRootPitch(p))        s += 0.05;

        // After REST: re-entry attack still rewarded.
        if (i > 0 && (steps[i - 1].time === 'REST' || steps[i - 1].time === 'TIE_REST')) s += 0.30;
        // Phrase peak: highlighting the top of a line is musical.
        if (p === peak) s += 0.20;

        scores.push({ idx: i, score: s });
    }
    return scores;
}

function pickTopNIndices(scored, n) {
    if (n <= 0) return [];
    if (n >= scored.length) return scored.map(s => s.idx);
    const sorted = [...scored].sort((a, b) => b.score - a.score);
    const out = sorted.slice(0, n).map(s => s.idx);
    out.sort((a, b) => a - b);
    return out;
}
