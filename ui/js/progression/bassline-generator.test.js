// Tests for bassline-generator.js pure helpers - runs with Node.js
// Usage: node ui/js/progression/bassline-generator.test.js
//
// Self-contained: inlines the pure functions (existing codebase convention,
// see progression-generator.test.js). When bassline-generator.js changes,
// update the inlined copies below.

// --- Inline copies of pure helpers from bassline-generator.js ---

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const ANCHOR_STEPS = [0, 4, 8, 12];
const FILL_CANDIDATES = [2, 6, 10, 14];

function createRng(seed) {
    if (seed == null) return { next: () => Math.random() };
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

function degreeToPitchClass(root, scale, degree) {
    const idx = (degree - 1) % scale.intervals.length;
    return (root + scale.intervals[idx]) % 12;
}

function isRest(step) {
    return step && (step.time === 'REST' || step.time === 'TIE_REST');
}

function countActive(pattern) {
    if (!pattern || !Array.isArray(pattern.steps)) return 0;
    let n = 0;
    for (const s of pattern.steps) if (!isRest(s)) n++;
    return n;
}

function pickRandom(pool, k, rng) {
    const copy = pool.slice();
    const out = [];
    for (let i = 0; i < k && copy.length > 0; i++) {
        const idx = Math.floor(rng.next() * copy.length);
        out.push(copy.splice(idx, 1)[0]);
    }
    return out;
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
        picked = pickRandom(FILL_CANDIDATES, Math.min(targetFillCount, FILL_CANDIDATES.length), rng);
    }
    for (const s of picked) mask[s] = true;
    return mask;
}

function maskAcidFollow(acidPattern, config, rng) {
    const mask = new Array(16).fill(false);
    mask[0] = true;
    const fraction = (config.acid_follow && config.acid_follow.density_fraction) ?? 0.5;
    const minAdditional = (config.acid_follow && config.acid_follow.min_additional_active) ?? 3;
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

function buildRhythmMask({ mode, acidPattern, harmonyConfig, rng }) {
    switch (mode) {
        case 'four_on_floor': return maskFourOnFloor();
        case 'offbeat_8':     return maskOffbeat8();
        case 'anchor_fill':   return maskAnchorFill(acidPattern, harmonyConfig, rng);
        case 'acid_follow':   return maskAcidFollow(acidPattern, harmonyConfig, rng);
        default: throw new Error(`unknown mode '${mode}'`);
    }
}

function pickSecondaryPc(centerPc, scalePcs, order) {
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

function computeAccent(mode, stepIdx, mask, acidPattern) {
    if (stepIdx === 0) return true;
    if (mode === 'four_on_floor') return false;
    if (mode === 'offbeat_8') return stepIdx === 8;
    if (mode === 'anchor_fill') return stepIdx === 8 && mask[8];
    if (mode === 'acid_follow') return !!(acidPattern && acidPattern.steps[stepIdx] && acidPattern.steps[stepIdx].accent);
    return false;
}

function restStep(noteName) {
    return { note: noteName, transpose: 'NORMAL', accent: false, slide: false, time: 'REST' };
}

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
        if (!mask[s]) { steps[s] = restStep(rootName); continue; }
        const isAnchor = anchorSet.has(s);
        const note = isAnchor ? rootName : (rng.next() < rootProb ? rootName : secondaryName);
        steps[s] = {
            note,
            transpose: 'NORMAL',
            accent: computeAccent(mode, s, mask, acidPattern),
            slide: false,
            time: 'NORMAL',
        };
    }
    return { active_steps: 16, triplet: !!acidPattern.triplet, steps };
}

function applyEndingRewriteMaybe(pattern, center, nextCenter, config) {
    const cfg = config.ending_rewrite;
    if (!cfg || !cfg.enabled) return null;
    const stepIdx = cfg.step_index ?? 14;
    const step = pattern.steps[stepIdx];
    if (!step || step.time !== 'NORMAL') return null;
    if (center.centerPc === nextCenter.centerPc) return null;
    const newName = NOTE_NAMES[nextCenter.centerPc];
    step.note = newName;
    return { stepIndex: stepIdx, note: newName, fromCenter: center.centerPc, toCenter: nextCenter.centerPc };
}

