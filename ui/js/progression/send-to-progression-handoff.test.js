// Tests for send-to-progression-handoff.js - runs with Node.js
// Usage: node ui/js/progression/send-to-progression-handoff.test.js
//
// Self-contained: stubs sessionStorage and the generator helpers inline so
// Node (no DOM, no ES-module loader assumptions) can execute the logic.

// --- sessionStorage stub -----------------------------------------------------

function makeStorage(initial = {}) {
    const store = { ...initial };
    return {
        getItem: (k) => (k in store ? store[k] : null),
        setItem: (k, v) => { store[k] = String(v); },
        removeItem: (k) => { delete store[k]; },
        _raw: store,
    };
}

// Install the stub on globalThis so readHandoff sees it.
let session;
function installStorage(initial) {
    session = makeStorage(initial);
    globalThis.sessionStorage = session;
}

// --- Inline copies of the module under test ---------------------------------
// Duplication cost is explicit but small; avoids a module loader and keeps
// the test runnable via plain `node`.

const HANDOFF_KEY = 'td3_progression_handoff';
const FRESHNESS_MS = 30_000;

function readHandoff() {
    let raw;
    try { raw = sessionStorage.getItem(HANDOFF_KEY); } catch { return null; }
    if (!raw) return null;
    try { sessionStorage.removeItem(HANDOFF_KEY); } catch { /* noop */ }

    let blob;
    try { blob = JSON.parse(raw); } catch { return null; }
    if (!blob || typeof blob !== 'object') return null;

    const { p1, root, scale, sentAt } = blob;
    if (!p1 || !Array.isArray(p1.steps) || p1.steps.length !== 16) return null;
    if (typeof root !== 'number' || root < 0 || root > 11) return null;
    if (typeof scale !== 'string' || !scale) return null;
    if (typeof sentAt !== 'number') return null;
    if (Date.now() - sentAt > FRESHNESS_MS) return null;
    return blob;
}

function consumeHandoff(deps) {
    const {
        state, getScale, progressionConfig,
        deriveSiblings, createRng, resolveProfile, chooseProgressionDegrees,
        toast: toastFn,
    } = deps;

    const blob = readHandoff();
    if (!blob) return null;

    const scale = getScale(blob.scale);
    if (!scale) return null;

    const rng = createRng(null);
    const profile = resolveProfile(scale, progressionConfig);
    const degrees = chooseProgressionDegrees(profile, progressionConfig, rng);
    const anchorSteps = progressionConfig.anchor_steps || [0, 4, 8, 12];

    const patterns = deriveSiblings(blob.p1, {
        root: blob.root, scale, degrees, anchorSteps,
        config: progressionConfig, rng, profile,
    });

    state.setPatterns(patterns);
    state.setProgressionRoot(blob.root);
    state.setProgressionScaleId(blob.scale);
    state.setProgressionDegrees(degrees);
    const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const rootName = NOTE_NAMES[blob.root] || '';
    const label = `${rootName} ${scale.name} - from main pattern`;
    state.setProgressionLabel(label);

    if (toastFn) toastFn('Derived from main pattern', 'info');
    return { patterns, root: blob.root, scale, profile, degrees, label };
}

// --- Test harness -----------------------------------------------------------

let passed = 0, failed = 0;
function test(name, fn) {
    try { fn(); console.log(`  ok - ${name}`); passed++; }
    catch (e) { console.log(`  FAIL - ${name}\n    ${e.message}`); failed++; }
}
function assert(cond, msg) { if (!cond) throw new Error(msg || 'assertion failed'); }
function eq(a, b, msg) { if (a !== b) throw new Error(`${msg || 'not equal'}: ${a} !== ${b}`); }

function defaultPattern() {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, () => ({
            note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL',
        })),
    };
}

function validBlob(overrides = {}) {
    return {
        p1: defaultPattern(),
        root: 0,
        scale: 'major',
        sentAt: Date.now(),
        ...overrides,
    };
}

// --- readHandoff tests ------------------------------------------------------

console.log('readHandoff validation:');

test('absent blob → null', () => {
    installStorage({});
    assert(readHandoff() === null);
});

test('valid blob → returns blob', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob()) });
    const b = readHandoff();
    assert(b !== null, 'should return blob');
    eq(b.root, 0, 'root roundtrip');
    eq(b.scale, 'major', 'scale roundtrip');
});

test('one-shot: key removed after successful read', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob()) });
    readHandoff();
    assert(!(HANDOFF_KEY in session._raw), 'key must be removed');
});

test('one-shot: key removed even on malformed JSON', () => {
    installStorage({ [HANDOFF_KEY]: '{not-json' });
    const b = readHandoff();
    assert(b === null, 'malformed → null');
    assert(!(HANDOFF_KEY in session._raw), 'key must be removed even on parse failure');
});

