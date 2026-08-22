// Usage: node ui/js/shared/steps-txt-parse.test.js
//
// Exercises the UI-side .steps.txt parser. The parser mirrors
// `src/formats/steps_txt.rs::import` and feeds the PASTE FULL / Ctrl+V
// path on the main Control page, so regressions here silently corrupt
// patterns the user pastes from Notepad/WhatsApp.
//
// Round-trip coverage pairs this with steps-txt-format.test.js: format
// then parse, parse then format - the pattern must be stable.

import { readFileSync } from 'node:fs';

import { formatPatternAsStepsTxt } from './steps-txt-format.js';
import { parseStepsTxt, parseStepsTxtDocument, looksLikeStepsTxt } from './steps-txt-parse.js';

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

const V11_FIXTURE = readFileSync(
    new URL('../../../tests/fixtures/stepsdslv1_1.steps.txt', import.meta.url),
    'utf8',
).replaceAll(String.fromCharCode(13), '');

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
    const text = formatPatternAsStepsTxt(defPattern(), 120);
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
    const p2 = parseStepsTxt(formatPatternAsStepsTxt(p, 120));
    assert(p2.active_steps === 7, 'active_steps=7');
});

test('TIE_REST and UP parse correctly', () => {
    const p = defPattern();
    p.steps[0] = { note: 'G', transpose: 'UP', accent: true, slide: true, time: 'TIE_REST' };
    const back = parseStepsTxt(formatPatternAsStepsTxt(p, 120));
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

test('parses the v1.1 fixture as a short document with exact BPM', () => {
    const doc = parseStepsTxtDocument(V11_FIXTURE);
    assert(doc.centibpm === 12800, 'integer BPM becomes exact centibpm');
    assert(doc.pattern.active_steps === 3, 'active_steps=3');
    assert(doc.pattern.steps.length === 16, 'internal pattern keeps 16 steps');
    assert(doc.pattern.steps[0].note === 'G', 'fixture row 1 preserved');
    assert(doc.pattern.steps[1].transpose === 'DOWN', 'fixture row 2 preserved');
    assert(doc.pattern.steps[2].time === 'TIE', 'fixture row 3 preserved');
    for (let i = 3; i < 16; i++) {
        assert(JSON.stringify(doc.pattern.steps[i]) === JSON.stringify(defStep()), `step ${i + 1} defaulted`);
    }
});

test('legacy 16-row document preserves rows after active_steps', () => {
    const text = FIXTURE_ALL.replace('active_steps=16', 'active_steps=3');
    const doc = parseStepsTxtDocument(text);
    assert(doc.pattern.active_steps === 3, 'active_steps=3');
    assert(doc.pattern.steps[15].note === 'E', 'provided row 16 preserved');
});

test('missing row inside active range is rejected', () => {
    const text = V11_FIXTURE.replace('02  G:D--:N\n', '');
    let message = '';
    try { parseStepsTxtDocument(text); } catch (err) { message = err.message; }
    assert(message.includes('missing steps: [2]'), 'missing active row reported');
});

test('reads fractional BPM exactly and missing BPM as null', () => {
    const fractional = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=128.37'));
    assert(fractional.centibpm === 12837, '128.37 becomes 12837');
    const oneDigit = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=128.3'));
    assert(oneDigit.centibpm === 12830, '128.3 becomes 12830');
    const trailingZero = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=128.30'));
    assert(trailingZero.centibpm === 12830, '128.30 becomes 12830');
    const absent = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128\n', ''));
    assert(absent.centibpm === null, 'missing BPM returns null');
});

test('accepts BPM boundaries and an empty fractional suffix', () => {
    const minimum = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=20'));
    assert(minimum.centibpm === 2000, '20 BPM accepted');
    const maximum = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=300.00'));
    assert(maximum.centibpm === 30000, '300.00 BPM accepted');
    const emptyFraction = parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=128.'));
    assert(emptyFraction.centibpm === 12800, '128. accepted');
});

test('rejects malformed, out-of-range, and duplicate BPM fields', () => {
    for (const bpm of ['19.99', '300.01', '128.371', '+128', '1e2', 'NaN', ' 128']) {
        let threw = false;
        try { parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', `bpm=${bpm}`)); }
        catch (_) { threw = true; }
        assert(threw, `${bpm} rejected`);
    }
    let duplicate = false;
    try { parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=128\nbpm=129')); }
    catch (err) { duplicate = /duplicate bpm/.test(err.message); }
    assert(duplicate, 'duplicate BPM rejected');

    let trailingWhitespace = false;
    try { parseStepsTxtDocument(V11_FIXTURE.replace('bpm=128', 'bpm=128 ')); }
    catch (_) { trailingWhitespace = true; }
    assert(trailingWhitespace, 'trailing BPM whitespace rejected');
});

test('rejects duplicate step indexes', () => {
    const text = V11_FIXTURE.replace('02  G:D--:N', '01  G:D--:N');
    let threw = false;
    try { parseStepsTxtDocument(text); } catch (err) { threw = /duplicate step/.test(err.message); }
    assert(threw, 'duplicate row rejected');
});

test('rejects step indexes 00 and 17', () => {
    for (const index of ['00', '17']) {
        const text = V11_FIXTURE.replace('01  G:---:N', `${index}  G:---:N`);
        let threw = false;
        try { parseStepsTxtDocument(text); }
        catch (err) { threw = /step index out of range/.test(err.message); }
        assert(threw, `step ${index} rejected`);
    }
});

// --- Round-trip -----------------------------------------------------------

test('format → parse → format is stable', () => {
    const p = defPattern();
    p.triplet = true;
    p.active_steps = 12;
    p.steps[3] = { note: 'A#', transpose: 'DOWN', accent: true, slide: false, time: 'TIE' };
    p.steps[9] = { note: 'C^', transpose: 'UP', accent: false, slide: true, time: 'REST' };

    const text1 = formatPatternAsStepsTxt(p, 128.37);
    const p2 = parseStepsTxt(text1);
    const text2 = formatPatternAsStepsTxt(p2, 128.37);
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

const V11_HEADER = 'format=td3-stepdsl-v1.1\nactive_steps=4\ntriplet_time=off\nbpm=120\n\n';

test('v1 documents parse with empty meta', () => {
    const doc = parseStepsTxtDocument(V11_FIXTURE);
    assert(JSON.stringify(doc.meta) === '{}', `empty meta, got ${JSON.stringify(doc.meta)}`);
});

test('v1.1 lanes fixture reads every field', () => {
    const fixture = readFileSync(
        new URL('../../../tests/fixtures/stepsdsl_v1_1_lanes.steps.txt', import.meta.url),
        'utf8',
    );
    assert(looksLikeStepsTxt(fixture), 'v1.1 tag is detected');
    const doc = parseStepsTxtDocument(fixture);
    assert(doc.pattern.active_steps === 8, 'active steps');
    assert(doc.centibpm === 12450, 'bpm');
    assert(doc.meta.stepCutoffs.slice(0, 8).join(',') === '0,40,64,90,127,100,12,77', 'cutoff values');
    assert(doc.meta.stepCutoffs.slice(8).every((v) => v === 64), 'rows beyond active keep the default');
    assert(doc.meta.stepGates.every((v) => v === 50), 'gate values');
    assert(doc.meta.cutoffLaneOn === true && doc.meta.gateLaneOn === false, 'switches');
    assert(doc.meta.tripletMorphPercent === 40, 'morph');
    assert(doc.meta.liveUpdate === false, 'live');
});

test('v1.1 unknown header keys are ignored, v1 ones are still rejected', () => {
    const ok = parseStepsTxtDocument('format=td3-stepdsl-v1.1\nactive_steps=1\nmystery=1\n\n01  C:---:N\n');
    assert(ok.pattern.active_steps === 1, 'v1.1 ignores mystery key');
    let threw = false;
    try { parseStepsTxtDocument('format=td3-stepdsl-v1\nactive_steps=1\nmystery=1\n\n01  C:---:N\n'); }
    catch (_) { threw = true; }
    assert(threw, 'v1 rejects mystery key');
});

test('out-of-range lane values clamp, non-numeric or missing drop the lane', () => {
    const doc = parseStepsTxtDocument(V11_HEADER
        + '01  C:---:N|CO:200|GT:128\n02  C:---:N|CO:-5|GT:0\n03  C:---:N|CO:64|GT:50\n04  C:---:N|CO:64|GT:50\n');
    assert(doc.meta.stepCutoffs[0] === 127 && doc.meta.stepCutoffs[1] === 0, 'cutoff clamps');
    assert(doc.meta.stepGates[0] === 100 && doc.meta.stepGates[1] === 1, 'gate clamps');

    const bad = parseStepsTxtDocument(V11_HEADER
        + '01  C:---:N|CO:10|GT:x\n02  C:---:N|GT:50\n03  C:---:N|CO:30|GT:50\n04  C:---:N|CO:40|GT:50\n');
    assert(bad.meta.stepCutoffs === undefined, 'missing CO drops the cutoff lane');
    assert(bad.meta.stepGates === undefined, 'invalid GT drops the gate lane');
});

test('lane switch comes from the header key or the all-equal heuristic', () => {
    const heur = parseStepsTxtDocument(V11_HEADER
        + '01  C:---:N|CO:10|GT:30\n02  C:---:N|CO:10|GT:50\n03  C:---:N|CO:10|GT:50\n04  C:---:N|CO:10|GT:50\n');
    assert(heur.meta.cutoffLaneOn === false, 'all equal means off');
    assert(heur.meta.gateLaneOn === true, 'one differs means on');
    const explicit = parseStepsTxtDocument(
        'format=td3-stepdsl-v1.1\nactive_steps=2\npattern_co_lane=on\npattern_gt_lane=off\n\n'
        + '01  C:---:N|CO:10|GT:30\n02  C:---:N|CO:10|GT:60\n');
    assert(explicit.meta.cutoffLaneOn === true && explicit.meta.gateLaneOn === false, 'header keys win');
});

test('morph needs on plus a percentage; junk on/off values are absent', () => {
    const noPercent = parseStepsTxtDocument('format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=on\n\n01  C:---:N\n');
    assert(noPercent.meta.tripletMorphPercent === undefined, 'on without percentage is absent');
    const off = parseStepsTxtDocument('format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=off\ntriplet_morph_percentage=50\n\n01  C:---:N\n');
    assert(off.meta.tripletMorphPercent === undefined, 'off with percentage is absent');
    const junk = parseStepsTxtDocument('format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=maybe\nlive_update=yes\n\n01  C:---:N\n');
    assert(junk.meta.liveUpdate === undefined, 'unusable live value is absent');
    const clamped = parseStepsTxtDocument('format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=on\ntriplet_morph_percentage=250\n\n01  C:---:N\n');
    assert(clamped.meta.tripletMorphPercent === 100, 'percentage clamps to 100');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
