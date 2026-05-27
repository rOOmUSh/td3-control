// Tests for history.js undo/redo cursor logic - runs with Node.js
// Usage: node ui/js/history.test.js
//
// IndexedDB is not available in Node, so we test the linear timeline
// logic with an in-memory simulation of the same algorithm.

// --- In-memory simulation of the IndexedDB undo/redo cursor ---

function createHistory() {
    let entries = []; // { id, state }
    let nextId = 1;
    let position = null;

    return {
        push(state) {
            // Discard entries after current position
            if (position !== null) {
                entries = entries.filter(e => e.id <= position);
            }
            const id = nextId++;
            entries.push({ id, state: JSON.parse(JSON.stringify(state)) });
            position = id;
        },

        undo() {
            if (position === null) return null;
            const idx = entries.findIndex(e => e.id === position);
            if (idx <= 0) return null; // at beginning
            position = entries[idx - 1].id;
            return entries[idx - 1].state;
        },

        redo() {
            const idx = position === null
                ? -1
                : entries.findIndex(e => e.id === position);
            if (idx >= entries.length - 1) return null; // at end
            position = entries[idx + 1].id;
            return entries[idx + 1].state;
        },

        getPosition() { return position; },
        getEntries() { return entries; },
    };
}

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

console.log('history undo/redo tests:');

test('push creates entries', () => {
    const h = createHistory();
    h.push({ pattern: 'A' });
    h.push({ pattern: 'B' });
    h.push({ pattern: 'C' });
    assert(h.getEntries().length === 3, 'should have 3 entries');
});

test('undo returns previous state', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    h.push({ v: 3 });
    const s = h.undo();
    assert(s.v === 2, `expected 2, got ${s.v}`);
});

test('undo twice returns two steps back', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    h.push({ v: 3 });
    h.undo(); // → 2
    const s = h.undo(); // → 1
    assert(s.v === 1, `expected 1, got ${s.v}`);
});

test('undo at beginning returns null', () => {
    const h = createHistory();
    h.push({ v: 1 });
    const s = h.undo();
    assert(s === null, 'should return null at beginning');
});

test('undo on empty returns null', () => {
    const h = createHistory();
    assert(h.undo() === null, 'empty undo is null');
});

test('redo returns next state', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    h.push({ v: 3 });
    h.undo(); // → 2
    const s = h.redo(); // → 3
    assert(s.v === 3, `expected 3, got ${s.v}`);
});

test('redo at end returns null', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    assert(h.redo() === null, 'should return null at end');
});

test('new push after undo discards forward entries', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    h.push({ v: 3 });
    h.undo(); // cursor at 2
    h.push({ v: 99 }); // should discard entry 3
    assert(h.getEntries().length === 3, 'should have 3 entries (1, 2, 99)');
    assert(h.getEntries()[2].state.v === 99, 'last entry should be 99');
    assert(h.redo() === null, 'no redo after new push');
});

test('linear timeline: undo 10 from 100, push new = 91 entries', () => {
    const h = createHistory();
    for (let i = 1; i <= 100; i++) h.push({ v: i });
    for (let i = 0; i < 10; i++) h.undo();
    // cursor at 90
    h.push({ v: 'new' });
    assert(h.getEntries().length === 91, `expected 91, got ${h.getEntries().length}`);
    assert(h.getEntries()[90].state.v === 'new', 'last entry is new');
});

test('undo then redo is idempotent', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    h.push({ v: 3 });
    const u = h.undo(); // → 2
    const r = h.redo(); // → 3
    assert(u.v === 2, 'undo gives 2');
    assert(r.v === 3, 'redo gives 3');
});

test('multiple undo/redo cycles', () => {
    const h = createHistory();
    h.push({ v: 'A' });
    h.push({ v: 'B' });
    h.push({ v: 'C' });

    assert(h.undo().v === 'B', 'undo 1');
    assert(h.undo().v === 'A', 'undo 2');
    assert(h.undo() === null, 'undo 3 = null');
    assert(h.redo().v === 'B', 'redo 1');
    assert(h.redo().v === 'C', 'redo 2');
    assert(h.redo() === null, 'redo 3 = null');
});

test('push after partial undo removes correct entries', () => {
    const h = createHistory();
    h.push({ v: 1 });
    h.push({ v: 2 });
    h.push({ v: 3 });
    h.push({ v: 4 });
    h.push({ v: 5 });

    h.undo(); // → 4
    h.undo(); // → 3
    h.push({ v: 6 }); // entries 4, 5 discarded

    const entries = h.getEntries();
    assert(entries.length === 4, `expected 4, got ${entries.length}`);
    assert(entries.map(e => e.state.v).join(',') === '1,2,3,6', 'entries should be 1,2,3,6');
});

test('deep copy: modifying source does not affect stored state', () => {
    const h = createHistory();
    const obj = { v: 1, arr: [1, 2, 3] };
    h.push(obj);
    obj.v = 999;
    obj.arr.push(4);
    h.undo(); // nothing to undo to (only 1 entry)
    // Push another and undo to get the first
    h.push({ v: 2 });
    const restored = h.undo();
    assert(restored.v === 1, 'stored value should be 1, not 999');
    assert(restored.arr.length === 3, 'stored array should have 3 items');
});

// --- Summary ---

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