test('stale blob (> 30s) → null', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ sentAt: Date.now() - 60_000 })) });
    assert(readHandoff() === null, 'stale must reject');
});

test('missing p1 → null', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ p1: null })) });
    assert(readHandoff() === null);
});

test('wrong step count → null', () => {
    const p1 = defaultPattern();
    p1.steps = p1.steps.slice(0, 8);
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ p1 })) });
    assert(readHandoff() === null);
});

test('out-of-range root → null', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ root: 99 })) });
    assert(readHandoff() === null);
});

test('non-number root → null', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ root: 'C' })) });
    assert(readHandoff() === null);
});

test('empty scale → null', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ scale: '' })) });
    assert(readHandoff() === null);
});

test('missing sentAt → null', () => {
    const blob = validBlob();
    delete blob.sentAt;
    installStorage({ [HANDOFF_KEY]: JSON.stringify(blob) });
    assert(readHandoff() === null);
});

// --- consumeHandoff tests ---------------------------------------------------

console.log('\nconsumeHandoff end-to-end:');

function makeStateStub() {
    const rec = { patterns: null, root: null, scaleId: null, degrees: null, label: null };
    return {
        rec,
        setPatterns:          (p) => { rec.patterns = p; },
        setProgressionRoot:   (v) => { rec.root = v; },
        setProgressionScaleId:(v) => { rec.scaleId = v; },
        setProgressionDegrees:(v) => { rec.degrees = v; },
        setProgressionLabel:  (v) => { rec.label = v; },
    };
}

function makeDeps(overrides = {}) {
    return {
        state: makeStateStub(),
        getScale: (id) => (id ? { id, name: 'Major', intervals: [0,2,4,5,7,9,11], tags: [] } : null),
        progressionConfig: { anchor_steps: [0,4,8,12], presets: { safe: [[1,4,5,1]] }, profile_priority: ['safe'] },
        deriveSiblings: (p1) => [JSON.parse(JSON.stringify(p1)), defaultPattern(), defaultPattern(), defaultPattern()],
        createRng: () => ({ next: () => 0.5 }),
        resolveProfile: () => 'safe',
        chooseProgressionDegrees: () => [1, 4, 5, 1],
        toast: () => {},
        ...overrides,
    };
}

test('valid handoff → state mutated, returns derived data', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob()) });
    const deps = makeDeps();
    const result = consumeHandoff(deps);
    assert(result !== null, 'should consume');
    assert(Array.isArray(result.patterns) && result.patterns.length === 4, 'patterns returned');
    eq(result.root, 0, 'root returned');
    assert(result.scale && result.scale.id === 'major', 'scale object returned');
    eq(result.profile, 'safe', 'profile returned');
    assert(Array.isArray(result.degrees) && result.degrees.length === 4, 'degrees returned');
    assert(result.label.includes('from main pattern'), 'label returned');
    assert(deps.state.rec.patterns !== null, 'patterns set on state');
    eq(deps.state.rec.patterns.length, 4, '4 patterns');
    eq(deps.state.rec.root, 0, 'root set on state');
    eq(deps.state.rec.scaleId, 'major', 'scaleId set on state');
    assert(deps.state.rec.label.includes('from main pattern'), 'label mentions handoff');
});

test('absent handoff → returns null, state untouched', () => {
    installStorage({});
    const deps = makeDeps();
    const result = consumeHandoff(deps);
    eq(result, null);
    assert(deps.state.rec.patterns === null, 'state untouched');
});

test('unknown scale → returns null', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob()) });
    const deps = makeDeps({ getScale: () => null });
    eq(consumeHandoff(deps), null);
});

test('toast fires on success', () => {
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob()) });
    let toasted = null;
    const deps = makeDeps({ toast: (msg) => { toasted = msg; } });
    consumeHandoff(deps);
    assert(toasted && toasted.includes('main pattern'), 'toast fired');
});

test('p1 passed to deriveSiblings matches handoff blob', () => {
    const p1 = defaultPattern();
    p1.steps[0].note = 'G';
    p1.steps[4].accent = true;
    installStorage({ [HANDOFF_KEY]: JSON.stringify(validBlob({ p1 })) });
    let seen = null;
    const deps = makeDeps({
        deriveSiblings: (input) => { seen = input; return [input, defaultPattern(), defaultPattern(), defaultPattern()]; },
    });
    consumeHandoff(deps);
    eq(seen.steps[0].note, 'G', 'note roundtrip');
    eq(seen.steps[4].accent, true, 'accent roundtrip');
});

// --- Summary ----------------------------------------------------------------

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
