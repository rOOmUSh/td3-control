import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clamp } from './math.js';

test('clamp returns values inside the range unchanged', () => {
    assert.equal(clamp(5, 1, 9), 5);
});

test('clamp limits values below the range', () => {
    assert.equal(clamp(-3, 0, 12), 0);
});

test('clamp limits values above the range', () => {
    assert.equal(clamp(20, 0, 12), 12);
});

test('clamp preserves existing NaN behavior', () => {
    assert.ok(Number.isNaN(clamp(Number.NaN, 0, 12)));
});