function assertDensityInvariants(pattern, acidPattern, patternIndex) {
    const label = `P${patternIndex + 1}_BASSLINE`;
    const active = countActive(pattern);
    const acidActive = countActive(acidPattern);
    if (active < 4) throw new Error(`density invariant failed [${label}]: active=${active} < 4`);
    if (active > acidActive) throw new Error(`density invariant failed [${label}]: bassline active=${active} > acid active=${acidActive}`);
    const hasAnchor = ANCHOR_STEPS.some(s => !isRest(pattern.steps[s]));
    if (!hasAnchor) throw new Error(`density invariant failed [${label}]: no anchor step active`);
    if (isRest(pattern.steps[0])) throw new Error(`density invariant failed [${label}]: step 0 is not active`);
    if (!pattern.steps[0].accent) throw new Error(`density invariant failed [${label}]: step 0 is not accented`);
}

function resolveRhythmMode(profile, config) {
    const modes = (config && config.bass_rhythm_modes) || {};
    return modes[profile] || 'four_on_floor';
}

function buildHarmonicMap({ packageId, seed, root, scale, profile, degrees, timeline }) {
    const centers = degrees.map((degree, patternIndex) => ({
        patternIndex,
        degree,
        centerPc: degreeToPitchClass(root, scale, degree),
    }));
    return {
        packageId: packageId ?? null,
        createdAt: new Date().toISOString(),
        seed: seed ?? null,
        root,
        scaleId: scale.id ?? '',
        scaleName: scale.name ?? '',
        profile,
        degrees: [...degrees],
        centers,
        timeline: Array.isArray(timeline) ? [...timeline] : [],
        anchorSteps: [0, 4, 8, 12],
        scaleIntervals: [...scale.intervals],
        scaleTags: Array.isArray(scale.tags) ? [...scale.tags] : [],
    };
}

