// Usage: node ui/js/shared/steps-txt-format.test.js
//
// Verifies the JS renderer matches the Rust exporter byte-for-byte for the
// canonical fixtures in tests/fixtures/*.steps.txt. If these drift, the
// user's system-clipboard output no longer round-trips back through the
// backend importer, which is the whole point of the feature.

import { readFileSync } from 'node:fs';

import { formatPatternAsStepsTxt } from './steps-txt-format.js';
import { parseStepsTxtDocument } from './steps-txt-parse.js';

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

function stepRows(text) {
    return text.split('\n').filter(line => /^\d{2} /.test(line));
}

console.log('steps-txt-format tests:');

test('default pattern renders factory-default body', () => {
    const out = formatPatternAsStepsTxt(defPattern(), 120);
    assert(out.startsWith('format=td3-stepdsl-v1.1\n'), 'format header');
    assert(out.includes('pattern_co_lane=off\npattern_gt_lane=off\n'), 'lanes off by default');
    assert(out.includes('triplet_morph=off\ntriplet_morph_percentage=0\n'), 'morph off by default');
    assert(out.includes('live_update=off\n'), 'live off by default');
    assert(out.includes('active_steps=16\n'), 'active_steps header');
    assert(out.includes('triplet_time=off\n'), 'triplet_time=off');
    assert(out.includes('bpm=120\n'), 'integer BPM');
    // Step 1 with bare C note pads right to ' C'
    assert(out.includes('01  C:---:N|CO:64|GT:50\n'), 'step 01 default row');
    assert(out.includes('16  C:---:N|CO:64|GT:50\n'), 'step 16 default row');
    assert(out.endsWith('# Live Update | live_update: on/off\n'), 'trailing legend');
});

test('triplet flag renders on', () => {
    const p = defPattern();
    p.triplet = true;
    const out = formatPatternAsStepsTxt(p, 120);
    assert(out.includes('triplet_time=on\n'), 'triplet_time=on');
});

test('active_steps non-default renders', () => {
    const p = defPattern();
    p.active_steps = 3;
    const out = formatPatternAsStepsTxt(p, 120);
    assert(out.includes('active_steps=3\n'), 'active_steps=3');
    assert(stepRows(out).length === 3, 'only three active rows emitted');
    assert(!out.includes('04  C:---:N'), 'inactive row 4 omitted');
});

