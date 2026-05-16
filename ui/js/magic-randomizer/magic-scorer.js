// Score a validated candidate from 0-100.
//
// Weights:
//   15 root placement
//   15 stable strong beats
//   15 movement balance
//   10 scale identity
//   10 phrase shape          (← phrase-template biases live here)
//   10 motif quality
//   10 anti-stuck quality
//   10 loop quality
//    5 register control
//
// "Phrase templates" do not constrain generation - the scorer awards a
// small bonus to candidates that happen to match Root-Anchored,
// Tension-Release, Motif-and-Variation, Acid-Cell, or Climb/Fall
// shapes. The best matching template wins. This way the scorer rewards
// shape without flattening output.

import { ROLE_STABLE } from './magic-scale-analysis.js';

const STRONG_BEATS = [0, 4, 8, 12];

export const SCORE_WEIGHTS = {
    rootPlacement:   15,
    stableStrong:    15,
    movement:        15,
    scaleIdentity:   10,
    phraseShape:     10,
    motif:           10,
    antiStuck:       10,
    loop:            10,
    register:         5,
};

export function scoreCandidate(candidate, analysis, metrics) {
    const m = metrics;
    const breakdown = {};
    breakdown.rootPlacement   = scoreRootPlacement(m);
    breakdown.stableStrong    = scoreStableStrong(m);
    breakdown.movement        = scoreMovement(m);
    breakdown.scaleIdentity   = scoreScaleIdentity(candidate, analysis, m);
    breakdown.phraseShape     = scorePhraseShape(candidate, analysis, m);
    breakdown.motif           = scoreMotif(candidate, m);
    breakdown.antiStuck       = scoreAntiStuck(m);
    breakdown.loop            = scoreLoop(m, analysis);
    breakdown.register        = scoreRegister(m);

    let total = 0;
    for (const [k, score] of Object.entries(breakdown)) {
        total += clamp01(score) * SCORE_WEIGHTS[k];
    }
    return { score: Math.round(total), breakdown };
}

// --- Component scorers - each returns 0..1 ---

function scoreRootPlacement(m) {
    if (m.activeCount === 0) return 0;
    const ratio = m.rootCount / Math.max(1, m.activeCount);
    // Sweet spot ~12-25% - root present but not dominating.
    let score = 0;
    if (ratio === 0) score = 0;
    else if (ratio < 0.08) score = 0.5;
    else if (ratio < 0.30) score = 1.0;
    else if (ratio < 0.45) score = 0.7;
    else score = 0.4;
    // Placement bonus
    if (m.rootInFirstHalf && m.rootInLastQuarter) score = Math.min(1, score + 0.15);
    else if (m.rootInFirstHalf || m.rootInLastQuarter) score = Math.min(1, score + 0.07);
    return score;
}

function scoreStableStrong(m) {
    const activeStrong = STRONG_BEATS.filter(i => m.activeIdx.includes(i));
    if (activeStrong.length === 0) return 0.5;
    const ratio = m.strongStableCount / activeStrong.length;
    return ratio;
}

function scoreMovement(m) {
    if (m.movementCount === 0) return 0.5;
    const total = m.movementCount;
    const r = (k) => m.movementBuckets[k] / total;
    // Targets: repeat 0.15-0.35, step 0.40-0.65, smallLeap 0.10-0.25,
    // medium 0-0.15, large 0-0.05. Penalize distance from sweet spot.
    let s = 1.0;
    s -= Math.max(0, r('repeat')     - 0.45) * 1.5;
    s -= Math.max(0, 0.05            - r('repeat')) * 1.0;
    s -= Math.max(0, 0.30            - r('step'))   * 1.5;
    s -= Math.max(0, r('step')       - 0.80) * 1.0;
    s -= Math.max(0, r('smallLeap')  - 0.35) * 1.0;
    s -= Math.max(0, r('mediumLeap') - 0.20) * 1.0;
    s -= Math.max(0, r('largeLeap')  - 0.07) * 2.0;
    return clamp01(s);
}

function scoreScaleIdentity(candidate, analysis, m) {
    const colorCount = countActivePcs(candidate, analysis.colorPcs);
    const tensionCount = countActivePcs(candidate, analysis.tensionPcs);
    // We want at least one identity tone for non-trivial scales. If the
    // scale only has root + fifth + third (rare), reward the candidate by
    // default - there's nothing distinctive to require.
    const identityPool = analysis.colorPcs.size + analysis.tensionPcs.size;
    if (identityPool === 0) return 0.85;
    const identityHits = colorCount + tensionCount;
    if (identityHits === 0) return 0.0;
    if (identityHits === 1) return 0.6;
    if (identityHits <= 4) return 1.0;
    if (identityHits <= 6) return 0.85;
    return 0.6;  // too much color / tension blurs the tonal center
}

