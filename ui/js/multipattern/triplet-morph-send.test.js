// Usage: node ui/js/multipattern/triplet-morph-send.test.js
//
// TRIPLET with morph send on has to be a reversible toggle: switching on
// replaces the source with its twelve-cell projection, switching off
// puts the original sixteen cells back with their notes at their
// original indexes. These cover that round trip and every case where the
// restore must refuse rather than write a stale source.

const storage = new Map();
globalThis.sessionStorage = {
    getItem: (key) => (storage.has(key) ? storage.get(key) : null),
    setItem: (key, value) => { storage.set(key, String(value)); },
    removeItem: (key) => { storage.delete(key); },
};
globalThis.window = {
    TD3_CONFIG_ENV: {
        uiDefaultBpm: 120, uiMaxBankHistorySize: 20,
        uiAutoSetLiveUpdate: false, uiDefaultTriplet: false,
    },
};

let planCalls = 0;
let planMode = 'ok';
globalThis.fetch = async (url) => ({
    ok: true,
    async json() {
        if (!String(url).includes('/triplet-morph/plan')) return {};
        planCalls += 1;
        if (planMode === 'ineligible') return { eligible: false };
        if (planMode === 'throw') throw new Error('plan failed');
        return {
            eligible: true,
            plan: {
                planVersion: 1,
                endpointCells: Array.from({ length: 12 }, (_, index) => ({
                    targetIndex: index,
                    sourceStep: index,
                    role: 'survivor',
                    step: {
                        note: `T${index}`, transpose: 'NORMAL',
                        accent: false, slide: false, time: 'NORMAL',
                    },
                })),
            },
        };
    },
});

const send = await import('./triplet-morph-send.js');
const { canonicalPatternText } = await import('../shared/pattern-canonical.js');

let passed = 0;
let failed = 0;
function check(condition, message) {
    if (condition) { passed += 1; return; }
    failed += 1;
    console.error(`  FAIL: ${message}`);
}

function sourcePattern(seed) {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, (_, index) => ({
            note: `${seed}${index}`, transpose: 'NORMAL',
            accent: index % 3 === 0, slide: index % 4 === 0, time: 'NORMAL',
        })),
    };
}

function fakeState(patterns) {
    return {
        patterns,
        getPattern(index) { return this.patterns[index]; },
        setPattern(index, pattern) { this.patterns[index] = pattern; },
    };
}

// --- Round trip ---

send.forgetProjections();
const state = fakeState([sourcePattern('a'), sourcePattern('b')]);
const before = canonicalPatternText(state.getPattern(0));

const applied = await send.applyEndpointProjection(state, [0]);
check(applied.morphed.length === 1 && applied.skipped.length === 0, 'the source is morphed');
check(state.getPattern(0).active_steps === 12, 'twelve active steps');
check(state.getPattern(0).triplet === true, 'triplet on');
check(state.getPattern(0).steps[0].note === 'T0', 'projected content');
check(canonicalPatternText(state.getPattern(1)) === canonicalPatternText(sourcePattern('b')),
    'an untargeted pattern is untouched');

const restored = send.restoreProjectedSources(state, [0]);
check(restored.length === 1, 'the projection is undone');
check(canonicalPatternText(state.getPattern(0)) === before,
    'the original sixteen cells come back with notes at their original indexes');
check(state.getPattern(0).triplet === false, 'triplet is off again');

check(send.restoreProjectedSources(state, [0]).length === 0,
    'a second restore has nothing to undo');

// --- The restore refuses a pattern it did not produce ---

send.forgetProjections();
const edited = fakeState([sourcePattern('c')]);
await send.applyEndpointProjection(edited, [0]);
edited.getPattern(0).steps[0].note = 'X';
check(send.restoreProjectedSources(edited, [0]).length === 0,
    'an edited projection is not overwritten with a stale source');
check(edited.getPattern(0).steps[0].note === 'X', 'the edit survives');

send.forgetProjections();
const swapped = fakeState([sourcePattern('d')]);
await send.applyEndpointProjection(swapped, [0]);
swapped.setPattern(0, sourcePattern('e'));
check(send.restoreProjectedSources(swapped, [0]).length === 0,
    'a replaced pattern is not overwritten');
check(swapped.getPattern(0).steps[0].note === 'e0', 'the replacement survives');

send.forgetProjections();
const forgotten = fakeState([sourcePattern('f')]);
await send.applyEndpointProjection(forgotten, [0]);
send.forgetProjections();
check(send.restoreProjectedSources(forgotten, [0]).length === 0,
    'forgetting drops the memory');

// --- Sources that cannot be projected ---

send.forgetProjections();
const mixed = fakeState([sourcePattern('g'), { ...sourcePattern('h'), triplet: true }]);
const mixedResult = await send.applyEndpointProjection(mixed, [0, 1]);
check(mixedResult.morphed.length === 1 && mixedResult.skipped.length === 1,
    'an already-triplet source is reported as skipped, not morphed');
check(mixed.getPattern(1).active_steps === 16, 'the skipped source is untouched');

