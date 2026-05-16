// Usage: node ui/js/shared/steps-txt-parse.test.js
//
// Exercises the UI-side .steps.txt parser. The parser mirrors
// `src/formats/steps_txt.rs::import` and feeds the PASTE FULL / Ctrl+V
// path on the main Control page, so regressions here silently corrupt
// patterns the user pastes from Notepad/WhatsApp.
//
// Round-trip coverage pairs this with steps-txt-format.test.js: format
// then parse, parse then format - the pattern must be stable.

import { formatPatternAsStepsTxt } from './steps-txt-format.js';
import { parseStepsTxt, looksLikeStepsTxt } from './steps-txt-parse.js';

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

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (err) {
        console.error(`  FAIL: ${name}: ${err.stack || err.message}`);
        failed++;
    }
}

function defStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' };
}
function defPattern() {
    return { active_steps: 16, triplet: false, steps: Array.from({ length: 16 }, defStep) };
}

const FIXTURE_ALL =
    'format=td3-stepdsl-v1\n' +
    'active_steps=16\n' +
    'triplet_time=on\n' +
    '\n' +
    '01  C:DA-:N\n' +
    '02 C#:---:T\n' +
    '03  D:-A-:R\n' +
    '04 D#:D--:N\n' +
    '05  E:-A-:T\n' +
    '06  F:--S:R\n' +
    '07 F#:DA-:N\n' +
    '08  G:--S:T\n' +
    '09 G#:-A-:R\n' +
    '10  A:D--:N\n' +
    '11 A#:-A-:T\n' +
    '12  B:--S:R\n' +
    '13 C^:---:N\n' +
    '14  C:D-S:T\n' +
    '15  D:-A-:R\n' +
    '16  E:---:N\n' +
    '\n' +
    '# NOTE:TAS:TIME\n' +
    '# transpose: U|D|-\n' +
    '# accent: A|-\n' +
    '# slide: S|-\n' +
    '# time: N|T|R|TR\n';

console.log('steps-txt-parse tests:');

// --- Header detector ------------------------------------------------------

test('looksLikeStepsTxt detects header', () => {
    assert(looksLikeStepsTxt(FIXTURE_ALL), 'fixture matches');
    assert(!looksLikeStepsTxt('hello world'), 'random text no match');
    assert(!looksLikeStepsTxt(''), 'empty no match');
    assert(!looksLikeStepsTxt(null), 'null no match');
    assert(!looksLikeStepsTxt(undefined), 'undefined no match');
});

// --- Happy path -----------------------------------------------------------

test('parses full-feature fixture', () => {
    const p = parseStepsTxt(FIXTURE_ALL);
    assert(p.active_steps === 16, 'active_steps');
    assert(p.triplet === true, 'triplet on');
    assert(p.steps.length === 16, '16 steps');
    assert(p.steps[0].note === 'C' && p.steps[0].transpose === 'DOWN'
        && p.steps[0].accent === true && p.steps[0].slide === false
        && p.steps[0].time === 'NORMAL', 'step 1');
    assert(p.steps[12].note === 'C^' && p.steps[12].time === 'NORMAL', 'step 13 C^');
    assert(p.steps[5].slide === true && p.steps[5].time === 'REST', 'step 6 rest+slide');
    assert(p.steps[13].slide === true && p.steps[13].transpose === 'DOWN', 'step 14 D+slide');
});

test('parses minimal default pattern', () => {
    const text = formatPatternAsStepsTxt(defPattern());
    const p = parseStepsTxt(text);
    assert(p.active_steps === 16 && p.triplet === false, 'defaults preserved');
    for (let i = 0; i < 16; i++) {
        const s = p.steps[i];
        assert(s.note === 'C' && s.transpose === 'NORMAL' && !s.accent
            && !s.slide && s.time === 'NORMAL', `step ${i+1} default`);
    }
});