test('all flag/note combinations render correctly', () => {
    // Mirrors the layout of tests/fixtures/all_features.steps.txt so the JS
    // output matches the Rust exporter byte-for-byte.
    const rows = [
        { note: 'C',  transpose: 'DOWN',   accent: true,  slide: false, time: 'NORMAL'   },
        { note: 'C#', transpose: 'NORMAL', accent: false, slide: false, time: 'TIE'      },
        { note: 'D',  transpose: 'NORMAL', accent: true,  slide: false, time: 'REST'     },
        { note: 'D#', transpose: 'DOWN',   accent: false, slide: false, time: 'NORMAL'   },
        { note: 'E',  transpose: 'NORMAL', accent: true,  slide: false, time: 'TIE'      },
        { note: 'F',  transpose: 'NORMAL', accent: false, slide: true,  time: 'REST'     },
        { note: 'F#', transpose: 'DOWN',   accent: true,  slide: false, time: 'NORMAL'   },
        { note: 'G',  transpose: 'NORMAL', accent: false, slide: true,  time: 'TIE'      },
        { note: 'G#', transpose: 'NORMAL', accent: true,  slide: false, time: 'REST'     },
        { note: 'A',  transpose: 'DOWN',   accent: false, slide: false, time: 'NORMAL'   },
        { note: 'A#', transpose: 'NORMAL', accent: true,  slide: false, time: 'TIE'      },
        { note: 'B',  transpose: 'NORMAL', accent: false, slide: true,  time: 'REST'     },
        { note: 'C^', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL'   },
        { note: 'C',  transpose: 'DOWN',   accent: false, slide: true,  time: 'TIE'      },
        { note: 'D',  transpose: 'NORMAL', accent: true,  slide: false, time: 'REST'     },
        { note: 'E',  transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL'   },
    ];
    const p = { active_steps: 16, triplet: true, steps: rows };
    const out = formatPatternAsStepsTxt(p, 120);
    const expected =
        'format=td3-stepdsl-v1.1\n' +
        'active_steps=16\n' +
        'triplet_time=on\n' +
        'triplet_morph=off\n' +
        'triplet_morph_percentage=0\n' +
        'bpm=120\n' +
        'live_update=off\n' +
        'pattern_co_lane=off\n' +
        'pattern_gt_lane=off\n' +
        '\n' +
        '01  C:DA-:N|CO:64|GT:50\n' +
        '02 C#:---:T|CO:64|GT:50\n' +
        '03  D:-A-:R|CO:64|GT:50\n' +
        '04 D#:D--:N|CO:64|GT:50\n' +
        '05  E:-A-:T|CO:64|GT:50\n' +
        '06  F:--S:R|CO:64|GT:50\n' +
        '07 F#:DA-:N|CO:64|GT:50\n' +
        '08  G:--S:T|CO:64|GT:50\n' +
        '09 G#:-A-:R|CO:64|GT:50\n' +
        '10  A:D--:N|CO:64|GT:50\n' +
        '11 A#:-A-:T|CO:64|GT:50\n' +
        '12  B:--S:R|CO:64|GT:50\n' +
        '13 C^:---:N|CO:64|GT:50\n' +
        '14  C:D-S:T|CO:64|GT:50\n' +
        '15  D:-A-:R|CO:64|GT:50\n' +
        '16  E:---:N|CO:64|GT:50\n' +
        '\n' +
        '# NOTE:TAS:TIME|CO:cutoff|GT:gate\n' +
        '# transpose: U|D|-\n' +
        '# accent: A|-\n' +
        '# slide: S|-\n' +
        '# time: N|T|R|TR\n' +
        '# Cutoff Control | CO:0-127\n' +
        '# Gate Control | GT:1-100\n' +
        '# Lanes | pattern_co_lane, pattern_gt_lane: on/off\n' +
        '# Live Update | live_update: on/off\n';
    assert(out === expected, `byte-for-byte match\n---got---\n${out}\n---want---\n${expected}\n`);
});

test('UP transpose and TIE_REST time render', () => {
    const p = defPattern();
    p.steps[0] = { note: 'G', transpose: 'UP', accent: true, slide: true, time: 'TIE_REST' };
    const out = formatPatternAsStepsTxt(p, 120);
    assert(out.includes('01  G:UAS:TR|CO:64|GT:50\n'), 'up/accent/slide/tie-rest row');
});

test('throws on missing steps array', () => {
    let threw = false;
    try { formatPatternAsStepsTxt({ active_steps: 16, triplet: false }, 120); }
    catch (_) { threw = true; }
    assert(threw, 'no-steps pattern should throw');
});

test('throws on wrong step count', () => {
    let threw = false;
    try { formatPatternAsStepsTxt({ active_steps: 16, triplet: false, steps: [defStep()] }, 120); }
    catch (_) { threw = true; }
    assert(threw, '1-step pattern should throw');
});

test('fractional BPM preserves canonical centibpm precision', () => {
    const precise = formatPatternAsStepsTxt(defPattern(), 128.37);
    assert(precise.includes('bpm=128.37\n'), 'two decimals preserved');
    const oneDigit = formatPatternAsStepsTxt(defPattern(), 128.3);
    assert(oneDigit.includes('bpm=128.3\n'), 'trailing zero omitted');
});

test('active_steps=16 emits exactly 16 rows', () => {
    const out = formatPatternAsStepsTxt(defPattern(), 120);
    assert(stepRows(out).length === 16, 'all 16 active rows emitted');
});

test('invalid BPM is rejected', () => {
    for (const bpm of [19.99, 300.01, 128.371, NaN, Infinity, '128']) {
        let threw = false;
        try { formatPatternAsStepsTxt(defPattern(), bpm); } catch (_) { threw = true; }
        assert(threw, `${String(bpm)} rejected`);
    }
});

test('v1.1 fixture parse-format-parse is idempotent', () => {
    const first = parseStepsTxtDocument(V11_FIXTURE);
    const formatted = formatPatternAsStepsTxt(first.pattern, first.centibpm / 100);
    const second = parseStepsTxtDocument(formatted);
    assert(second.centibpm === first.centibpm, 'BPM preserved');
    assert(JSON.stringify(second.pattern) === JSON.stringify(first.pattern), 'pattern preserved');
    assert(stepRows(formatted).length === 3, 'short form preserved');
    assert(formatted.startsWith('format=td3-stepdsl-v1.1\n'), 'a v1 fixture re-renders as v1.1');
});

test('meta renders lanes, switches, morph and live; values clamp', () => {
    const p = defPattern();
    p.active_steps = 2;
    const out = formatPatternAsStepsTxt(p, 120, {
        stepCutoffs: [0, 200],
        stepGates: [128, 0],
        cutoffLaneOn: true,
        gateLaneOn: false,
        tripletMorphPercent: 69,
        liveUpdate: true,
    });
    assert(out.includes('triplet_morph=on\ntriplet_morph_percentage=69\n'), 'morph on');
    assert(out.includes('live_update=on\n'), 'live on');
    assert(out.includes('pattern_co_lane=on\npattern_gt_lane=off\n'), 'switches');
    assert(out.includes('01  C:---:N|CO:0|GT:100\n'), 'row 1 clamps GT 128 to 100');
    assert(out.includes('02  C:---:N|CO:127|GT:1\n'), 'row 2 clamps CO 200 and GT 0');
    const zero = formatPatternAsStepsTxt(p, 120, { tripletMorphPercent: 0 });
    assert(zero.includes('triplet_morph=off\ntriplet_morph_percentage=0\n'), 'amount 0 is off');
});

test('v1.1 lanes fixture renders back byte-for-byte from its parsed meta', () => {
    const fixture = readFileSync(
        new URL('../../../tests/fixtures/stepsdsl_v1_1_lanes.steps.txt', import.meta.url),
        'utf8',
    ).replaceAll(String.fromCharCode(13), '');
    const doc = parseStepsTxtDocument(fixture);
    const out = formatPatternAsStepsTxt(doc.pattern, doc.centibpm / 100, doc.meta);
    assert(out === fixture, `fixture is canonical output\n---got---\n${out}\n---want---\n${fixture}`);
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
