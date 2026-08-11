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
    assert(out.startsWith('format=td3-stepdsl-v1\n'), 'format header');
    assert(out.includes('active_steps=16\n'), 'active_steps header');
    assert(out.includes('triplet_time=off\n'), 'triplet_time=off');
    assert(out.includes('bpm=120\n'), 'integer BPM');
    // Step 1 with bare C note pads right to ' C'
    assert(out.includes('01  C:---:N\n'), 'step 01 default row');
    assert(out.includes('16  C:---:N\n'), 'step 16 default row');
    assert(out.endsWith('# time: N|T|R|TR\n'), 'trailing legend');
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
    assert(!out.includes('04  C:---:N\n'), 'inactive row 4 omitted');
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
        'format=td3-stepdsl-v1\n' +
        'active_steps=16\n' +
        'triplet_time=on\n' +
        'bpm=120\n' +
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
    assert(out === expected, `byte-for-byte match\n---got---\n${out}\n---want---\n${expected}\n`);
});

test('UP transpose and TIE_REST time render', () => {
    const p = defPattern();
    p.steps[0] = { note: 'G', transpose: 'UP', accent: true, slide: true, time: 'TIE_REST' };
    const out = formatPatternAsStepsTxt(p, 120);
    assert(out.includes('01  G:UAS:TR\n'), 'up/accent/slide/tie-rest row');
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
    assert(formatted === V11_FIXTURE, 'neutral fixture is canonical output');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
