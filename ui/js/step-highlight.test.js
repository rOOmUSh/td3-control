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

test('applyStepHighlight adds only the semantic active class', () => {
    const card = makeCard(['step-card', 'bg-surface-container-highest', 'step-downbeat']);

    applyStepHighlight(card);

    assert(has(card, 'bg-surface-container-highest'), 'original bg preserved');
    assert(has(card, 'step-active'), 'step-active added');
    assert(!has(card, 'step-pulse'), 'pulse class not added');
    assert(!has(card, 'led-glow-green-bright'), 'extra glow class not added');
    assert(!has(card, 'bg-primary-fixed'), 'active background utility not added');
    assert(has(card, 'step-downbeat'), 'non-bg classes preserved');
    assert(card.dataset.origBg === undefined, 'no background bookkeeping stored');
});

test('restoreStepHighlight removes active state without changing the base card', () => {
    const card = makeCard(['step-card', 'step-downbeat', 'bg-surface-container-highest']);

    applyStepHighlight(card);
    restoreStepHighlight(card);

    assert(!has(card, 'step-active'), 'step-active removed');
    assert(has(card, 'bg-surface-container-highest'), 'original bg remains');
    assert(has(card, 'step-downbeat'), 'downbeat class preserved');
    assert(card.dataset.origBg === undefined, 'no background bookkeeping created');
});

test('apply and restore are idempotent', () => {
    const card = makeCard(['step-card', 'bg-surface-container-high', 'step-downbeat']);

    applyStepHighlight(card);
    applyStepHighlight(card);
    assert(has(card, 'step-active'), 'repeated apply leaves card active');
    assert(has(card, 'bg-surface-container-high'), 'repeated apply preserves base bg');

    restoreStepHighlight(card);
    restoreStepHighlight(card);

    assert(!has(card, 'step-active'), 'repeated restore leaves card inactive');
    assert(has(card, 'bg-surface-container-high'), 'repeated restore preserves base bg');
    assert(has(card, 'step-downbeat'), 'downbeat class preserved');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
