// Usage: node ui/js/shared/remote-start-delay.test.js

import { cycleDurationMicros } from '../remote-sync-timing.js';
import {
    CLOCKS_PER_BAR,
    DEFAULT_DIVISION,
    DEFAULT_MS,
    DIVISIONS,
    MAX_MS,
    MODE_MS,
    MODE_MUSICAL,
    clockTickMicros,
    divisionById,
    normalizeDivision,
    normalizeMilliseconds,
    normalizeMode,
    startDelayMicros,
    startDelayMillis,
} from './remote-start-delay.js';

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

test('the divisions are fractions of a 96 clock bar', () => {
    assert(divisionById('1bar').clocks === CLOCKS_PER_BAR, 'a bar is 96 clocks');
    assert(divisionById('1/2').clocks === CLOCKS_PER_BAR / 2, 'half');
    assert(divisionById('1/4').clocks === CLOCKS_PER_BAR / 4, 'quarter');
    assert(divisionById('1/8').clocks === CLOCKS_PER_BAR / 8, 'eighth');
    assert(divisionById('2bars').clocks === CLOCKS_PER_BAR * 2, 'two bars');
    assert(divisionById('4bars').clocks === CLOCKS_PER_BAR * 4, 'four bars');
    assert(DIVISIONS.length === 6, 'six choices');
});

test('one bar matches the old full straight cycle at every tempo', () => {
    for (const bpm of [20, 90, 120, 128.5, 174, 300]) {
        const before = cycleDurationMicros(bpm, 16, false);
        const after = startDelayMicros({ mode: MODE_MUSICAL, division: '1bar', bpm });
        assert(after === before, `bpm ${bpm}: ${after} !== ${before}`);
    }
});

test('one bar stays fixed instead of following shorter pattern cycles', () => {
    const bar = startDelayMicros({ mode: MODE_MUSICAL, division: '1bar', bpm: 120 });
    assert(bar !== cycleDurationMicros(120, 8, false), 'short straight cycle differs');
    assert(bar !== cycleDurationMicros(120, 16, true), 'triplet cycle differs');
});

test('a bar is two seconds at 120 BPM, which is what was measured', () => {
    const micros = startDelayMicros({ mode: MODE_MUSICAL, division: '1bar', bpm: 120 });
    assert(Math.abs(micros - 2000000) < 100, `got ${micros}`);
    assert(startDelayMillis({ mode: MODE_MUSICAL, division: '1bar', bpm: 120 }) === 2000, 'millis');
});

test('the musical lead scales with tempo', () => {
    const slow = startDelayMicros({ mode: MODE_MUSICAL, division: '1bar', bpm: 60 });
    const fast = startDelayMicros({ mode: MODE_MUSICAL, division: '1bar', bpm: 240 });
    assert(slow > fast, 'a slower tempo means a longer bar');
    assert(Math.abs(slow / fast - 4) < 0.01, 'four times the tempo, a quarter of the wait');
});

test('the divisions are ordered and proportional at a fixed tempo', () => {
    const at = (division) => startDelayMicros({ mode: MODE_MUSICAL, division, bpm: 120 });
    assert(at('1/8') < at('1/4'), 'eighth under quarter');
    assert(at('1/4') < at('1/2'), 'quarter under half');
    assert(at('1/2') < at('1bar'), 'half under a bar');
    assert(at('1bar') < at('2bars'), 'a bar under two');
    assert(at('2bars') < at('4bars'), 'two bars under four');
    assert(at('4bars') === at('1bar') * 4, 'four bars is four times a bar');
});

test('millisecond mode ignores the tempo', () => {
    for (const bpm of [60, 120, 300]) {
        const micros = startDelayMicros({ mode: MODE_MS, milliseconds: 250, bpm });
        assert(micros === 250000, `bpm ${bpm} gave ${micros}`);
    }
});

test('a zero millisecond lead is allowed', () => {
    assert(startDelayMicros({ mode: MODE_MS, milliseconds: 0, bpm: 120 }) === 0, 'no lead');
});

test('normalizeMode falls back to musical', () => {
    assert(normalizeMode(MODE_MS) === MODE_MS, 'ms kept');
    assert(normalizeMode(MODE_MUSICAL) === MODE_MUSICAL, 'musical kept');
    assert(normalizeMode('nonsense') === MODE_MUSICAL, 'unknown');
    assert(normalizeMode(undefined) === MODE_MUSICAL, 'missing');
});

test('normalizeDivision falls back to one bar', () => {
    assert(normalizeDivision('1/4') === '1/4', 'known kept');
    assert(normalizeDivision('3bars') === DEFAULT_DIVISION, 'unknown');
    assert(normalizeDivision(undefined) === DEFAULT_DIVISION, 'missing');
});

// A bad value must not silently become a zero lead, which would start the
// devices at whatever moment each request happened to land.
test('normalizeMilliseconds refuses to turn junk into no lead', () => {
    assert(normalizeMilliseconds('abc') === DEFAULT_MS, 'non numeric');
    assert(normalizeMilliseconds(undefined) === DEFAULT_MS, 'missing');
    assert(normalizeMilliseconds(null) === DEFAULT_MS, 'null');
    assert(normalizeMilliseconds(NaN) === DEFAULT_MS, 'NaN');
    assert(normalizeMilliseconds(Infinity) === DEFAULT_MS, 'Infinity');
});

test('normalizeMilliseconds clamps to the accepted window', () => {
    assert(normalizeMilliseconds(-50) === 0, 'negative clamps to zero');
    assert(normalizeMilliseconds(MAX_MS + 1000) === MAX_MS, 'over the maximum');
    assert(normalizeMilliseconds('250') === 250, 'numeric string');
    assert(normalizeMilliseconds(250.6) === 251, 'rounded');
});

test('an unusable settings object still yields the default lead', () => {
    const fallback = startDelayMicros({ mode: 'nonsense', division: 'nonsense', bpm: 120 });
    assert(fallback === startDelayMicros({ mode: MODE_MUSICAL, division: '1bar', bpm: 120 }),
        'falls back to one bar');
    assert(startDelayMicros() > 0, 'no arguments still leads');
});

test('clockTickMicros matches the transport tick', () => {
    assert(clockTickMicros(120) === Math.floor(250000000 / 12000), '120 BPM');
    assert(clockTickMicros(0) === clockTickMicros(0.01), 'clamped at the bottom');
    assert(clockTickMicros(99999) === clockTickMicros(300), 'clamped at the top');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
