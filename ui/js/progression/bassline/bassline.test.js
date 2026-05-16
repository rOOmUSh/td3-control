// Bassline archetype tests - Node-runnable, no DOM.
// Usage: node ui/js/progression/bassline/bassline.test.js

import { extractFeatures, isRestStep } from './features.js';
import { pedal }            from './archetypes/pedal.js';
import { rootPulse }        from './archetypes/root-pulse.js';
import { offbeatResponse }  from './archetypes/offbeat-response.js';
import { simplifiedShadow } from './archetypes/simplified-shadow.js';
import { acidArpeggio }     from './archetypes/acid-arpeggio.js';
import { selectDefaultArchetype } from './selector.js';
import { generateAllBasslines } from './generator-v2.js';
import {
    pcToNoteName, scaleDegreesPc, approachBelowPc,
} from './home-movement-approach.js';

// --- Scales ------------------------------------------------------------------

const NATURAL_MINOR = [0, 2, 3, 5, 7, 8, 10];
const MAJOR         = [0, 2, 4, 5, 7, 9, 11];

// --- Fixtures ----------------------------------------------------------------

const NOTE_NAMES = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B','C^'];

function restStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'REST' };
}
function activeStep(name, { accent = false, slide = false } = {}) {
    return { note: name, transpose: 'NORMAL', accent, slide, time: 'NORMAL' };
}


const JAM_NOTES = ['D#','F','F','F#','C','G#','A','A','A','G#','C','F#','E','G','F#','C#'];
function jamPattern() {
    return {
        active_steps: 16,
        triplet: false,
        steps: JAM_NOTES.map(n => activeStep(n)),
    };
}

/** Sparse pattern - only anchor steps active, home on C#. */
function sparsePattern() {
    const steps = Array.from({ length: 16 }, restStep);
    for (const s of [0, 4, 8, 12]) steps[s] = activeStep('C#');
    return { active_steps: 16, triplet: false, steps };
}

/** Medium pattern - every even step active. */
function mediumPattern() {
    const steps = Array.from({ length: 16 }, restStep);
    for (let s = 0; s < 16; s += 2) steps[s] = activeStep('C#');
    return { active_steps: 16, triplet: false, steps };
}

/** Seeded RNG for determinism in tests. */
function createSeededRng(seed) {
    let s = seed >>> 0;
    return {
        next() {
            // Mulberry32
            s = (s + 0x6D2B79F5) | 0;
            let t = s;
            t = Math.imul(t ^ (t >>> 15), t | 1);
            t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
            return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
        },
    };
}

// --- Test harness ------------------------------------------------------------

let passed = 0, failed = 0;
function test(name, fn) {
    try { fn(); console.log(`  ok - ${name}`); passed++; }
    catch (e) { console.log(`  FAIL - ${name}\n    ${e.message}`); failed++; }
}
function assert(cond, msg) { if (!cond) throw new Error(msg || 'assertion failed'); }
function eq(a, b, msg) { if (a !== b) throw new Error(`${msg || 'mismatch'}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`); }

function countActive(pattern) {
    let n = 0;
    for (const s of pattern.steps) if (!isRestStep(s)) n++;
    return n;
}

function noteAt(pattern, idx) { return pattern.steps[idx].note; }

// --- Feature extraction ------------------------------------------------------

console.log('features: extractFeatures');

test('extracts density + active count on jam pattern', () => {
    const p = jamPattern();
    const f = extractFeatures(p);
    eq(f.activeCount, 16, 'all 16 active');
    eq(f.density, 1.0, 'density = 1');
    eq(f.anchorsActive, 4, 'all 4 anchors active');
});

test('chromaFraction flags out-of-scale pitches when scalePcs provided', () => {
    const scalePcs = new Set(scaleDegreesPc(1, NATURAL_MINOR)); // C# minor
    const f = extractFeatures(jamPattern(), { root: 1, scalePcs });
    // Jam pattern includes C, F, G - all out of C# natural minor.
    assert(f.chromaFraction > 0, `expected chroma > 0, got ${f.chromaFraction}`);
});

