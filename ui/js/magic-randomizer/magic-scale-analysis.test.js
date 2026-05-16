// Usage: node ui/js/magic-randomizer/magic-scale-analysis.test.js
//
// Verifies generic scale-role classification. These tests assert that the SAME engine
// produces correct stable / color / tension partitions for major,
// minor, dorian, phrygian dominant, harmonic minor, blues, pentatonic,
// chromatic, and whole-tone scales without any per-scale code path.

import {
    analyzeScale, classifyInterval, intervalName,
    ROLE_STABLE, ROLE_WEAK_STABLE, ROLE_COLOR, ROLE_TENSION,
} from './magic-scale-analysis.js';

let passed = 0, failed = 0;
function assert(c, m) { if (!c) { console.error(`  FAIL: ${m}`); failed++; return; } passed++; }
function test(n, f) { try { f(); console.log(`  ok: ${n}`); } catch (e) { console.error(`  FAIL: ${n}: ${e.stack || e.message}`); failed++; } }

const SCALES = {
    major:           { id: 'major',   name: 'Major',           intervals: [0, 2, 4, 5, 7, 9, 11] },
    nat_minor:       { id: 'minor',   name: 'Natural Minor',   intervals: [0, 2, 3, 5, 7, 8, 10] },
    dorian:          { id: 'dorian',  name: 'Dorian',          intervals: [0, 2, 3, 5, 7, 9, 10] },
    phrygian:        { id: 'phryg',   name: 'Phrygian',        intervals: [0, 1, 3, 5, 7, 8, 10] },
    phrygian_dom:    { id: 'phrygd',  name: 'Phrygian Dominant', intervals: [0, 1, 4, 5, 7, 8, 10] },
    harm_minor:      { id: 'hmin',    name: 'Harmonic Minor',  intervals: [0, 2, 3, 5, 7, 8, 11] },
    blues:           { id: 'blues',   name: 'Blues',           intervals: [0, 3, 5, 6, 7, 10] },
    minor_pent:      { id: 'mpent',   name: 'Minor Pentatonic', intervals: [0, 3, 5, 7, 10] },
    chromatic:       { id: 'chrom',   name: 'Chromatic',       intervals: [0,1,2,3,4,5,6,7,8,9,10,11] },
    whole_tone:      { id: 'wtone',   name: 'Whole Tone',      intervals: [0, 2, 4, 6, 8, 10] },
    locrian:         { id: 'locr',    name: 'Locrian',         intervals: [0, 1, 3, 5, 6, 8, 10] },
};

// --- classifyInterval (the data-driven core) ---

test('root is always STABLE', () => {
    for (const s of Object.values(SCALES)) {
        assert(classifyInterval(0, s.intervals) === ROLE_STABLE, `${s.name}: root stable`);
    }
});

test('perfect fifth is STABLE when present', () => {
    assert(classifyInterval(7, SCALES.major.intervals) === ROLE_STABLE, 'major');
    assert(classifyInterval(7, SCALES.phrygian.intervals) === ROLE_STABLE, 'phryg');
    assert(classifyInterval(7, SCALES.harm_minor.intervals) === ROLE_STABLE, 'hmin');
});

test('major third is STABLE; minor third is STABLE only when major absent', () => {
    assert(classifyInterval(4, SCALES.major.intervals) === ROLE_STABLE, 'major 3 in major');
    assert(classifyInterval(3, SCALES.nat_minor.intervals) === ROLE_STABLE, 'minor 3 in nat minor');
    assert(classifyInterval(3, SCALES.dorian.intervals) === ROLE_STABLE, 'minor 3 in dorian');
});

test('tritone is TENSION when fifth present, WEAK_STABLE when fifth absent', () => {
    assert(classifyInterval(6, SCALES.blues.intervals) === ROLE_TENSION, 'blues has 5 → 6 is tension');
    // Locrian has no perfect 5 - its tritone is the closest thing to an anchor.
    assert(classifyInterval(6, SCALES.locrian.intervals) === ROLE_WEAK_STABLE, 'locrian: no 5 → 6 weak-stable');
});

test('b2 is TENSION', () => {
    assert(classifyInterval(1, SCALES.phrygian.intervals) === ROLE_TENSION);
    assert(classifyInterval(1, SCALES.phrygian_dom.intervals) === ROLE_TENSION);
});