send.forgetProjections();
const odd = fakeState([{ ...sourcePattern('i'), active_steps: 7 }]);
const oddResult = await send.applyEndpointProjection(odd, [0]);
check(oddResult.skipped.length === 1, 'a source with a partial beat is skipped');
check(planCalls > 0, 'the planner is consulted for eligible sources');

const callsBefore = planCalls;
await send.applyEndpointProjection(odd, [0]);
check(planCalls === callsBefore, 'an ineligible source never reaches the planner');

// --- The planner refusing or failing is not a silent drop ---

send.forgetProjections();
planMode = 'ineligible';
const refused = fakeState([sourcePattern('j')]);
const refusedResult = await send.applyEndpointProjection(refused, [0]);
check(refusedResult.skipped.length === 1, 'a refused plan is skipped');
check(refused.getPattern(0).active_steps === 16, 'the source is left alone');
check(send.restoreProjectedSources(refused, [0]).length === 0,
    'nothing was remembered for a refused plan');

planMode = 'throw';
const thrown = fakeState([sourcePattern('k')]);
const thrownResult = await send.applyEndpointProjection(thrown, [0]);
check(thrownResult.skipped.length === 1, 'a planner failure is skipped, not thrown');
check(thrown.getPattern(0).active_steps === 16, 'the source survives a planner failure');
planMode = 'ok';

// --- Bulk ---

send.forgetProjections();
const bulk = fakeState([sourcePattern('m'), sourcePattern('n'), sourcePattern('o')]);
const bulkTexts = [0, 1, 2].map((index) => canonicalPatternText(bulk.getPattern(index)));
const bulkResult = await send.applyEndpointProjection(bulk, [0, 1, 2]);
check(bulkResult.morphed.length === 3, 'every eligible target is morphed');
check(send.restorableIndices(bulk, [0, 1, 2]).length === 3, 'all three are restorable');
const bulkRestored = send.restoreProjectedSources(bulk, [0, 1, 2]);
check(bulkRestored.length === 3, 'every one is restored');
check([0, 1, 2].every((index) => canonicalPatternText(bulk.getPattern(index)) === bulkTexts[index]),
    'each pattern gets its own source back, not a neighbour\'s');

// --- The memory survives a page load ---
//
// Leaving for the progression page and coming back is a fresh document.
// Without the stored source the twelve-step projection is the only copy
// of the pattern left, so the sixteen steps are gone for good.

send.forgetProjections();
const crossing = fakeState([sourcePattern('p'), sourcePattern('q')]);
const crossingTexts = [0, 1].map((index) => canonicalPatternText(crossing.getPattern(index)));
await send.applyEndpointProjection(crossing, [0, 1]);
check(storage.has('td3_triplet_morph_send_v1'), 'the projection is written to session storage');

// A reload drops the module memory and rebuilds it from storage.
check(send.loadProjections() === 2, 'both projections are read back');
check(send.restorableIndices(crossing, [0, 1]).length === 2,
    'both are restorable after the reload');
check(send.restoreProjectedSources(crossing, [0, 1]).length === 2, 'both restore');
check([0, 1].every((i) => canonicalPatternText(crossing.getPattern(i)) === crossingTexts[i]),
    'each pattern gets its own sixteen steps back across the reload');
check(!storage.has('td3_triplet_morph_send_v1'),
    'the stored memory is cleared once nothing is projected');

// The canonical guard still runs against a rehydrated projection.
send.forgetProjections();
const tampered = fakeState([sourcePattern('r')]);
await send.applyEndpointProjection(tampered, [0]);
check(send.loadProjections() === 1, 'the projection is read back');
tampered.getPattern(0).steps[0].note = 'X';
check(send.restoreProjectedSources(tampered, [0]).length === 0,
    'a projection edited before the reload is still refused');
check(tampered.getPattern(0).steps[0].note === 'X', 'the edit survives the reload');

// Storage is hostile input.
const malformed = [
    'not json',
    '{}',
    '{"version":99,"entries":[]}',
    '{"version":1,"entries":"nope"}',
    '{"version":1,"entries":[{"index":-1,"source":{},"projectedText":"x"}]}',
    '{"version":1,"entries":[{"index":0,"projectedText":"x"}]}',
    '{"version":1,"entries":[{"index":0,"source":{"steps":[]},"projectedText":"x"}]}',
    '{"version":1,"entries":[{"index":0,"source":{"steps":[1,2,3]},"projectedText":""}]}',
];
let survived = true;
for (const raw of malformed) {
    storage.set('td3_triplet_morph_send_v1', raw);
    try {
        if (send.loadProjections() !== 0) survived = false;
    } catch (_) {
        survived = false;
    }
}
check(survived, 'every malformed stored payload is dropped without throwing');

// A partially valid payload keeps only the entries that check out.
storage.set('td3_triplet_morph_send_v1', JSON.stringify({
    version: 1,
    entries: [
        { index: 0, source: sourcePattern('s'), projectedText: 'projected' },
        { index: 1, source: { steps: [] }, projectedText: 'projected' },
    ],
}));
check(send.loadProjections() === 1, 'the valid entry is kept and the broken one dropped');
send.forgetProjections();

console.log(`triplet-morph-send tests: ${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