test('throws on malformed input', () => {
    try { extractFeatures(null); throw new Error('should have thrown'); }
    catch (e) { assert(/16 steps/.test(e.message)); }
});

test('sparsePattern has density 0.25, 4 anchors active', () => {
    const f = extractFeatures(sparsePattern());
    eq(f.activeCount, 4);
    eq(f.density, 0.25);
    eq(f.anchorsActive, 4);
});

// --- PEDAL archetype ---------------------------------------------------------

console.log('\npedal archetype');

test('pedal: home on step 0 (accented) and step 8', () => {
    const rng = createSeededRng(42);
    const features = extractFeatures(jamPattern());
    const p = pedal({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    eq(noteAt(p, 0), 'C#', 'step 0 is home');
    assert(p.steps[0].accent, 'step 0 accented');
    eq(noteAt(p, 8), 'C#', 'step 8 is home');
});

test('pedal: approach on step 15 when lead is not super-sparse', () => {
    const rng = createSeededRng(42);
    const features = extractFeatures(jamPattern());
    const p = pedal({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    assert(!isRestStep(p.steps[15]), 'step 15 active');
    eq(noteAt(p, 15), 'C', 'step 15 = home - 1 (C natural)');
});

test('pedal: very sparse lead upgrades to 4-anchor home', () => {
    const rng = createSeededRng(42);
    const features = extractFeatures({ active_steps: 16, triplet: false, steps: Array.from({ length: 16 }, restStep) });
    features.density = 0.1; // force the sparse branch
    const p = pedal({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    assert(!isRestStep(p.steps[4]), 'step 4 active when lead is sparse');
    assert(!isRestStep(p.steps[12]), 'step 12 active when lead is sparse');
});

test('pedal: active count is small (2-5 notes)', () => {
    const rng = createSeededRng(42);
    const features = extractFeatures(jamPattern());
    const p = pedal({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    const active = countActive(p);
    assert(active >= 2 && active <= 5, `pedal should be sparse, got ${active} active`);
});

// --- ROOT PULSE archetype ----------------------------------------------------

console.log('\nroot-pulse archetype');

test('rootPulse: all 4 anchors active and on home', () => {
    const rng = createSeededRng(7);
    const p = rootPulse({ root: 1, scaleIntervals: NATURAL_MINOR, rng });
    for (const s of [0, 4, 8, 12]) {
        assert(!isRestStep(p.steps[s]), `anchor ${s} active`);
        eq(noteAt(p, s), 'C#', `anchor ${s} is home`);
    }
});

test('rootPulse: approach on step 15', () => {
    const rng = createSeededRng(7);
    const p = rootPulse({ root: 1, scaleIntervals: NATURAL_MINOR, rng });
    assert(!isRestStep(p.steps[15]));
    eq(noteAt(p, 15), 'C');
});

test('rootPulse: exactly 2 mid-bar fills', () => {
    const rng = createSeededRng(7);
    const p = rootPulse({ root: 1, scaleIntervals: NATURAL_MINOR, rng });
    const fillsActive = [2, 6, 10, 14].filter(s => !isRestStep(p.steps[s])).length;
    eq(fillsActive, 2, 'exactly 2 fills from {2,6,10,14}');
});

test('rootPulse: step 0 accented, step 8 accented', () => {
    const rng = createSeededRng(7);
    const p = rootPulse({ root: 1, scaleIntervals: NATURAL_MINOR, rng });
    assert(p.steps[0].accent, 'step 0 accented');
    assert(p.steps[8].accent, 'step 8 accented');
});

// --- OFFBEAT RESPONSE archetype ----------------------------------------------

console.log('\noffbeat-response archetype');

test('offbeatResponse: step 0 always home, accented', () => {
    const rng = createSeededRng(9);
    const features = extractFeatures(mediumPattern());
    const p = offbeatResponse({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: mediumPattern(), features, rng });
    eq(noteAt(p, 0), 'C#');
    assert(p.steps[0].accent);
});

test('offbeatResponse: step 15 is approach to home', () => {
    const rng = createSeededRng(9);
    const features = extractFeatures(mediumPattern());
    const p = offbeatResponse({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: mediumPattern(), features, rng });
    eq(noteAt(p, 15), 'C');
});

test('offbeatResponse: responses avoid lead-active steps when offbeat rests exist', () => {
    // mediumPattern: even steps active, odd steps rest. Responses should
    // prefer odd steps, not stack on active even steps.
    const rng = createSeededRng(9);
    const lead = mediumPattern();
    const features = extractFeatures(lead);
    const p = offbeatResponse({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: lead, features, rng });
    // Count response-era overlaps: active bass steps in the 3..13 odd range
    // that collide with a lead-active even step. There should be zero.
    for (let s = 1; s < 16; s++) {
        if (!isRestStep(p.steps[s]) && !isRestStep(lead.steps[s])) {
            throw new Error(`bass stacked on lead at step ${s}`);
        }
    }
});

test('offbeatResponse: 3-5 response fills on top of the 2 required (0 + 15)', () => {
    const rng = createSeededRng(9);
    const lead = mediumPattern();
    const features = extractFeatures(lead);
    const p = offbeatResponse({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: lead, features, rng });
    const total = countActive(p);
    assert(total >= 5 && total <= 7, `expected 5-7 total active, got ${total}`);
});

// --- SIMPLIFIED SHADOW archetype ---------------------------------------------

console.log('\nsimplified-shadow archetype');

test('shadow: never exceeds lead active count', () => {
    const rng = createSeededRng(3);
    const lead = jamPattern();
    const p = simplifiedShadow({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: lead, rng });
    assert(countActive(p) <= countActive(lead), 'shadow density ≤ lead density');
});

test('shadow: step 0 is always home even when lead rests there', () => {
    const rng = createSeededRng(3);
    const lead = { active_steps: 16, triplet: false, steps: Array.from({ length: 16 }, restStep) };
    lead.steps[8] = activeStep('G#'); // only step 8 active
    const p = simplifiedShadow({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: lead, rng });
    assert(!isRestStep(p.steps[0]), 'step 0 active');
    eq(noteAt(p, 0), 'C#');
    assert(p.steps[0].accent);
});

test('shadow: kept notes are all in scale (step 15 approach exempt)', () => {
    const rng = createSeededRng(3);
    const lead = jamPattern();
    const p = simplifiedShadow({ root: 1, scaleIntervals: NATURAL_MINOR, acidPattern: lead, rng });
    const scalePcs = new Set(scaleDegreesPc(1, NATURAL_MINOR));
    for (let s = 0; s < 15; s++) { // step 15 is the semitone approach, intentionally chromatic
        if (!isRestStep(p.steps[s])) {
            const pc = NOTE_NAMES.indexOf(p.steps[s].note) % 12;
            assert(scalePcs.has(pc), `step ${s} note ${p.steps[s].note} (pc=${pc}) not in C# minor`);
        }
    }
});

// --- ACID ARPEGGIO archetype -------------------------------------------------

console.log('\nacid-arpeggio archetype');

test('arpeggio: hits 0, 4, 6, 8, 12 with 1/3/5/1/1 arp pattern', () => {
    const rng = createSeededRng(99);
    const features = extractFeatures(sparsePattern());
    const p = acidArpeggio({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    // C# minor: home=C#, minor 3 = E, fifth = G#.
    eq(noteAt(p, 0), 'C#', 'step 0 home');
    eq(noteAt(p, 4), 'G#', 'step 4 fifth');
    eq(noteAt(p, 6), 'C#', 'step 6 home');
    eq(noteAt(p, 8), 'G#', 'step 8 fifth');
    eq(noteAt(p, 12), 'C#', 'step 12 home');
});

test('arpeggio: step 15 is approach-below (C), often with slide', () => {
    const rng = createSeededRng(99);
    const features = extractFeatures(sparsePattern());
    const p = acidArpeggio({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    eq(noteAt(p, 15), 'C');
});

test('arpeggio: busy lead thins steps 2 and 10', () => {
    const rng = createSeededRng(99);
    // Fake high density
    const busyLead = {
        active_steps: 16, triplet: false,
        steps: Array.from({ length: 16 }, () => activeStep('C#')),
    };
    const features = extractFeatures(busyLead);
    const p = acidArpeggio({ root: 1, scaleIntervals: NATURAL_MINOR, features, rng });
    assert(isRestStep(p.steps[2]), 'step 2 rested when lead is busy');
    assert(isRestStep(p.steps[10]), 'step 10 rested when lead is busy');
});

// --- Selector ----------------------------------------------------------------

console.log('\nselector');

test('very sparse lead → arpeggio', () => {
    eq(selectDefaultArchetype({ density: 0.25, anchorsActive: 4, syncopation: 0 }), 'arpeggio');
});
test('medium lead → rootPulse', () => {
    eq(selectDefaultArchetype({ density: 0.45, anchorsActive: 3, syncopation: 0 }), 'rootPulse');
});
test('busy-grounded lead → shadow', () => {
    eq(selectDefaultArchetype({ density: 0.65, anchorsActive: 3, syncopation: 0.2 }), 'shadow');
});
test('busy-airy lead → offbeat', () => {
    eq(selectDefaultArchetype({ density: 0.65, anchorsActive: 1, syncopation: 0.2 }), 'offbeat');
});
test('wall-to-wall lead → pedal', () => {
    eq(selectDefaultArchetype({ density: 0.85, anchorsActive: 4, syncopation: 0.2 }), 'pedal');
});
test('highly syncopated lead always → pedal', () => {
    eq(selectDefaultArchetype({ density: 0.3, anchorsActive: 3, syncopation: 0.7 }), 'pedal');
});

// --- Integration -------------------------------------------------------------

console.log('\ngenerateAllBasslines integration');

test('produces 4 pattern entries × 5 archetype keys', () => {
    const rng = createSeededRng(1);
    const harmonicMap = {
        root: 1,
        scaleIntervals: NATURAL_MINOR,
        centers: [
            { centerPc: 1, degree: 1 }, // C#
            { centerPc: 8, degree: 5 }, // G# (fifth of C# minor)
            { centerPc: 4, degree: 3 }, // E (b3)
            { centerPc: 1, degree: 1 }, // back home
        ],
    };
    const acidPatterns = [jamPattern(), mediumPattern(), sparsePattern(), jamPattern()];
    const out = generateAllBasslines({ acidPatterns, harmonicMap, rng });
    eq(out.basslinesByPattern.length, 4);
    for (const set of out.basslinesByPattern) {
        for (const k of ['pedal','rootPulse','offbeat','shadow','arpeggio']) {
            assert(set[k], `missing archetype ${k}`);
            eq(set[k].steps.length, 16);
        }
    }
    eq(out.defaultArchetypeByPattern.length, 4);
});

test('each generated bassline has home at step 0', () => {
    const rng = createSeededRng(11);
    const harmonicMap = {
        root: 1,
        scaleIntervals: NATURAL_MINOR,
        centers: [
            { centerPc: 1, degree: 1 },
            { centerPc: 8, degree: 5 },
            { centerPc: 4, degree: 3 },
            { centerPc: 1, degree: 1 },
        ],
    };
    const out = generateAllBasslines({
        acidPatterns: [jamPattern(), jamPattern(), jamPattern(), jamPattern()],
        harmonicMap, rng,
    });
    const homesByIdx = ['C#', 'G#', 'E', 'C#'];
    for (let i = 0; i < 4; i++) {
        for (const k of ['pedal','rootPulse','offbeat','shadow','arpeggio']) {
            eq(out.basslinesByPattern[i][k].steps[0].note, homesByIdx[i],
               `P${i+1} ${k} step 0 should be home ${homesByIdx[i]}`);
        }
    }
});

// --- Summary -----------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
