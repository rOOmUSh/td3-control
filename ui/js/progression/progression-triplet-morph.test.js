// Usage: node ui/js/progression/progression-triplet-morph.test.js
//
// Morph session behavior on the progression page: eligibility over the
// four acid patterns, reset on canonical source change, archetype swaps
// leaving the session intact, and reload restore.

const values = new Map();
globalThis.sessionStorage = {
    getItem(key) { return values.has(key) ? values.get(key) : null; },
    setItem(key, value) { values.set(key, String(value)); },
    removeItem(key) { values.delete(key); },
};

const pendingFetches = [];
globalThis.fetch = async (url, options) => new Promise((resolve) => {
    pendingFetches.push({
        url,
        body: options?.body ? JSON.parse(options.body) : undefined,
        resolve: (payload) => resolve({ ok: true, async json() { return payload; } }),
    });
});

const state = await import('./progression-state.js');
const { canonicalPatternText } = await import('../shared/pattern-canonical.js');
const { morphRequestPercent } = await import('../shared/triplet-morph-timing.js');

const MORPH_KEY = 'td3_progression_triplet_morph_session_v1';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`  FAIL: ${message}`);
        failed += 1;
        return;
    }
    passed += 1;
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

function straightPattern() {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, () => ({
            note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL',
        })),
    };
}

function reset() {
    state.resetTripletMorphSession();
    state.setPatterns([
        straightPattern(), straightPattern(), straightPattern(), straightPattern(),
    ]);
    state.resetTripletMorphSession();
    pendingFetches.length = 0;
}

console.log('progression triplet morph tests:');

test('morph starts at zero and stays out of the progression state blob', () => {
    reset();
    assert(state.getTripletMorphPercent() === 0, 'defaults to zero');
    assert(!state.isTripletMorphActive(), 'not active at zero');
    state.setTripletMorphPercent(40);
    assert(state.getTripletMorphPercent() === 40, 'amount applied');
    const blob = JSON.parse(sessionStorage.getItem('td3_progression'));
    assert(!('tripletMorphPercent' in blob), 'no morph amount in the page blob');
    assert(!('tripletMorphSession' in blob), 'no morph session in the page blob');
    reset();
});

test('the session uses its own key, not the Control page key', () => {
    reset();
    state.setTripletMorphPercent(30);
    assert(sessionStorage.getItem(MORPH_KEY) !== null, 'progression key written');
    assert(sessionStorage.getItem('td3_triplet_morph_session_v1') === null,
        'the Control page key is untouched');
    const session = JSON.parse(sessionStorage.getItem(MORPH_KEY));
    assert(session.version === 1, 'versioned payload');
    assert(session.canonicalSources.length === 4, 'one text per acid pattern');
    reset();
});

test('eligibility covers all four acid patterns', () => {
    reset();
    assert(state.isTripletMorphSourceEligible(), 'four straight patterns are eligible');

    const withTriplet = [
        straightPattern(), straightPattern(), straightPattern(), straightPattern(),
    ];
    withTriplet[2].triplet = true;
    state.setPatterns(withTriplet);
    assert(!state.isTripletMorphSourceEligible(), 'one native triplet blocks the set');
    state.setTripletMorphPercent(50);
    assert(state.getTripletMorphPercent() === 0, 'positive amount refused');

    const withShort = [
        straightPattern(), straightPattern(), straightPattern(), straightPattern(),
    ];
    withShort[0].active_steps = 7;
    state.setPatterns(withShort);
    assert(!state.isTripletMorphSourceEligible(),
        'a length that is not whole beats blocks the set');
    state.setTripletMorphPercent(50);
    assert(state.getTripletMorphPercent() === 0, 'positive amount refused');

    // Whole four-step beats are all supported, and lengths may differ.
    const mixed = [straightPattern(), straightPattern(), straightPattern(), straightPattern()];
    mixed[0].active_steps = 4;
    mixed[1].active_steps = 8;
    mixed[2].active_steps = 12;
    state.setPatterns(mixed);
    assert(state.isTripletMorphSourceEligible(), 'mixed supported lengths are eligible');
    state.setTripletMorphPercent(50);
    assert(state.getTripletMorphPercent() === 50, 'mixed lengths accept morph');
    reset();
});

