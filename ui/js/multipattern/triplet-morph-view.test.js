// Usage: node ui/js/multipattern/triplet-morph-view.test.js

if (typeof globalThis.sessionStorage === 'undefined') {
    const store = new Map();
    globalThis.sessionStorage = {
        getItem: (k) => (store.has(k) ? store.get(k) : null),
        setItem: (k, v) => { store.set(k, String(v)); },
        removeItem: (k) => { store.delete(k); },
        clear: () => { store.clear(); },
    };
}

const pendingFetches = [];
globalThis.fetch = async (url, options) => new Promise((resolve) => {
    pendingFetches.push({
        url,
        body: options?.body ? JSON.parse(options.body) : undefined,
        resolve: (payload) => resolve({ ok: true, async json() { return payload; } }),
    });
});

const state = await import('./multipattern-state.js');
const view = await import('./triplet-morph-view.js');
const { canonicalPatternText } = await import('../shared/pattern-canonical.js');

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

async function asyncTest(name, fn) {
    try {
        await fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

function reset() {
    state.restoreSnapshot({
        patterns: [
            {
                active_steps: 16,
                triplet: false,
                steps: Array.from({ length: 16 }, () => ({
                    note: 'C', transpose: 'NORMAL',
                    accent: false, slide: false, time: 'NORMAL',
                })),
            },
        ],
        focusedIdx: 0,
        checked: [],
        timeline: [1],
        abMode: 'SERIAL',
        viewport: { group: 'ALL', side: 'ALL' },
    }, true);
    state.resetTripletMorphSession();
    pendingFetches.length = 0;
}

function defaultPlanBody() {
    const assignments = [];
    for (let beat = 0; beat < 4; beat += 1) {
        assignments.push(
            {
                step: beat * 4, survivor: true,
                sourceOffset: { num: 0, den: 1 }, targetOffset: { num: 0, den: 1 },
            },
            {
                step: beat * 4 + 1, survivor: true,
                sourceOffset: { num: 1, den: 4 }, targetOffset: { num: 1, den: 3 },
            },
            {
                step: beat * 4 + 2, survivor: false,
                sourceOffset: { num: 1, den: 2 }, targetOffset: { num: 2, den: 3 },
            },
            {
                step: beat * 4 + 3, survivor: true,
                sourceOffset: { num: 3, den: 4 }, targetOffset: { num: 2, den: 3 },
            },
        );
    }
    return { planVersion: 1, beats: [], assignments, endpointCells: [] };
}

// A step column holding a note card and its UP/DN/SL/AC control block,
// matching what createStepCard builds. The renderer moves the column and
// reaches the control block through it.
function fakeCell() {
    const controls = { style: {}, dataset: {}, className: 'step-controls' };
    const column = {
        style: {},
        dataset: {},
        controls,
        querySelector: (selector) => (selector === '.step-controls' ? controls : null),
    };
    const card = { style: {}, dataset: {}, parentElement: column };
    column.card = card;
    return card;
}

function columnOf(cell) {
    return cell.parentElement;
}

console.log('triplet-morph-view tests:');

test('cell presentation interpolates plan fractions into CSS positions', () => {
    const plan = defaultPlanBody();
    const anchor = view.cellPresentation(plan.assignments[0], 60);
    assert(anchor.translatePercent === 0, 'anchor never moves');
    assert(anchor.role === 'anchor', 'anchor role');

    const survivor = view.cellPresentation(plan.assignments[1], 100);
    assert(Math.abs(survivor.translatePercent - (100 / 3)) < 1e-6,
        'S2 lands a third of a card later at the endpoint');
    assert(survivor.role === 'survivor', 'survivor role');
    assert(!survivor.hidden, 'survivors stay visible');

    const half = view.cellPresentation(plan.assignments[1], 50);
    assert(Math.abs(half.translatePercent - (100 / 6)) < 1e-6,
        'linear amount means half the endpoint displacement at 50');
});

test('the losing cell merges continuously and leaves at the retirement point', () => {
    const plan = defaultPlanBody();
    const loser25 = view.cellPresentation(plan.assignments[2], 25);
    const loser49 = view.cellPresentation(plan.assignments[2], 49);
    const loser50 = view.cellPresentation(plan.assignments[2], view.MORPH_RETIREMENT_PERCENT);
    const loser100 = view.cellPresentation(plan.assignments[2], 100);
    assert(loser25.role === 'loser', 'loser role');
    assert(loser25.merging && loser49.merging, 'merge state above zero');
    assert(loser49.opacity < loser25.opacity, 'merge intensity grows');
    assert(!loser25.hidden && !loser49.hidden, 'still rendered below the retirement point');
    assert(loser50.hidden, 'gone at the retirement point, matching the schedule');
    assert(loser100.hidden, 'still gone at the endpoint');
    assert(loser49.translatePercent > loser25.translatePercent,
        'the loser keeps moving toward its collision destination');
});

test('a merging loser hides its control block so no stub sticks out', () => {
    const plan = defaultPlanBody();
    const cells = Array.from({ length: 16 }, fakeCell);

    // The loser slides under its winner and its block is wider than the
    // overlap, so it has to go as soon as the merge starts.
    view.applyToCells(cells, plan, 20);
    assert(columnOf(cells[2]).controls.style.visibility === 'hidden',
        'the merging loser hides its controls');
    assert(columnOf(cells[2]).style.visibility !== 'hidden',
        'but the loser card still travels');
    assert(columnOf(cells[1]).controls.style.visibility === '',
        'a survivor keeps its controls');
    assert(columnOf(cells[0]).controls.style.visibility === '',
        'an anchor keeps its controls');

    // Past retirement the whole column goes, controls with it.
    view.applyToCells(cells, plan, view.MORPH_RETIREMENT_PERCENT + 10);
    assert(columnOf(cells[2]).style.visibility === 'hidden', 'the retired column is hidden');
    assert(columnOf(cells[2]).controls.style.visibility === 'hidden', 'controls hidden too');

    // Back at zero every block is visible again.
    view.applyToCells(cells, plan, 0);
    assert(cells.every((cell) => columnOf(cell).controls.style.visibility === ''),
        'canonical view restores every control block');
});

test('the winner absorbing a retired loser stretches then returns to square', () => {
    const plan = defaultPlanBody();
    const at = (amount) => view.absorbedStretches(plan, amount).get(3);

    assert(view.absorbedStretches(plan, view.MORPH_RETIREMENT_PERCENT - 1).size === 0,
        'nothing is absorbed while the loser still sounds');

    const start = at(view.MORPH_RETIREMENT_PERCENT);
    const mid = at(85);
    const end = at(99);
    assert(start.extra > 0, `winner stretches at the retirement point, got ${start.extra}`);
    assert(mid.extra < start.extra, 'the stretch shrinks as the sweep continues');
    assert(end.extra < mid.extra, 'and keeps shrinking');
    assert(end.extra < 0.05, `back to square by the endpoint, got ${end.extra}`);

    // This plan's loser sits ahead of its winner, so the card grows back
    // over the slot it swallowed and its right edge never moves.
    assert(start.backwards === true, 'a forward collision grows the card backwards');
});

test('a loser that collides backwards stretches its winner forwards', () => {
    // S2 is the loser here and collides back onto S1's target, so the
    // winner sits behind the slot it swallows.
    const plan = defaultPlanBody();
    for (const assignment of plan.assignments) {
        const local = assignment.step % 4;
        if (local === 1) {
            assignment.survivor = false;
            assignment.targetOffset = { num: 0, den: 1 };
        }
        if (local === 2) assignment.survivor = true;
    }
    const stretches = view.absorbedStretches(plan, 85);
    const winner = stretches.get(0);
    assert(winner, 'the anchor absorbs the loser that collided onto it');
    assert(winner.extra > 0, 'it stretches');
    assert(winner.backwards === false, 'and it grows forwards, over the slot ahead of it');
});

test('only the absorbing note card is widened, never its controls', () => {
    const plan = defaultPlanBody();
    const cells = Array.from({ length: 16 }, fakeCell);
    view.applyToCells(cells, plan, view.MORPH_RETIREMENT_PERCENT);

    const winner = cells[3];
    assert(winner.style.width !== '' && winner.style.width !== undefined,
        `winner card is widened, got ${winner.style.width}`);
    assert(winner.dataset.morphAbsorbed === 'backwards', 'absorbed marker records the direction');
    // The right edge must not move: the extra width is pulled back by an
    // equal negative margin.
    const extra = Number.parseFloat(winner.style.width) - 100;
    assert(Math.abs(Number.parseFloat(winner.style.marginLeft) + extra) < 1e-6,
        `margin ${winner.style.marginLeft} should cancel ${extra}% of width`);
    assert(columnOf(winner).style.width === undefined || columnOf(winner).style.width === '',
        'the column keeps its grid width');
    assert(columnOf(winner).controls.style.width === undefined
        || columnOf(winner).controls.style.width === '',
        'the UP/DN/SL/AC block stays one step wide');
    assert(cells[1].style.width === undefined || cells[1].style.width === '',
        'an ordinary survivor is not widened');

    view.applyToCells(cells, plan, 0);
    assert(cells.every((cell) => (cell.style.width === undefined || cell.style.width === '')
        && (cell.style.marginLeft === undefined || cell.style.marginLeft === '')
        && !('morphAbsorbed' in cell.dataset)), 'width and margin cleared at 0');
});

test('applying cells keeps 16 identities below 100 and 12 at the endpoint', () => {
    const plan = defaultPlanBody();
    const cells = Array.from({ length: 16 }, fakeCell);
    view.applyToCells(cells, plan, 40);
    assert(cells.every(cell => columnOf(cell).style.transform.startsWith('translateX(')),
        'every source column keeps a positioned identity below 100');
    assert(cells.every(cell => columnOf(cell).style.visibility === ''), 'none hidden at 40');
    assert(cells[2].dataset.morphRole === 'loser', 'loser exposed for accessibility');
    assert(cells[2].dataset.morphMerging === 'true', 'merge flag set');

    view.applyToCells(cells, plan, 100);
    const visible = cells.filter(cell => columnOf(cell).style.visibility !== 'hidden');
    assert(visible.length === 12, 'exactly 12 target columns at the endpoint');
    assert(columnOf(cells[2]).style.visibility === 'hidden', 'beat 0 loser hidden');
});

test('the control block travels with its note card', () => {
    const plan = defaultPlanBody();
    const cells = Array.from({ length: 16 }, fakeCell);
    view.applyToCells(cells, plan, 50);

    // Geometry lands on the column, so the note card and its UP/DN/SL/AC
    // block share one transform instead of drifting apart.
    const survivor = cells[1];
    assert(columnOf(survivor).style.transform === 'translateX(16.667%)',
        'the column carries the displacement');
    assert(survivor.style.transform === undefined || survivor.style.transform === '',
        'the note card is not moved on its own');
    assert(columnOf(survivor).controls.style.transform === undefined
        || columnOf(survivor).controls.style.transform === '',
        'the control block is not moved on its own');

    // At the endpoint a removed loser takes its controls with it.
    view.applyToCells(cells, plan, 100);
    const loser = cells[2];
    assert(columnOf(loser).style.visibility === 'hidden',
        'the whole losing column is hidden, controls included');
    assert(loser.style.visibility === undefined || loser.style.visibility === '',
        'the note card is not hidden on its own');
});

test('returning to zero clears every derived style and attribute', () => {
    const plan = defaultPlanBody();
    const cells = Array.from({ length: 16 }, fakeCell);
    view.applyToCells(cells, plan, 80);
    view.applyToCells(cells, plan, 0);
    assert(cells.every(cell => columnOf(cell).style.transform === ''
        && columnOf(cell).style.opacity === ''
        && columnOf(cell).style.visibility === ''
        && !('morphRole' in cell.dataset)
        && !('morphMerging' in cell.dataset)), 'canonical view restored at 0');
});

test('rendering never mutates the canonical pattern or the plan', () => {
    const plan = defaultPlanBody();
    const planSnapshot = JSON.stringify(plan);
    const cells = Array.from({ length: 16 }, fakeCell);
    reset();
    const patternSnapshot = canonicalPatternText(state.getPattern(0));
    view.applyToCells(cells, plan, 65);
    assert(JSON.stringify(plan) === planSnapshot, 'plan untouched');
    assert(canonicalPatternText(state.getPattern(0)) === patternSnapshot,
        'canonical pattern untouched');
});

await asyncTest('a stale plan response is ignored after the session resets', async () => {
    reset();
    state.setTripletMorphPercent(50);
    view.ensurePlans();
    assert(pendingFetches.length === 1, 'one plan request in flight');
    const request = pendingFetches.shift();
    assert(request.url === '/api/pattern/triplet-morph/plan', 'plan endpoint used');

    state.resetTripletMorphSession();
    request.resolve({ eligible: true, plan: defaultPlanBody() });
    await new Promise((resolve) => { setTimeout(resolve, 0); });
    assert(state.getTripletMorphSession() === null, 'session stays closed');

    state.setTripletMorphPercent(30);
    const text = canonicalPatternText(state.getPattern(0));
    assert(state.getTripletMorphPlan(text) === null,
        'the stale response never entered the fresh session');
    state.resetTripletMorphSession();
});

await asyncTest('a fresh plan response is cached under its canonical text', async () => {
    reset();
    state.setTripletMorphPercent(50);
    view.ensurePlans();
    assert(pendingFetches.length === 1, 'one plan request in flight');
    const request = pendingFetches.shift();
    request.resolve({ eligible: true, plan: defaultPlanBody() });
    await new Promise((resolve) => { setTimeout(resolve, 0); });
    const text = canonicalPatternText(state.getPattern(0));
    assert(state.getTripletMorphPlan(text)?.planVersion === 1, 'plan cached');
    state.resetTripletMorphSession();
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