function scorePhraseShape(candidate, analysis, m) {
    // Reward whichever template best fits the candidate. Each template
    // returns 0..1, and we take the max - soft bias, never punitive.
    const a = templateRootAnchored(candidate, analysis, m);
    const b = templateTensionRelease(candidate, analysis, m);
    const c = templateClimbFall(candidate, m);
    const d = templateAcidCell(candidate, m);
    return Math.max(a, b, c, d);
}

function templateRootAnchored(candidate, analysis, m) {
    if (m.activeIdx.length < 4) return 0.4;
    const first = candidate.pitches[m.activeIdx[0]];
    const last = candidate.pitches[m.activeIdx[m.activeIdx.length - 1]];
    let s = 0.4;
    if (analysis.isStablePitch(first) || analysis.isRootPitch(first)) s += 0.3;
    if (analysis.isStablePitch(last)  || analysis.isRootPitch(last))  s += 0.3;
    return clamp01(s);
}

function templateTensionRelease(candidate, analysis, m) {
    if (m.activeIdx.length < 4) return 0.3;
    const first = candidate.pitches[m.activeIdx[0]];
    const last = candidate.pitches[m.activeIdx[m.activeIdx.length - 1]];
    let s = 0.3;
    if (analysis.isTensionPitch(first) || analysis.isColorPitch(first)) s += 0.35;
    if (analysis.isStablePitch(last) || analysis.isRootPitch(last)) s += 0.35;
    return clamp01(s);
}

function templateClimbFall(candidate, m) {
    if (m.activeIdx.length < 4) return 0.3;
    const seq = m.activeIdx.map(i => candidate.pitches[i]);
    let monotonic = 0, total = 0;
    let dir = 0;
    for (let i = 1; i < seq.length; i++) {
        const d = seq[i] - seq[i - 1];
        if (d === 0) continue;
        const sign = d > 0 ? 1 : -1;
        if (dir === 0) dir = sign;
        if (sign === dir) monotonic++;
        total++;
    }
    if (total === 0) return 0.4;
    return 0.3 + (monotonic / total) * 0.6;
}

function templateAcidCell(candidate, m) {
    // Acid-cell: small tonal vocabulary, controlled register.
    if (m.distinctPcs >= 2 && m.distinctPcs <= 4 && m.movementBuckets.largeLeap === 0) {
        return 0.85;
    }
    return 0.35;
}

function scoreMotif(candidate, m) {
    // Reward similar contour between bars 1-4 and 9-12.
    const idx = m.activeIdx;
    if (idx.length < 6) return 0.5;
    const a = idx.filter(i => i < 4).map(i => candidate.pitches[i]);
    const b = idx.filter(i => i >= 8 && i < 12).map(i => candidate.pitches[i]);
    if (a.length === 0 || b.length === 0) return 0.4;
    // Compare contour shape (sign of consecutive deltas).
    const shape = (arr) => {
        const out = [];
        for (let i = 1; i < arr.length; i++) {
            const d = arr[i] - arr[i - 1];
            out.push(d > 0 ? 1 : d < 0 ? -1 : 0);
        }
        return out;
    };
    const sa = shape(a), sb = shape(b);
    const len = Math.min(sa.length, sb.length);
    if (len === 0) return 0.5;
    let same = 0;
    for (let i = 0; i < len; i++) if (sa[i] === sb[i]) same++;
    return 0.4 + (same / len) * 0.6;
}

function scoreAntiStuck(m) {
    let s = 1.0;
    if (m.maxRunLen >= 4) s -= 0.6;          // pre-validator catches this, defensive
    else if (m.maxRunLen === 3) s -= 0.2;
    if (m.activeCount > 0) {
        const dom = m.maxAbsPitchCount / m.activeCount;
        if (dom > 0.45) s -= 0.3;
    }
    return clamp01(s);
}

function scoreLoop(m, analysis) {
    if (m.loopMovement == null) return 0.5;
    let s = 0.6;
    if (m.loopMovement === 0) s = 1.0;
    else if (m.loopMovement <= 2) s = 0.95;
    else if (m.loopMovement <= 4) s = 0.8;
    else if (m.loopMovement <= 7) s = 0.6;
    else s = 0.35;
    if (m.loopLandsOnStable) s = Math.min(1, s + 0.1);
    return s;
}

function scoreRegister(m) {
    const span = m.registerMax - m.registerMin;
    if (span <= 12) return 1.0;
    if (span <= 18) return 0.85;
    if (span <= 24) return 0.6;
    return 0.35;
}

// --- helpers ---

function countActivePcs(candidate, pcSet) {
    if (!pcSet || pcSet.size === 0) return 0;
    let n = 0;
    for (const i of activeIndices(candidate)) {
        const p = candidate.pitches[i];
        const pc = ((p % 12) + 12) % 12;
        if (pcSet.has(pc)) n++;
    }
    return n;
}

function activeIndices(candidate) {
    const out = [];
    for (let i = 0; i < candidate.mask.length; i++) if (candidate.mask[i]) out.push(i);
    return out;
}

function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }

export { STRONG_BEATS };
