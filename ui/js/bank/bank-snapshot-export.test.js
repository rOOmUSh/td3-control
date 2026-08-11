// Tests for the pure snapshot export request builder.

import assert from 'node:assert/strict';

import { buildSnapshotExportRequest } from './bank-snapshot-export.js';

let bpm = 128.37;
let reads = 0;
const getBpm = () => {
    reads++;
    return bpm;
};

const first = buildSnapshotExportRequest({
    slotKeys: ['G1-P1A'],
    formatIds: ['steps_txt'],
    targetDir: 'C:\\exports',
    getBpm,
});
assert.deepEqual(first, {
    slot_keys: ['G1-P1A'],
    formats: ['steps_txt'],
    target_dir: 'C:\\exports',
    centibpm: 12837,
});

bpm = 156;
const second = buildSnapshotExportRequest({
    slotKeys: ['G2-P3B'],
    formatIds: ['steps_txt', 'json'],
    targetDir: 'C:\\exports',
    getBpm,
});
assert.equal(second.centibpm, 15600);
assert.equal(reads, 2, 'the BPM provider is read for each export request');

assert.throws(
    () => buildSnapshotExportRequest({
        slotKeys: ['G1-P1A'],
        formatIds: ['steps_txt'],
        targetDir: 'C:\\exports',
        getBpm: () => 19.99,
    }),
    /BPM must be between 20 and 300/,
);

console.log('bank-snapshot-export tests passed');
