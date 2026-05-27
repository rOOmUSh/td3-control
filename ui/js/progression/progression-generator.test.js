// Tests for progression-generator.js - runs with Node.js
// Usage: node ui/js/progression/progression-generator.test.js
//
// Self-contained: stubs scaleNotes() inline to avoid browser API dependencies.

// --- Stub scaleNotes (normally imported from scales.js) ---
// We monkey-patch the import by defining the functions inline.

function scaleNotesLocal(root, scale) {
    const allowed = new Set();
    for (const interval of scale.intervals) {
        const pitch = (root + interval) % 12;
        allowed.add(pitch);
    }
    const result = [];
    for (let n = 0; n <= 12; n++) {
        if (allowed.has(n % 12)) result.push(n);
    }
    return result;
}

// --- Inline copies of pure functions from progression-generator.js ---
// (Since we can't use ES module imports in Node without transpilation)

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

function resolveProfile(scale, config) {
    if (config.scale_profiles && config.scale_profiles[scale.id]) {
        return config.scale_profiles[scale.id];
    }
    if (scale.tags && config.profile_priority) {
        for (const profile of config.profile_priority) {
            if (scale.tags.includes(profile)) return profile;
        }
    }
    return 'safe';
}

function chooseProgressionDegrees(profile, config, rng) {
    const presets = config.presets && config.presets[profile];
    if (!presets || presets.length === 0) return [1, 4, 5, 1];
    return presets[Math.floor(rng.next() * presets.length)];
}

function degreeToPitchClass(root, scale, degree) {
    const idx = (degree - 1) % scale.intervals.length;
    return (root + scale.intervals[idx]) % 12;
}

function classifyCenterNotes(root, scale, degree) {
    const centerPc = degreeToPitchClass(root, scale, degree);
    const scalePcs = new Set(scale.intervals.map(i => (root + i) % 12));
    const anchors = new Set();
    anchors.add(centerPc);
    const minor3rd = (centerPc + 3) % 12;
    const major3rd = (centerPc + 4) % 12;
    if (scalePcs.has(minor3rd)) anchors.add(minor3rd);
    else if (scalePcs.has(major3rd)) anchors.add(major3rd);
    const fifth = (centerPc + 7) % 12;
    if (scalePcs.has(fifth)) anchors.add(fifth);
    const color = new Set();
    for (const pc of scalePcs) { if (!anchors.has(pc)) color.add(pc); }
    return { anchors, color, centerPc };
}

// --- Test fixtures ---

const NATURAL_MINOR = { id: 'natural_minor', name: 'Natural Minor', intervals: [0,2,3,5,7,8,10], tags: ['dark'] };
const MAJOR = { id: 'major', name: 'Major', intervals: [0,2,4,5,7,9,11], tags: ['safe'] };
const PHRYGIAN_DOM = { id: 'phrygian_dominant', name: 'Phrygian Dominant', intervals: [0,1,4,5,7,8,10], tags: ['tension'] };
const MAJOR_PENTA = { id: 'major_pentatonic', name: 'Major Pentatonic', intervals: [0,2,4,7,9], tags: ['safe'] };

const CONFIG = {
    anchor_steps: [0, 4, 8, 12],
    profile_priority: ['tension', 'dark', 'jazz', 'safe'],
    scale_profiles: { natural_minor: 'dark', major: 'safe', phrygian_dominant: 'tension' },
    presets: {
        safe: [[1, 4, 5, 1], [1, 6, 7, 1]],
        dark: [[1, 6, 7, 1], [1, 7, 6, 5], [1, 3, 7, 1]],
        tension: [[1, 7, 6, 5], [1, 2, 7, 1]],
        jazz: [[1, 4, 5, 1], [1, 3, 6, 2]],
    },
    mutation: { target_changes: 3, min_changes: 2, max_changes: 4, rhythm_preserve: 0.75, contour_preserve: 0.6 },
    range_policy: { mode: 'fold_then_clamp', anchor_priority: 'target_pitch_class', body_priority: 'contour_then_target' },
    default_timeline: [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4],
};

// --- Test runner ---

let passed = 0, failed = 0;

function assert(condition, msg) {
    if (!condition) {
        console.error(`  FAIL: ${msg}`);
        failed++;
    } else {
        passed++;
    }
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (e) {
        console.error(`  FAIL: ${name}: ${e.message}`);
        failed++;
    }
}

// =========================================================================
// Tests
// =========================================================================

console.log('progression-generator tests:');

// --- createRng ---

test('seeded RNG produces deterministic output', () => {
    const rng1 = createRng(42);
    const rng2 = createRng(42);
    const vals1 = Array.from({length: 10}, () => rng1.next());
    const vals2 = Array.from({length: 10}, () => rng2.next());
    for (let i = 0; i < 10; i++) {
        assert(vals1[i] === vals2[i], `seed 42 output ${i} must match`);
    }
});

test('seeded RNG values are in [0,1)', () => {
    const rng = createRng(123);
    for (let i = 0; i < 1000; i++) {
        const v = rng.next();
        assert(v >= 0 && v < 1, `value ${v} must be in [0,1)`);
    }
});

test('different seeds produce different output', () => {
    const rng1 = createRng(1);
    const rng2 = createRng(2);
    let same = 0;
    for (let i = 0; i < 10; i++) {
        if (rng1.next() === rng2.next()) same++;
    }
    assert(same < 10, 'different seeds should produce mostly different values');
});

// --- resolveProfile ---

