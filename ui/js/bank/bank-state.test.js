import {
    state,
    clearAllSelections,
    setImportBatchSelection,
    setSnapshotSelection,
    setVisibleItemSelection,
    toggleImportBatchSelection,
    toggleSnapshotSelection,
} from './bank-state.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`  FAIL: ${message}`);
        failed++;
        return;
    }
    passed++;
}

function test(name, fn) {
    try {
        resetState();
        fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed++;
    }
}

function resetState() {
    state.items = [{ item_id: 'a' }, { item_id: 'b' }, { item_id: 'c' }];
    state.snapshots = [{ snapshot_id: 's1' }, { snapshot_id: 's2' }];
    state.importBatches = [{ batch_id: 'i1' }, { batch_id: 'i2' }];
    state.selectedIds = new Set();
    state.selectedSnapshotIds = new Set();
    state.selectedImportBatchIds = new Set();
    state.selectedSnapshotSlots = new Set();
    state.lastSelectedIndex = -1;
}

console.log('bank-state tests:');

test('setVisibleItemSelection selects and clears visible items only', () => {
    state.selectedIds.add('outside');
    setVisibleItemSelection(['a', 'b'], true);
    assert(JSON.stringify(Array.from(state.selectedIds).sort()) === '["a","b","outside"]',
        'visible item selection added');
    setVisibleItemSelection(['a', 'b'], false);
    assert(JSON.stringify(Array.from(state.selectedIds)) === '["outside"]',
        'visible item selection cleared');
});

test('snapshot card selection can toggle and bulk-set ids', () => {
    toggleSnapshotSelection('s1');
    assert(state.selectedSnapshotIds.has('s1'), 'snapshot toggled on');
    toggleSnapshotSelection('s1');
    assert(!state.selectedSnapshotIds.has('s1'), 'snapshot toggled off');
    setSnapshotSelection(['s1', 's2'], true);
    assert(state.selectedSnapshotIds.size === 2, 'snapshot bulk selected');
    setSnapshotSelection(['s1'], false);
    assert(!state.selectedSnapshotIds.has('s1') && state.selectedSnapshotIds.has('s2'),
        'snapshot bulk cleared one id');
});

test('import batch selection can toggle and bulk-set ids', () => {
    toggleImportBatchSelection('i1');
    assert(state.selectedImportBatchIds.has('i1'), 'import batch toggled on');
    toggleImportBatchSelection('i1');
    assert(!state.selectedImportBatchIds.has('i1'), 'import batch toggled off');
    setImportBatchSelection(['i1', 'i2'], true);
    assert(state.selectedImportBatchIds.size === 2, 'import batch bulk selected');
    setImportBatchSelection(['i2'], false);
    assert(state.selectedImportBatchIds.has('i1') && !state.selectedImportBatchIds.has('i2'),
        'import batch bulk cleared one id');
});

test('clearAllSelections clears every bank selection set', () => {
    state.selectedIds.add('a');
    state.selectedSnapshotIds.add('s1');
    state.selectedImportBatchIds.add('i1');
    state.selectedSnapshotSlots.add('G1-P1A');
    clearAllSelections();
    assert(state.selectedIds.size === 0, 'items cleared');
    assert(state.selectedSnapshotIds.size === 0, 'snapshots cleared');
    assert(state.selectedImportBatchIds.size === 0, 'import batches cleared');
    assert(state.selectedSnapshotSlots.size === 0, 'snapshot slots cleared');
});

if (failed > 0) {
    console.error(`\nbank-state: ${failed} FAILED (${passed} passed)`);
    process.exit(1);
}

console.log(`\nbank-state: ${passed} passed`);