test('parses active_steps non-default', () => {
    const p = defPattern();
    p.active_steps = 7;
    const p2 = parseStepsTxt(formatPatternAsStepsTxt(p));
    assert(p2.active_steps === 7, 'active_steps=7');
});

test('TIE_REST and UP parse correctly', () => {
    const p = defPattern();
    p.steps[0] = { note: 'G', transpose: 'UP', accent: true, slide: true, time: 'TIE_REST' };
    const back = parseStepsTxt(formatPatternAsStepsTxt(p));
    const s = back.steps[0];
    assert(s.note === 'G' && s.transpose === 'UP' && s.accent && s.slide && s.time === 'TIE_REST',
        'TIE_REST + UP + accent + slide round-trips');
});

test('ignores comment and blank lines freely', () => {
    const text =
        '# prelude\n' +
        'format=td3-stepdsl-v1\n' +
        '\n' +
        '# settings\n' +
        'active_steps=16\n' +
        'triplet_time=off\n' +
        '\n';
    // Append 16 default steps
    let body = text;
    for (let i = 1; i <= 16; i++) {
        body += `${i < 10 ? '0' + i : i}  C:---:N\n`;
    }
    body += '# trailing\n';
    const p = parseStepsTxt(body);
    assert(p.active_steps === 16, 'parsed with comments');
});

test('CRLF line endings parse as well', () => {
    const text = FIXTURE_ALL.replace(/\n/g, '\r\n');
    const p = parseStepsTxt(text);
    assert(p.steps[0].transpose === 'DOWN', 'CRLF fixture parses');
});

// --- Round-trip -----------------------------------------------------------

test('format → parse → format is stable', () => {
    const p = defPattern();
    p.triplet = true;
    p.active_steps = 12;
    p.steps[3] = { note: 'A#', transpose: 'DOWN', accent: true, slide: false, time: 'TIE' };
    p.steps[9] = { note: 'C^', transpose: 'UP', accent: false, slide: true, time: 'REST' };

    const text1 = formatPatternAsStepsTxt(p);
    const p2 = parseStepsTxt(text1);
    const text2 = formatPatternAsStepsTxt(p2);
    assert(text1 === text2, 'round-trip is idempotent');
});

// --- Negative cases -------------------------------------------------------

test('rejects unknown format header', () => {
    const t = FIXTURE_ALL.replace('format=td3-stepdsl-v1', 'format=td3-stepdsl-v99');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, 'v99 rejected');
});

test('rejects invalid active_steps', () => {
    const t = FIXTURE_ALL.replace('active_steps=16', 'active_steps=abc');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, 'non-numeric active_steps rejected');
});

test('rejects out-of-range active_steps', () => {
    const t = FIXTURE_ALL.replace('active_steps=16', 'active_steps=99');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, 'active_steps=99 rejected');
});

test('rejects missing step', () => {
    // Drop step 08.
    const t = FIXTURE_ALL.replace('08  G:--S:T\n', '');
    let threw = false;
    try { parseStepsTxt(t); } catch (err) { threw = /missing/.test(err.message); }
    assert(threw, 'missing step reported');
});

test('rejects bad TAS width', () => {
    const t = FIXTURE_ALL.replace('01  C:DA-:N', '01  C:DA:N');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, '2-char TAS rejected');
});

test('rejects invalid transpose char', () => {
    const t = FIXTURE_ALL.replace('01  C:DA-:N', '01  C:XA-:N');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, 'transpose=X rejected');
});

test('rejects invalid time code', () => {
    const t = FIXTURE_ALL.replace('01  C:DA-:N', '01  C:DA-:Z');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, 'time=Z rejected');
});

test('rejects unknown note name', () => {
    const t = FIXTURE_ALL.replace('01  C:DA-:N', '01  H:DA-:N');
    let threw = false;
    try { parseStepsTxt(t); } catch (_) { threw = true; }
    assert(threw, 'note=H rejected');
});

test('rejects non-string input', () => {
    let threw = false;
    try { parseStepsTxt(null); } catch (_) { threw = true; }
    assert(threw, 'null rejected');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
