import { applyStepHighlight, restoreStepHighlight } from './step-highlight.js';

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) {
        console.error(`  FAIL: ${msg}`);
        failed++;
        return;
    }
    passed++;
}

function has(card, cls) {
    return card.classList.contains(cls);
}

function makeCard(classes = []) {
    const set = new Set(classes);

    return {
        dataset: {},
        classList: {
            add(...tokens) {
                tokens.forEach((token) => set.add(token));
            },
            remove(...tokens) {
                tokens.forEach((token) => set.delete(token));
            },
            contains(token) {
                return set.has(token);
            },
            [Symbol.iterator]() {
                return set.values();
            },
        },
    };
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (err) {
        console.error(`  FAIL: ${name}: ${err.stack || err.message}`);
        failed++;
    }
}

console.log('step-highlight tests:');

test('applyStepHighlight stores original backgrounds and adds active classes', () => {
    const card = makeCard(['step-card', 'bg-surface-container-highest', 'step-downbeat']);

    applyStepHighlight(card);

    assert(card.dataset.origBg === 'bg-surface-container-highest', 'original bg stored');
    assert(!has(card, 'bg-surface-container-highest'), 'original bg removed');
    assert(has(card, 'bg-primary-fixed'), 'active bg added');
    assert(has(card, 'step-active'), 'step-active added');
    assert(has(card, 'step-downbeat'), 'non-bg classes preserved');
});

test('restoreStepHighlight removes active bg and restores original background', () => {
    const card = makeCard(['step-card', 'step-downbeat', 'bg-surface-container-highest']);

    applyStepHighlight(card);
    restoreStepHighlight(card);

    assert(!has(card, 'step-active'), 'step-active removed');
    assert(!has(card, 'step-pulse'), 'step-pulse removed');
    assert(!has(card, 'led-glow-green-bright'), 'glow removed');
    assert(!has(card, 'bg-primary-fixed'), 'active bg removed');
    assert(has(card, 'bg-surface-container-highest'), 'original bg restored');
    assert(has(card, 'step-downbeat'), 'downbeat class preserved');
    assert(card.dataset.origBg === undefined, 'origBg cleared after restore');
});

test('restoreStepHighlight clears stale active bg even when no origBg is stored', () => {
    const card = makeCard(['step-card', 'bg-primary-fixed', 'step-active', 'step-downbeat']);

    restoreStepHighlight(card);

    assert(!has(card, 'bg-primary-fixed'), 'stale active bg removed');
    assert(!has(card, 'step-active'), 'stale active class removed');
    assert(has(card, 'step-downbeat'), 'downbeat class preserved');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
