// Tests for progression-push.js - runs with Node.js
// Usage: node ui/js/progression/progression-push.test.js
//
// Covers the pure `computeTargets` helper only. The modal/API side is
// DOM-dependent and exercised by hand in the browser.

import { computeTargets } from './progression-push.js';

let passed = 0, failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; }
    else passed++;
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

console.log('progression-push computeTargets tests:');

// Scratch P2, selected P1 → skip P2 in the walk.
test('scratch=P2 selected=P1 produces P1,P3,P4,P5', () => {
    const targets = computeTargets(
        { group: 1, pattern: 1, side: 'A' },
        { group: 1, pattern: 2, side: 'A' },
    );
    const labels = targets.map((t) => t.label);
    assert(labels.length === 4, `expected 4 targets, got ${labels.length}`);
    assert(labels[0] === 'G1-P1A', `[0] expected G1-P1A got ${labels[0]}`);
    assert(labels[1] === 'G1-P3A', `[1] expected G1-P3A got ${labels[1]}`);
    assert(labels[2] === 'G1-P4A', `[2] expected G1-P4A got ${labels[2]}`);
    assert(labels[3] === 'G1-P5A', `[3] expected G1-P5A got ${labels[3]}`);
});

// Scratch P2, selected P3 → scratch is before the range, no skip.
test('scratch=P2 selected=P3 produces P3,P4,P5,P6 (no skip)', () => {
    const targets = computeTargets(
        { group: 1, pattern: 3, side: 'A' },
        { group: 1, pattern: 2, side: 'A' },
    );
    const labels = targets.map((t) => t.label);
    assert(labels[0] === 'G1-P3A', labels[0]);
    assert(labels[1] === 'G1-P4A', labels[1]);
    assert(labels[2] === 'G1-P5A', labels[2]);
    assert(labels[3] === 'G1-P6A', labels[3]);
});

// Scratch P2, selected P8 → walks 8→1→(skip 2)→3→4.
test('scratch=P2 selected=P8 wraps and skips scratch', () => {
    const targets = computeTargets(
        { group: 1, pattern: 8, side: 'A' },
        { group: 1, pattern: 2, side: 'A' },
    );
    const labels = targets.map((t) => t.label);
    assert(labels[0] === 'G1-P8A', labels[0]);
    assert(labels[1] === 'G1-P1A', labels[1]);
    assert(labels[2] === 'G1-P3A', labels[2]);
    assert(labels[3] === 'G1-P4A', labels[3]);
});

// Scratch on a different side - no skip on the selected side/group.
test('scratch on different side is never skipped', () => {
    const targets = computeTargets(
        { group: 1, pattern: 1, side: 'A' },
        { group: 1, pattern: 2, side: 'B' },
    );
    const labels = targets.map((t) => t.label);
    assert(labels[0] === 'G1-P1A', labels[0]);
    assert(labels[1] === 'G1-P2A', `scratch is on B, P2A must not skip, got ${labels[1]}`);
    assert(labels[2] === 'G1-P3A', labels[2]);
    assert(labels[3] === 'G1-P4A', labels[3]);
});

// Scratch on a different group - no skip on the selected group.
test('scratch on different group is never skipped', () => {
    const targets = computeTargets(
        { group: 1, pattern: 1, side: 'A' },
        { group: 2, pattern: 2, side: 'A' },
    );
    const labels = targets.map((t) => t.label);
    assert(labels[1] === 'G1-P2A', `scratch in G2, P2A must not skip, got ${labels[1]}`);
});

// Side/group are preserved on every target.
test('side B is preserved across wrap', () => {
    const targets = computeTargets(
        { group: 2, pattern: 7, side: 'B' },
        { group: 1, pattern: 1, side: 'A' },
    );
    const labels = targets.map((t) => t.label);
    assert(labels[0] === 'G2-P7B', labels[0]);
    assert(labels[1] === 'G2-P8B', labels[1]);
    assert(labels[2] === 'G2-P1B', labels[2]);
    assert(labels[3] === 'G2-P2B', labels[3]);
});

// Field shape - callers rely on {group, pattern, side, label}.
test('target shape contains group/pattern/side/label', () => {
    const t = computeTargets(
        { group: 1, pattern: 1, side: 'A' },
        { group: 1, pattern: 8, side: 'A' },
    )[0];
    assert(t.group === 1, 'group');
    assert(t.pattern === 1, 'pattern');
    assert(t.side === 'A', 'side');
    assert(typeof t.label === 'string' && t.label.length > 0, 'label');
});

// --- Summary ---

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