function generateSupportingBasslines({ acidPatterns, harmonicMap, harmonyConfig, rng }) {
    const rhythmMode = resolveRhythmMode(harmonicMap.profile, harmonyConfig);
    const derivationLog = [];
    const basslines = new Array(4);
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

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// Natural minor: the "classic acid test" scale.
const NATURAL_MINOR = { id: 'natural_minor', name: 'Natural Minor', intervals: [0,2,3,5,7,8,10], tags: ['safe','dark'] };
// Locrian - fifth is diminished, so the fallback order matters.
const LOCRIAN = { id: 'locrian', name: 'Locrian', intervals: [0,1,3,5,6,8,10], tags: ['dark'] };
// Whole tone - neither perfect fifth nor perfect fourth are in scale.
const WHOLE_TONE = { id: 'whole_tone', name: 'Whole Tone', intervals: [0,2,4,6,8,10], tags: ['tension'] };

const DEFAULT_CONFIG = {
    bass_rhythm_modes: {
        safe: 'four_on_floor', dark: 'anchor_fill', tension: 'offbeat_8', jazz: 'acid_follow'
    },
    ending_rewrite: { enabled: true, step_index: 14 },
    anchor_fill: { target_fill_count: 2, avoid_acid_overlap: true },
    acid_follow: { density_fraction: 0.5, min_additional_active: 3 },
    pitch_layer: {
        root_probability_nonanchor: 0.8,
        fifth_fallback_order: ['fifth', 'fourth', 'third']
    }
};

function step({ note = 'C', transpose = 'NORMAL', accent = false, slide = false, time = 'NORMAL' } = {}) {
    return { note, transpose, accent, slide, time };
}

function restS(note = 'C') { return step({ note, time: 'REST' }); }

function acidPattern(stepSpec) {
    // stepSpec: array of 16 (partial step objects or 'REST' shorthand).
    const steps = stepSpec.map(s => s === 'REST' ? restS() : step(s));
    return { active_steps: 16, triplet: false, steps };
}

function densePattern() {
    // Every step active, pitches bouncing on the D minor scale.
    const notes = ['D','D','A','F','D','D','F','G','D','A','D','D','F','A','A#','D'];
    return acidPattern(notes.map(n => ({ note: n })));
}

function gapsPattern() {
    // RESTs at UI 3 (idx 2), UI 11 (idx 10), UI 16 (idx 15) - two rests in fill positions.
    const arr = Array(16).fill(null).map((_, i) => ({ note: 'D' }));
    arr[2] = 'REST';
    arr[10] = 'REST';
    arr[15] = 'REST';
    return acidPattern(arr);
}

// ---------------------------------------------------------------------------
// Test runner
// ---------------------------------------------------------------------------

let passed = 0, failed = 0;
function assert(cond, msg) { if (!cond) { console.error(`  FAIL: ${msg}`); failed++; } else passed++; }
function test(name, fn) {
    try { fn(); console.log(`  ok: ${name}`); }
    catch (e) { console.error(`  FAIL: ${name}: ${e.message}`); failed++; }
}

console.log('bassline-generator tests:');

// ---------------------------------------------------------------------------
// resolveRhythmMode
// ---------------------------------------------------------------------------

test('resolveRhythmMode maps profile to mode', () => {
    assert(resolveRhythmMode('safe', DEFAULT_CONFIG) === 'four_on_floor', 'safe → four_on_floor');
    assert(resolveRhythmMode('dark', DEFAULT_CONFIG) === 'anchor_fill', 'dark → anchor_fill');
    assert(resolveRhythmMode('tension', DEFAULT_CONFIG) === 'offbeat_8', 'tension → offbeat_8');
    assert(resolveRhythmMode('jazz', DEFAULT_CONFIG) === 'acid_follow', 'jazz → acid_follow');
});

test('resolveRhythmMode falls back for unknown profile', () => {
    assert(resolveRhythmMode('unknown', DEFAULT_CONFIG) === 'four_on_floor', 'fallback → four_on_floor');
});

// ---------------------------------------------------------------------------
// Rhythm masks
// ---------------------------------------------------------------------------

test('four_on_floor: exactly {0,4,8,12}', () => {
    const mask = maskFourOnFloor();
    for (let i = 0; i < 16; i++) {
        const expected = ANCHOR_STEPS.includes(i);
        assert(mask[i] === expected, `step ${i}: expected ${expected}, got ${mask[i]}`);
    }
});

test('offbeat_8: every even step active', () => {
    const mask = maskOffbeat8();
    for (let i = 0; i < 16; i++) {
        const expected = (i % 2 === 0);
        assert(mask[i] === expected, `step ${i}: expected ${expected}, got ${mask[i]}`);
    }
});

test('anchor_fill picks acid-REST fills when ≥2 rests in {2,6,10,14}', () => {
    const rng = createRng(42);
    const mask = maskAnchorFill(gapsPattern(), DEFAULT_CONFIG, rng);
    // anchors must be on
    for (const s of ANCHOR_STEPS) assert(mask[s], `anchor ${s} must be active`);
    // exactly 2 fills, both from the acid-REST set {2, 10}
    const activeFills = FILL_CANDIDATES.filter(s => mask[s]);
    assert(activeFills.length === 2, `expected 2 fills, got ${activeFills.length}`);
    for (const f of activeFills) {
        assert([2, 10].includes(f), `fill ${f} should come from acid-REST set {2,10}`);
    }
});

test('anchor_fill falls back to RNG when <2 acid-REST positions', () => {
    const rng = createRng(7);
    // Dense pattern has notes at every fill position - 0 rests available.
    const mask = maskAnchorFill(densePattern(), DEFAULT_CONFIG, rng);
    const activeFills = FILL_CANDIDATES.filter(s => mask[s]);
    assert(activeFills.length === 2, `fallback picks target_fill_count=2`);
});

test('acid_follow keeps step 0 always', () => {
    const rng = createRng(1);
    const acid = gapsPattern();
    const mask = maskAcidFollow(acid, DEFAULT_CONFIG, rng);
    assert(mask[0] === true, 'step 0 always kept');
});

test('acid_follow picks only from acid-active steps', () => {
    const rng = createRng(1);
    const acid = gapsPattern(); // rests at 2, 10, 15
    const mask = maskAcidFollow(acid, DEFAULT_CONFIG, rng);
    assert(mask[2] === false, 'rest step 2 must not be selected');
    assert(mask[10] === false, 'rest step 10 must not be selected');
    assert(mask[15] === false, 'rest step 15 must not be selected');
});

test('acid_follow honors min_additional_active (3)', () => {
    const rng = createRng(1);
    // Acid with 6 active (excluding step 0) → 50% = 3, min = 3 → 3 picked.
    const acid = acidPattern(Array(16).fill(null).map((_, i) => {
        if (i === 0) return { note: 'D' };
        if ([3, 5, 7, 9, 11, 13].includes(i)) return { note: 'D' };
        return 'REST';
    }));
    const mask = maskAcidFollow(acid, DEFAULT_CONFIG, rng);
    const picked = mask.filter(Boolean).length;
    assert(picked >= 4, `step 0 + ≥3 picks = ≥4 active; got ${picked}`);
});

// ---------------------------------------------------------------------------
// pickSecondaryPc
// ---------------------------------------------------------------------------

test('pickSecondaryPc: perfect fifth when in scale (natural_minor)', () => {
    const scalePcs = new Set(NATURAL_MINOR.intervals.map(i => (2 + i) % 12)); // root=D(2)
    const pc = pickSecondaryPc(2, scalePcs, ['fifth','fourth','third']);
    assert(pc === 9, `D's fifth = A (9), got ${pc}`);
});

test('pickSecondaryPc: fourth fallback when fifth out of scale (locrian on B)', () => {
    // Locrian from B: root=11, intervals [0,1,3,5,6,8,10] → scalePcs {11,0,2,4,5,7,9}
    // Center B → fifth F# (6) is NOT in scalePcs; fourth E (4) IS in scalePcs.
    const root = 11;
    const scalePcs = new Set(LOCRIAN.intervals.map(i => (root + i) % 12));
    const pc = pickSecondaryPc(11, scalePcs, ['fifth','fourth','third']);
    assert(pc === 4, `locrian B: expect fourth E (4), got ${pc}`);
});

test('pickSecondaryPc: third fallback when fifth and fourth both out (whole_tone)', () => {
    const root = 0;
    const scalePcs = new Set(WHOLE_TONE.intervals.map(i => (root + i) % 12));
    const pc = pickSecondaryPc(0, scalePcs, ['fifth','fourth','third']);
    // centerPc=0: fifth=7 (not in {0,2,4,6,8,10}), fourth=5 (not in), minor3=3 (not in), major3=4 (IN).
    assert(pc === 4, `whole_tone C: expect major 3rd E (4), got ${pc}`);
});

// ---------------------------------------------------------------------------
// computeAccent
// ---------------------------------------------------------------------------

test('computeAccent: step 0 always accented', () => {
    for (const mode of ['four_on_floor','offbeat_8','anchor_fill','acid_follow']) {
        assert(computeAccent(mode, 0, [], {steps:[]}) === true, `${mode} step 0 accented`);
    }
});

test('computeAccent: four_on_floor only accents step 0', () => {
    for (const s of ANCHOR_STEPS) {
        const expected = s === 0;
        assert(computeAccent('four_on_floor', s, [], {}) === expected, `step ${s}: ${expected}`);
    }
});

test('computeAccent: offbeat_8 accents steps 0 and 8', () => {
    assert(computeAccent('offbeat_8', 0, [], {}) === true);
    assert(computeAccent('offbeat_8', 8, [], {}) === true);
    assert(computeAccent('offbeat_8', 4, [], {}) === false);
    assert(computeAccent('offbeat_8', 12, [], {}) === false);
});

test('computeAccent: anchor_fill inherits accent on step 8 only when active', () => {
    const maskActive = new Array(16).fill(false); maskActive[0] = true; maskActive[8] = true;
    assert(computeAccent('anchor_fill', 8, maskActive, {}) === true, 'step 8 active + accented');
    const maskInactive = new Array(16).fill(false); maskInactive[0] = true;
    assert(computeAccent('anchor_fill', 8, maskInactive, {}) === false, 'step 8 inactive → no accent');
});

test('computeAccent: acid_follow inherits acid accents', () => {
    const acid = acidPattern(Array(16).fill(null).map((_, i) => ({ note: 'D', accent: i === 5 })));
    assert(computeAccent('acid_follow', 5, [], acid) === true, 'acid accent at 5 inherited');
    assert(computeAccent('acid_follow', 6, [], acid) === false, 'acid no accent at 6');
});

// ---------------------------------------------------------------------------
// populatePattern - pitch layer
// ---------------------------------------------------------------------------

test('populatePattern: anchors always root for current center', () => {
    const rng = createRng(1);
    const mask = maskOffbeat8();
    const acid = densePattern();
    const p = populatePattern({
        mask, mode: 'offbeat_8', centerPc: 7 /* G */,
        scaleIntervals: NATURAL_MINOR.intervals, root: 2,
        acidPattern: acid, harmonyConfig: DEFAULT_CONFIG, rng,
    });
    for (const s of ANCHOR_STEPS) {
        assert(p.steps[s].note === 'G', `anchor ${s} must be G (root), got ${p.steps[s].note}`);
    }
});

test('populatePattern: non-anchors ~80% root under controlled RNG (offbeat_8 on D minor)', () => {
    // With fresh seeds and 1000 trials, count root rate on step 2 (non-anchor).
    let rootHits = 0, trials = 0;
    for (let seed = 0; seed < 500; seed++) {
        const rng = createRng(seed);
        const mask = maskOffbeat8();
        const p = populatePattern({
            mask, mode: 'offbeat_8', centerPc: 2,
            scaleIntervals: NATURAL_MINOR.intervals, root: 2,
            acidPattern: densePattern(), harmonyConfig: DEFAULT_CONFIG, rng,
        });
        // step 2 is a non-anchor in offbeat_8. Root = D, fifth = A.
        trials++;
        if (p.steps[2].note === 'D') rootHits++;
    }
    const rate = rootHits / trials;
    assert(rate > 0.70 && rate < 0.90, `root rate expected ~0.80, got ${rate.toFixed(2)}`);
});

test('populatePattern: inactive steps are REST placeholders', () => {
    const rng = createRng(1);
    const mask = maskFourOnFloor();
    const p = populatePattern({
        mask, mode: 'four_on_floor', centerPc: 2,
        scaleIntervals: NATURAL_MINOR.intervals, root: 2,
        acidPattern: densePattern(), harmonyConfig: DEFAULT_CONFIG, rng,
    });
    for (let i = 0; i < 16; i++) {
        if (mask[i]) {
            assert(p.steps[i].time === 'NORMAL', `active step ${i} → NORMAL`);
        } else {
            assert(p.steps[i].time === 'REST', `inactive step ${i} → REST`);
        }
    }
});

test('populatePattern: V1 strips all slides and ties', () => {
    const rng = createRng(1);
    const mask = maskOffbeat8();
    const p = populatePattern({
        mask, mode: 'offbeat_8', centerPc: 2,
        scaleIntervals: NATURAL_MINOR.intervals, root: 2,
        acidPattern: densePattern(), harmonyConfig: DEFAULT_CONFIG, rng,
    });
    for (const s of p.steps) {
        assert(s.slide === false, 'no slides in V1');
        assert(s.time === 'NORMAL' || s.time === 'REST', 'no TIE or TIE_REST in V1');
    }
});

// ---------------------------------------------------------------------------
// Ending rewrite
// ---------------------------------------------------------------------------

test('ending rewrite: step 14 overwritten when next center differs', () => {
    const rng = createRng(1);
    const mask = maskOffbeat8(); // step 14 is active
    const p = populatePattern({
        mask, mode: 'offbeat_8', centerPc: 2,
        scaleIntervals: NATURAL_MINOR.intervals, root: 2,
        acidPattern: densePattern(), harmonyConfig: DEFAULT_CONFIG, rng,
    });
    const center = { centerPc: 2 };        // D
    const nextCenter = { centerPc: 9 };    // A
    const res = applyEndingRewriteMaybe(p, center, nextCenter, DEFAULT_CONFIG);
    assert(res !== null, 'rewrite applied');
    assert(p.steps[14].note === 'A', `step 14 note rewritten to A, got ${p.steps[14].note}`);
});

test('ending rewrite: skipped when next center is the same', () => {
    const rng = createRng(1);
    const mask = maskOffbeat8();
    const p = populatePattern({
        mask, mode: 'offbeat_8', centerPc: 2,
        scaleIntervals: NATURAL_MINOR.intervals, root: 2,
        acidPattern: densePattern(), harmonyConfig: DEFAULT_CONFIG, rng,
    });
    const before = p.steps[14].note;
    const res = applyEndingRewriteMaybe(p, { centerPc: 2 }, { centerPc: 2 }, DEFAULT_CONFIG);
    assert(res === null, 'rewrite NOT applied');
    assert(p.steps[14].note === before, 'step 14 unchanged');
});

test('ending rewrite: skipped when step 14 is REST (four_on_floor)', () => {
    const rng = createRng(1);
    const mask = maskFourOnFloor(); // step 14 is NOT active
    const p = populatePattern({
        mask, mode: 'four_on_floor', centerPc: 2,
        scaleIntervals: NATURAL_MINOR.intervals, root: 2,
        acidPattern: densePattern(), harmonyConfig: DEFAULT_CONFIG, rng,
    });
    const res = applyEndingRewriteMaybe(p, { centerPc: 2 }, { centerPc: 9 }, DEFAULT_CONFIG);
    assert(res === null, 'rewrite NOT applied because step 14 is REST');
    assert(p.steps[14].time === 'REST', 'step 14 still REST');
});

// ---------------------------------------------------------------------------
// Density invariants
// ---------------------------------------------------------------------------

test('invariant: active < 4 → throws', () => {
    const acid = densePattern();
    const sparse = { active_steps: 16, triplet: false, steps: Array(16).fill(0).map((_, i) => i === 0 ? step({ accent: true }) : restS()) };
    let threw = false;
    try { assertDensityInvariants(sparse, acid, 0); } catch (e) { threw = true; assert(/< 4/.test(e.message), 'correct error'); }
    assert(threw, 'should throw on active=1');
});

test('invariant: active > acid active → throws', () => {
    const acid = acidPattern(Array(16).fill(0).map((_, i) => i < 5 ? { note: 'D' } : 'REST')); // 5 active
    const bass = { active_steps: 16, triplet: false, steps: Array(16).fill(0).map(() => step({ accent: true })) }; // 16 active
    let threw = false;
    try { assertDensityInvariants(bass, acid, 0); } catch (e) { threw = true; assert(/> acid active/.test(e.message), 'correct error'); }
    assert(threw, 'should throw when bass busier than acid');
});

test('invariant: no anchor active → throws', () => {
    const acid = densePattern();
    const steps = Array(16).fill(0).map(() => step());
    // Clear anchors but fill non-anchors so total active ≥ 4
    for (const s of [0, 4, 8, 12]) steps[s] = restS();
    for (const s of [2, 6, 10, 14]) steps[s] = step({ accent: false });
    const bass = { active_steps: 16, triplet: false, steps };
    let threw = false;
    try { assertDensityInvariants(bass, acid, 0); } catch (e) { threw = true; assert(/no anchor/.test(e.message), 'correct error'); }
    assert(threw, 'should throw on zero anchors');
});

test('invariant: step 0 not accented → throws', () => {
    const acid = densePattern();
    const steps = Array(16).fill(0).map(() => restS());
    for (const s of [0, 4, 8, 12]) steps[s] = step(); // active but no accent on step 0
    const bass = { active_steps: 16, triplet: false, steps };
    let threw = false;
    try { assertDensityInvariants(bass, acid, 0); } catch (e) { threw = true; assert(/not accented/.test(e.message), 'correct error'); }
    assert(threw, 'should throw on unaccented step 0');
});

// ---------------------------------------------------------------------------
// Harmonic map
// ---------------------------------------------------------------------------

test('harmonic map: centers resolved via degreeToPitchClass', () => {
    // A natural minor: root=9, degrees [1,4,6,1] → A, D, F, A → pc {9, 2, 5, 9}
    const hm = buildHarmonicMap({
        packageId: 'pkg1', seed: 42,
        root: 9, scale: NATURAL_MINOR, profile: 'dark',
        degrees: [1, 4, 6, 1],
        timeline: [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4],
    });
    assert(hm.centers.length === 4, 'four centers');
    assert(hm.centers[0].centerPc === 9, `degree 1 → A (9), got ${hm.centers[0].centerPc}`);
    assert(hm.centers[1].centerPc === 2, `degree 4 → D (2), got ${hm.centers[1].centerPc}`);
    assert(hm.centers[2].centerPc === 5, `degree 6 → F (5), got ${hm.centers[2].centerPc}`);
    assert(hm.centers[3].centerPc === 9, `degree 1 → A (9), got ${hm.centers[3].centerPc}`);
    assert(hm.anchorSteps.length === 4, 'anchorSteps populated');
});

// ---------------------------------------------------------------------------
// End-to-end happy path
// ---------------------------------------------------------------------------

test('end-to-end: dark profile → anchor_fill, 4 valid basslines', () => {
    const rng = createRng(123);
    const acid1 = gapsPattern(); acid1.steps[0].accent = true;
    const acid2 = gapsPattern(); acid2.steps[0].accent = true;
    const acid3 = gapsPattern(); acid3.steps[0].accent = true;
    const acid4 = gapsPattern(); acid4.steps[0].accent = true;
    const hm = buildHarmonicMap({
        packageId: 'pkg1', seed: 1,
        root: 2, scale: NATURAL_MINOR, profile: 'dark',
        degrees: [1, 4, 5, 1], // D, G, A, D
        timeline: [],
    });
    const result = generateSupportingBasslines({
        acidPatterns: [acid1, acid2, acid3, acid4],
        harmonicMap: hm,
        harmonyConfig: DEFAULT_CONFIG,
        rng,
    });
    assert(result.meta.rhythmMode === 'anchor_fill', `mode = anchor_fill, got ${result.meta.rhythmMode}`);
    assert(result.basslines.length === 4, 'four basslines');
    for (let i = 0; i < 4; i++) {
        assert(countActive(result.basslines[i]) >= 4, `P${i + 1} has ≥4 active`);
        assert(result.basslines[i].steps[0].accent === true, `P${i + 1} step 0 accented`);
    }
    // Root sanity: P1 anchors = D, P2 anchors = G, P3 anchors = A, P4 anchors = D
    assert(result.basslines[0].steps[0].note === 'D', 'P1 step 0 = D');
    assert(result.basslines[1].steps[0].note === 'G', 'P2 step 0 = G');
    assert(result.basslines[2].steps[0].note === 'A', 'P3 step 0 = A');
    assert(result.basslines[3].steps[0].note === 'D', 'P4 step 0 = D');
});

test('P1..P4: same rhythm mask, re-rooted pitches', () => {
    const rng = createRng(10);
    const acid = gapsPattern(); acid.steps[0].accent = true;
    const hm = buildHarmonicMap({
        packageId: 'pkg', seed: 1, root: 2, scale: NATURAL_MINOR, profile: 'safe',
        degrees: [1, 4, 5, 1], timeline: [],
    });
    const { basslines } = generateSupportingBasslines({
        acidPatterns: [acid, acid, acid, acid],
        harmonicMap: hm, harmonyConfig: DEFAULT_CONFIG, rng,
    });
    // four_on_floor → active steps should be exactly {0,4,8,12} on all 4 patterns
    for (let p = 0; p < 4; p++) {
        for (let s = 0; s < 16; s++) {
            const expected = ANCHOR_STEPS.includes(s);
            const actual = !isRest(basslines[p].steps[s]);
            assert(actual === expected, `P${p + 1} step ${s}: expected ${expected}, got ${actual}`);
        }
    }
    // Pitches re-rooted: P1/P4 anchors = D, P2 anchors = G, P3 anchors = A.
    // Step 14 is REST in four_on_floor, so ending-rewrite never applies.
    assert(basslines[0].steps[0].note === 'D' && basslines[0].steps[4].note === 'D', 'P1 all anchors = D');
    assert(basslines[1].steps[0].note === 'G' && basslines[1].steps[8].note === 'G', 'P2 all anchors = G');
    assert(basslines[2].steps[0].note === 'A' && basslines[2].steps[12].note === 'A', 'P3 all anchors = A');
});

test('end-to-end: tension profile → offbeat_8 with ending rewrite', () => {
    const rng = createRng(55);
    const acid = acidPattern(Array(16).fill(0).map(() => ({ note: 'D', accent: false })));
    acid.steps[0].accent = true;
    const hm = buildHarmonicMap({
        packageId: 'pkg', seed: 1, root: 2, scale: NATURAL_MINOR, profile: 'tension',
        degrees: [1, 4, 5, 1], // D → G → A → D : rewrites on P1, P2, P3
        timeline: [],
    });
    const { basslines, meta } = generateSupportingBasslines({
        acidPatterns: [acid, acid, acid, acid], harmonicMap: hm, harmonyConfig: DEFAULT_CONFIG, rng,
    });
    assert(meta.rhythmMode === 'offbeat_8', 'offbeat_8 mode');
    // Step 14 is active in offbeat_8. Rewrites point to next root.
    assert(basslines[0].steps[14].note === 'G', `P1 step 14 = next root G, got ${basslines[0].steps[14].note}`);
    assert(basslines[1].steps[14].note === 'A', `P2 step 14 = next root A, got ${basslines[1].steps[14].note}`);
    assert(basslines[2].steps[14].note === 'D', `P3 step 14 = next root D, got ${basslines[2].steps[14].note}`);
    // P4 goes back to D → same center as P1? In this test degrees=[1,4,5,1], P4→P1 both D, so NO rewrite.
    // P4 step 14 should stay as a normal pitch-layer pick (root or fifth, both in D minor = D or A).
    assert(['D','A'].includes(basslines[3].steps[14].note), `P4 step 14 = D or A, got ${basslines[3].steps[14].note}`);
});

test('determinism: same seed produces identical output', () => {
    const acid = gapsPattern(); acid.steps[0].accent = true;
    const hm = buildHarmonicMap({
        packageId: 'pkg', seed: 1, root: 2, scale: NATURAL_MINOR, profile: 'dark',
        degrees: [1, 4, 5, 1], timeline: [],
    });
    const run = () => generateSupportingBasslines({
        acidPatterns: [acid, acid, acid, acid], harmonicMap: hm, harmonyConfig: DEFAULT_CONFIG,
        rng: createRng(999),
    });
    const a = run(), b = run();
    assert(JSON.stringify(a.basslines) === JSON.stringify(b.basslines), 'same seed → identical basslines');
});

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