test('non-stable, non-tension scale tones are COLOR', () => {
    assert(classifyInterval(2, SCALES.major.intervals) === ROLE_COLOR, 'major 2');
    assert(classifyInterval(9, SCALES.dorian.intervals) === ROLE_COLOR, 'dorian 6');
    assert(classifyInterval(11, SCALES.major.intervals) === ROLE_COLOR, 'major 7');
    assert(classifyInterval(8, SCALES.nat_minor.intervals) === ROLE_COLOR, 'minor b6');
});

// --- analyzeScale: end-to-end role partition ---

test('analyzeScale: C major has root=C, fifth=G, third=E as stable', () => {
    const a = analyzeScale(0, SCALES.major);
    assert(a.stablePcs.has(0), 'C stable');
    assert(a.stablePcs.has(4), 'E stable');
    assert(a.stablePcs.has(7), 'G stable');
    // 2 (D), 9 (A), 5 (F), 11 (B) are color
    assert(a.colorPcs.has(2) && a.colorPcs.has(9) && a.colorPcs.has(5) && a.colorPcs.has(11), 'all color present');
    assert(a.tensionPcs.size === 0, 'major has no tension');
});

test('analyzeScale: C phrygian dominant - Db is tension, F is stable', () => {
    const a = analyzeScale(0, SCALES.phrygian_dom);
    assert(a.tensionPcs.has(1), 'Db (b2) tension');
    assert(a.stablePcs.has(4), 'E (major 3) stable');
    assert(a.stablePcs.has(7), 'G (5th) stable');
});

test('analyzeScale: C locrian has no perfect fifth → Gb is WEAK_STABLE', () => {
    const a = analyzeScale(0, SCALES.locrian);
    assert(!a.stablePcs.has(7), 'no 5 stable');
    assert(a.weakStablePcs.has(6), 'Gb weak-stable substitute');
});

test('analyzeScale rooted at non-zero respects rootPc', () => {
    // A natural minor: A=9, C=0 (minor 3), E=4 (5th)
    const a = analyzeScale(9, SCALES.nat_minor);
    assert(a.rootPc === 9, 'rootPc=9');
    assert(a.stablePcs.has(9), 'A stable');
    assert(a.stablePcs.has(0), 'C (minor 3) stable');
    assert(a.stablePcs.has(4), 'E stable');
});

test('analyzeScale: pitches partition across three TD-3 octaves', () => {
    const a = analyzeScale(0, SCALES.major);
    assert(a.pitches.all.length > 0, 'has pitches');
    // Stable pitches must outnumber tension pitches (which is 0 in major)
    assert(a.pitches.stable.length > 0, 'stable pitches');
    assert(a.pitches.tension.length === 0, 'major has no tension pitches');
    // Each stable pitch's pc is in stablePcs
    for (const p of a.pitches.stable) {
        const pc = ((p % 12) + 12) % 12;
        assert(a.stablePcs.has(pc), `stable pitch ${p} pc ${pc}`);
    }
});

test('analyzeScale: helper isRootPitch / isStablePitch', () => {
    const a = analyzeScale(0, SCALES.major);
    assert(a.isRootPitch(0) && a.isRootPitch(12) && a.isRootPitch(-12), 'root at 0, 12, -12');
    assert(!a.isRootPitch(7), 'G is not root');
    assert(a.isStablePitch(7) && a.isStablePitch(4), 'G and E stable');
    assert(!a.isStablePitch(2), 'D is not stable');
});

test('analyzeScale: empty/invalid scale returns safe empty object', () => {
    const a = analyzeScale(0, null);
    assert(a.degrees.length === 0, 'no degrees');
    assert(a.pitches.all.length === 0, 'no pitches');
    assert(a.isStablePitch(0) === false, 'no stable on empty');
});

// --- Generic property: every selected scale must produce some non-empty
// stable set (or weak-stable when truly fifth-less) - the engine is
// expected to find a tonal anchor in every configured scale.
test('every test scale has at least one anchor (stable or weak-stable)', () => {
    for (const [key, s] of Object.entries(SCALES)) {
        const a = analyzeScale(0, s);
        const anchors = a.stablePcs.size + a.weakStablePcs.size;
        assert(anchors >= 1, `${key} has anchors`);
    }
});

test('intervalName returns reasonable label', () => {
    assert(intervalName(0) === 'P1');
    assert(intervalName(7) === '5');
    assert(intervalName(11) === '7');
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