test('a canonical step edit resets the amount and clears the session', () => {
    reset();
    state.setTripletMorphPercent(60);
    assert(state.isTripletMorphActive(), 'session open');
    state.setStep(1, 4, {
        note: 'G', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL',
    });
    assert(state.getTripletMorphPercent() === 0, 'edit resets to zero');
    assert(sessionStorage.getItem(MORPH_KEY) === null, 'session storage cleared');
    assert(state.getStep(1, 4).note === 'G', 'the canonical edit still lands');
    reset();
});

test('a bassline archetype swap leaves the morph session intact', () => {
    reset();
    state.setTripletMorphPercent(75);
    // Basslines are derived content and carry the acid pattern's
    // triplet flag, so they are outside the session identity set.
    state.setActiveArchetype(2, 'offbeat');
    assert(state.getTripletMorphPercent() === 75, 'archetype swap keeps the amount');
    assert(state.getActiveArchetype(2) === 'offbeat', 'the swap still applied');
    reset();
});

test('plans cache per canonical source text', () => {
    reset();
    state.setTripletMorphPercent(20);
    const text = canonicalPatternText(state.getPattern(0));
    assert(state.getTripletMorphPlan(text) === null, 'no plan yet');
    state.setTripletMorphPlan(text, { planVersion: 1, assignments: [] });
    assert(state.getTripletMorphPlan(text).planVersion === 1, 'plan cached');
    // All four patterns are identical here, so one plan serves them all.
    assert(state.getTripletMorphPlan(canonicalPatternText(state.getPattern(3))) !== null,
        'identical sources share a cached plan');
    reset();
});

test('returning to zero discards the derived session', () => {
    reset();
    state.setTripletMorphPercent(80);
    state.setTripletMorphPercent(0);
    assert(state.getTripletMorphSession() === null, 'session dropped');
    assert(sessionStorage.getItem(MORPH_KEY) === null, 'storage removed');
});

test('morphRequestPercent gates each audition target independently', () => {
    const straight = straightPattern();
    const nativeTriplet = { ...straightPattern(), triplet: true };
    const short = { ...straightPattern(), active_steps: 7 };
    assert(morphRequestPercent(straight, 65) === 65, 'eligible source carries the amount');
    assert(morphRequestPercent(straight, 0) === 0, 'explicit zero is sent');
    assert(morphRequestPercent(nativeTriplet, 65) === null, 'native triplet omits');
    assert(morphRequestPercent(short, 65) === null, 'a non-beat length omits');
    for (const len of [4, 8, 12, 16]) {
        assert(morphRequestPercent({ ...straightPattern(), active_steps: len }, 65) === 65,
            `${len} steps carries the amount`);
    }
    assert(morphRequestPercent(null, 65) === null, 'missing pattern omits');
});

// Reload restore against the same storage, via a fresh module instance.
{
    reset();
    state.setTripletMorphPercent(55);
    const restored = await import('./progression-state.js?morph=match');
    assert(restored.getTripletMorphPercent() === 55,
        'reload restores the amount when canonical sources match');
    console.log('  ok: reload restores a matching morph session');

    const tampered = JSON.parse(sessionStorage.getItem(MORPH_KEY));
    tampered.canonicalSources[0] = tampered.canonicalSources[0]
        .replace('"note":"C"', '"note":"A#"');
    sessionStorage.setItem(MORPH_KEY, JSON.stringify(tampered));
    const mismatched = await import('./progression-state.js?morph=mismatch');
    assert(mismatched.getTripletMorphPercent() === 0,
        'source mismatch on reload resets to zero');
    assert(sessionStorage.getItem(MORPH_KEY) === null, 'mismatched payload deleted');
    console.log('  ok: reload rejects a mismatched morph session');
    reset();
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