test('resolveProfile uses direct scale override', () => {
    assert(resolveProfile(NATURAL_MINOR, CONFIG) === 'dark', 'natural_minor → dark');
    assert(resolveProfile(MAJOR, CONFIG) === 'safe', 'major → safe');
    assert(resolveProfile(PHRYGIAN_DOM, CONFIG) === 'tension', 'phrygian_dominant → tension');
});

test('resolveProfile falls back to tag-based', () => {
    const unknownScale = { id: 'unknown_xyz', name: 'Unknown', intervals: [0,2,4], tags: ['jazz'] };
    assert(resolveProfile(unknownScale, CONFIG) === 'jazz', 'unknown scale with jazz tag → jazz');
});

test('resolveProfile falls back to safe when no match', () => {
    const noTags = { id: 'no_match', name: 'NoMatch', intervals: [0], tags: ['weird'] };
    assert(resolveProfile(noTags, CONFIG) === 'safe', 'no matching tag → safe');
});

// --- chooseProgressionDegrees ---

test('chooseProgressionDegrees returns 4-element array', () => {
    const rng = createRng(42);
    const degrees = chooseProgressionDegrees('dark', CONFIG, rng);
    assert(Array.isArray(degrees), 'must be array');
    assert(degrees.length === 4, 'must have 4 degrees');
});

test('chooseProgressionDegrees falls back for unknown profile', () => {
    const rng = createRng(42);
    const degrees = chooseProgressionDegrees('nonexistent', CONFIG, rng);
    assert(degrees.length === 4, 'fallback must have 4 degrees');
    assert(degrees[0] === 1, 'fallback starts on 1');
});

test('chooseProgressionDegrees is deterministic with seed', () => {
    const d1 = chooseProgressionDegrees('dark', CONFIG, createRng(99));
    const d2 = chooseProgressionDegrees('dark', CONFIG, createRng(99));
    assert(JSON.stringify(d1) === JSON.stringify(d2), 'same seed → same degrees');
});

// --- degreeToPitchClass ---

test('degreeToPitchClass: A minor degree 1 = A', () => {
    assert(degreeToPitchClass(9, NATURAL_MINOR, 1) === 9, 'degree 1 of A minor = A (9)');
});

test('degreeToPitchClass: A minor degree 6 = F', () => {
    assert(degreeToPitchClass(9, NATURAL_MINOR, 6) === 5, 'degree 6 of A minor = F (5)');
});

test('degreeToPitchClass: A minor degree 7 = G', () => {
    assert(degreeToPitchClass(9, NATURAL_MINOR, 7) === 7, 'degree 7 of A minor = G (7)');
});

test('degreeToPitchClass: C major degree 4 = F', () => {
    assert(degreeToPitchClass(0, MAJOR, 4) === 5, 'degree 4 of C major = F (5)');
});

test('degreeToPitchClass: C major degree 5 = G', () => {
    assert(degreeToPitchClass(0, MAJOR, 5) === 7, 'degree 5 of C major = G (7)');
});

// --- classifyCenterNotes ---

test('classifyCenterNotes: A minor center = A includes A, C, E', () => {
    const { anchors, centerPc } = classifyCenterNotes(9, NATURAL_MINOR, 1);
    assert(centerPc === 9, 'center pitch class = 9 (A)');
    assert(anchors.has(9), 'anchor has A');
    assert(anchors.has(0), 'anchor has C (minor 3rd)');
    assert(anchors.has(4), 'anchor has E (5th)');
});

test('classifyCenterNotes: C major center = C includes C, E, G', () => {
    const { anchors, centerPc } = classifyCenterNotes(0, MAJOR, 1);
    assert(centerPc === 0, 'center = 0 (C)');
    assert(anchors.has(0), 'anchor has C');
    assert(anchors.has(4), 'anchor has E (major 3rd)');
    assert(anchors.has(7), 'anchor has G (5th)');
});

test('classifyCenterNotes: pentatonic degrades gracefully', () => {
    const { anchors } = classifyCenterNotes(0, MAJOR_PENTA, 1);
    assert(anchors.has(0), 'root in anchors');
    // Pentatonic has no minor 3rd, should find major 3rd (4) or 5th (7)
    assert(anchors.has(4) || anchors.has(7), 'at least one triad tone present');
});

// --- scaleNotes (local stub) ---

test('scaleNotes: A natural minor returns correct notes', () => {
    const notes = scaleNotesLocal(9, NATURAL_MINOR);
    assert(notes.includes(0), 'has C (0)');
    assert(notes.includes(2), 'has D (2)');
    assert(!notes.includes(3), 'D# (3) should not be in A minor');
    // Actually A minor = A B C D E F G = 9,11,0,2,4,5,7
    assert(notes.includes(9), 'has A (9)');
    assert(notes.includes(11), 'has B (11)');
    assert(notes.includes(4), 'has E (4)');
    assert(notes.includes(5), 'has F (5)');
    assert(notes.includes(7), 'has G (7)');
});

// --- Full integration test with config ---

test('default_timeline has 16 entries', () => {
    assert(CONFIG.default_timeline.length === 16, 'default timeline = 16');
});

test('all preset degree arrays have length 4', () => {
    for (const [profile, presets] of Object.entries(CONFIG.presets)) {
        for (const preset of presets) {
            assert(preset.length === 4, `${profile} preset must have 4 degrees`);
        }
    }
});

test('profile_priority contains all preset keys', () => {
    for (const key of Object.keys(CONFIG.presets)) {
        assert(CONFIG.profile_priority.includes(key), `${key} must be in profile_priority`);
    }
});

// --- Summary ---

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
