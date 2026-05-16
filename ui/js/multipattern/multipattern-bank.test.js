// Usage: node ui/js/multipattern/multipattern-bank.test.js

import assert from 'node:assert/strict';
import { buildBankSaveEntries, readSidebarBankMetadata } from './multipattern-bank.js';
import { defaultPattern } from './pattern-default.js';

let passed = 0;

function test(name, fn) {
    fn();
    passed++;
    console.log(`  ok: ${name}`);
}

console.log('multipattern-bank tests:');

test('buildBankSaveEntries maps canvas indexes to dashed slot keys in ALTERNATE mode', () => {
    const patterns = [defaultPattern(), defaultPattern(), defaultPattern()];
    const entries = buildBankSaveEntries({
        patterns,
        indexes: [0, 2],
        scratch: null,
        mode: 'ALTERNATE',
        startSlot: null,
    });
    assert.equal(entries.length, 2);
    assert.equal(entries[0].slot_key, 'G1-P1A');
    assert.equal(entries[0].display_name, 'P1 G1P1A');
    assert.equal(entries[1].slot_key, 'G1-P2A');
    assert.equal(entries[1].display_name, 'P3 G1P2A');
    assert.equal(entries[0].pattern, patterns[0]);
    assert.equal(entries[1].pattern, patterns[2]);
});

test('buildBankSaveEntries respects scratch-excluded slot assignment', () => {
    const patterns = [defaultPattern(), defaultPattern(), defaultPattern()];
    const entries = buildBankSaveEntries({
        patterns,
        indexes: [0, 1, 2],
        scratch: { group: 1, pattern: 1, side: 'A', label: 'G1P1A' },
        mode: 'ALTERNATE',
        startSlot: null,
    });
    assert.deepEqual(entries.map((entry) => entry.slot_key), ['G1-P1B', 'G1-P2A', 'G1-P2B']);
});

test('buildBankSaveEntries skips invalid indexes', () => {
    const patterns = [defaultPattern()];
    const entries = buildBankSaveEntries({
        patterns,
        indexes: [-1, 0, 3],
        scratch: null,
        mode: 'ALTERNATE',
        startSlot: null,
    });
    assert.equal(entries.length, 1);
    assert.equal(entries[0].slot_key, 'G1-P1A');
});

test('readSidebarBankMetadata returns root label and scale id', () => {
    const doc = {
        getElementById(id) {
            if (id === 'root-select') return { value: '2' };
            if (id === 'scale-select') return { value: 'phrygian_dominant' };
            return null;
        },
    };
    assert.deepEqual(readSidebarBankMetadata(doc), {
        root_note: 'D',
        scale_name: 'phrygian_dominant',
    });
});

console.log(`\n${passed} passed, 0 failed`);
