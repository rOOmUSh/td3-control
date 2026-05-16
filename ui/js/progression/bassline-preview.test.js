// Tests for bassline-preview.js - pure / mock-AudioContext coverage.
// Node-runnable: `node ui/js/progression/bassline-preview.test.js`.

import {
    stepToPreviewMidi,
    midiToHz,
    stepDurationSec,
    buildNoteEvents,
    previewBassline,
    stopPreview,
    isPreviewing,
    setPreviewGain,
    getPreviewGain,
    __setAudioContextFactory,
    __resetForTests,
} from './bassline-preview.js';

// ---------- tiny test harness ----------

let passed = 0;
let failed = 0;
const failures = [];

function test(name, fn) {
    try {
        fn();
        passed++;
        console.log(`  ok: ${name}`);
    } catch (err) {
        failed++;
        failures.push({ name, err });
        console.log(`  FAIL: ${name}`);
        console.log(`    ${err && err.message ? err.message : err}`);
    }
}

function assertEq(actual, expected, msg) {
    if (actual !== expected) {
        throw new Error(`${msg || 'expected equality'} - got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
    }
}
function assertClose(actual, expected, epsilon, msg) {
    if (Math.abs(actual - expected) > epsilon) {
        throw new Error(`${msg || 'expected close'} - got ${actual}, expected ${expected} ±${epsilon}`);
    }
}
function assertThrows(fn, substring) {
    let threw = false;
    try { fn(); } catch (err) {
        threw = true;
        if (substring && !String(err.message).includes(substring)) {
            throw new Error(`threw wrong message: "${err.message}" did not contain "${substring}"`);
        }
    }
    if (!threw) throw new Error('expected function to throw');
}
function assertTrue(v, msg) {
    if (!v) throw new Error(msg || 'expected truthy');
}
function assertFalse(v, msg) {
    if (v) throw new Error(msg || 'expected falsy');
}

// ---------- mock AudioContext ----------

function makeMockAudioContext() {
    const ctx = {
        _destroyed: false,
        currentTime: 0,
        sampleRate: 48000,
        scheduledOscillators: [],
        scheduledGains: [],
        destination: { _isDestination: true },
        createOscillator() {
            const osc = {
                _started: false,
                _stoppedAt: null,
                type: null,
                frequency: {
                    _events: [],
                    setValueAtTime(v, t) { this._events.push({ op: 'setValueAtTime', v, t }); },
                },
                connect(target) { this._connectedTo = target; },
                start(t) { this._started = true; this._startedAt = t; },
                stop(t) { this._stoppedAt = t; },
            };
            ctx.scheduledOscillators.push(osc);
            return osc;
        },
        createGain() {
            const g = {
                gain: {
                    value: 1,
                    _events: [],
                    setValueAtTime(v, t) { this.value = v; this._events.push({ op: 'setValueAtTime', v, t }); },
                    linearRampToValueAtTime(v, t) { this._events.push({ op: 'linearRampToValueAtTime', v, t }); },
                    exponentialRampToValueAtTime(v, t) { this._events.push({ op: 'exponentialRampToValueAtTime', v, t }); },
                    cancelScheduledValues(t) { this._events.push({ op: 'cancelScheduledValues', t }); },
                },
                connect(target) { this._connectedTo = target; },
            };
            ctx.scheduledGains.push(g);
            return g;
        },
        close() { this._destroyed = true; },
    };
    return ctx;
}

// ---------- fixtures ----------

function restStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'REST' };
}
function normalStep(note, accent = false) {
    return { note, transpose: 'NORMAL', accent, slide: false, time: 'NORMAL' };
}
function tieStep(note) {
    return { note, transpose: 'NORMAL', accent: false, slide: false, time: 'TIE' };
}
function makePattern(stepFn) {
    const steps = [];
    for (let i = 0; i < 16; i++) steps.push(stepFn(i));
    return { active_steps: 16, triplet: false, steps };
}

// ---------- tests ----------

console.log('bassline-preview tests:');

// stepToPreviewMidi
test('stepToPreviewMidi: C NORMAL → MIDI 36', () => {
    assertEq(stepToPreviewMidi({ note: 'C', transpose: 'NORMAL' }), 36);
});
test('stepToPreviewMidi: C UP adds 12', () => {
    assertEq(stepToPreviewMidi({ note: 'C', transpose: 'UP' }), 48);
});
test('stepToPreviewMidi: C DOWN subtracts 12', () => {
    assertEq(stepToPreviewMidi({ note: 'C', transpose: 'DOWN' }), 24);
});
test('stepToPreviewMidi: B NORMAL (index 11)', () => {
    assertEq(stepToPreviewMidi({ note: 'B', transpose: 'NORMAL' }), 47);
});
test('stepToPreviewMidi: C^ NORMAL (index 12)', () => {
    assertEq(stepToPreviewMidi({ note: 'C^', transpose: 'NORMAL' }), 48);
});
test('stepToPreviewMidi: unknown note throws', () => {
    assertThrows(() => stepToPreviewMidi({ note: 'H', transpose: 'NORMAL' }), 'unknown note');
});

// midiToHz
test('midiToHz: A4 (69) → 440', () => {
    assertClose(midiToHz(69), 440, 0.001);
});
test('midiToHz: A5 (81) → 880', () => {
    assertClose(midiToHz(81), 880, 0.001);
});
test('midiToHz: A3 (57) → 220', () => {
    assertClose(midiToHz(57), 220, 0.001);
});

// stepDurationSec
test('stepDurationSec: 120 BPM non-triplet → 0.125s', () => {
    assertClose(stepDurationSec(120, false), 0.125, 1e-9);
});
test('stepDurationSec: 120 BPM triplet → 0.0833s', () => {
    assertClose(stepDurationSec(120, true), 60 / 120 / 6, 1e-9);
});
test('stepDurationSec: 140 BPM non-triplet → ~0.107s', () => {
    assertClose(stepDurationSec(140, false), 60 / 140 / 4, 1e-9);
});
test('stepDurationSec: zero BPM throws', () => {
    assertThrows(() => stepDurationSec(0, false), 'positive number');
});
test('stepDurationSec: negative BPM throws', () => {
    assertThrows(() => stepDurationSec(-10, false), 'positive number');
});
test('stepDurationSec: non-finite BPM throws', () => {
    assertThrows(() => stepDurationSec(Infinity, false), 'positive number');
});

// buildNoteEvents
test('buildNoteEvents: four_on_floor pattern yields 4 events at 0/4/8/12', () => {
    const pat = makePattern(i => (i % 4 === 0 ? normalStep('D', i === 0) : restStep()));
    const events = buildNoteEvents(pat, 0.125);
    assertEq(events.length, 4);
    assertEq(events[0].stepIndex, 0);
    assertEq(events[1].stepIndex, 4);
    assertEq(events[2].stepIndex, 8);
    assertEq(events[3].stepIndex, 12);
    assertClose(events[0].startOffset, 0, 1e-9);
    assertClose(events[1].startOffset, 0.5, 1e-9);
    assertTrue(events[0].accent, 'step 0 accented');
    assertFalse(events[1].accent, 'step 4 not accented');
});
test('buildNoteEvents: TIE extends previous note duration', () => {
    const steps = [normalStep('D', true), tieStep('D')];
    for (let i = 2; i < 16; i++) steps.push(restStep());
    const pat = { active_steps: 16, triplet: false, steps };
    const events = buildNoteEvents(pat, 0.125);
    assertEq(events.length, 1);
    assertClose(events[0].duration, 0.25, 1e-9);
});
test('buildNoteEvents: REST at start skipped entirely', () => {
    const steps = [restStep(), normalStep('D', true)];
    for (let i = 2; i < 16; i++) steps.push(restStep());
    const pat = { active_steps: 16, triplet: false, steps };
    const events = buildNoteEvents(pat, 0.125);
    assertEq(events.length, 1);
    assertEq(events[0].stepIndex, 1);
});
test('buildNoteEvents: accent flag preserved', () => {
    const steps = [normalStep('D', true), normalStep('D', false)];
    for (let i = 2; i < 16; i++) steps.push(restStep());
    const pat = { active_steps: 16, triplet: false, steps };
    const events = buildNoteEvents(pat, 0.125);
    assertTrue(events[0].accent);
    assertFalse(events[1].accent);
});
test('buildNoteEvents: midi reflects transpose', () => {
    const steps = [
        { note: 'D', transpose: 'NORMAL', accent: true, slide: false, time: 'NORMAL' },
        { note: 'D', transpose: 'UP', accent: false, slide: false, time: 'NORMAL' },
    ];
    for (let i = 2; i < 16; i++) steps.push(restStep());
    const pat = { active_steps: 16, triplet: false, steps };
    const events = buildNoteEvents(pat, 0.125);
    assertEq(events[0].midi, 36 + 2);
    assertEq(events[1].midi, 36 + 2 + 12);
});

// previewBassline / stopPreview / isPreviewing - with mock AudioContext
function setupMockCtx() {
    __resetForTests();
    const mock = makeMockAudioContext();
    __setAudioContextFactory(() => mock);
    return mock;
}

test('previewBassline: requires 16-step array', () => {
    setupMockCtx();
    assertThrows(() => previewBassline({ steps: [] }, { bpm: 120 }), '16-length');
    assertThrows(() => previewBassline(null, { bpm: 120 }), '16-length');
});

test('previewBassline: returns durationSec and eventCount', () => {
    setupMockCtx();
    const pat = makePattern(i => (i % 4 === 0 ? normalStep('D', i === 0) : restStep()));
    const info = previewBassline(pat, { bpm: 120 });
    assertClose(info.durationSec, 16 * 0.125, 1e-9);
    assertEq(info.eventCount, 4);
    stopPreview();
});

test('previewBassline: creates one oscillator per active step', () => {
    const mock = setupMockCtx();
    const pat = makePattern(i => (i % 2 === 0 ? normalStep('D', i === 0) : restStep()));
    previewBassline(pat, { bpm: 120 });
    assertEq(mock.scheduledOscillators.length, 8);
    stopPreview();
});

test('previewBassline: each voice wired osc → gain → master → destination', () => {
    const mock = setupMockCtx();
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    previewBassline(pat, { bpm: 120 });
    const osc = mock.scheduledOscillators[0];
    const voiceGain = mock.scheduledGains[1]; // [0] is master, [1] is first voice
    assertTrue(osc._connectedTo === voiceGain, 'osc → voice gain');
    // voice gain connects to masterGain (first created gain)
    assertTrue(voiceGain._connectedTo === mock.scheduledGains[0], 'voice gain → master gain');
    assertTrue(mock.scheduledGains[0]._connectedTo === mock.destination, 'master → destination');
    stopPreview();
});

test('previewBassline: accent peak higher than normal peak', () => {
    const mock = setupMockCtx();
    const steps = [normalStep('D', true), normalStep('D', false)];
    for (let i = 2; i < 16; i++) steps.push(restStep());
    const pat = { active_steps: 16, triplet: false, steps };
    previewBassline(pat, { bpm: 120 });
    // Voice gains start at scheduledGains[1] (index 0 = master).
    const accentGain = mock.scheduledGains[1];
    const normalGain = mock.scheduledGains[2];
    const accentPeakEv = accentGain.gain._events.find(e => e.op === 'exponentialRampToValueAtTime' && e.v > 0.01);
    const normalPeakEv = normalGain.gain._events.find(e => e.op === 'exponentialRampToValueAtTime' && e.v > 0.01);
    assertTrue(accentPeakEv.v > normalPeakEv.v, 'accent peak > normal peak');
    stopPreview();
});

test('previewBassline: second call stops first', () => {
    const mock = setupMockCtx();
    const pat = makePattern(i => (i % 4 === 0 ? normalStep('D', i === 0) : restStep()));
    previewBassline(pat, { bpm: 120 });
    assertTrue(isPreviewing());
    const firstOscs = mock.scheduledOscillators.slice();
    previewBassline(pat, { bpm: 120 });
    assertTrue(isPreviewing());
    // first batch should have gotten a stop() call rescheduled
    for (const osc of firstOscs) {
        assertTrue(osc._stoppedAt != null, 'first-batch osc was stopped');
    }
    stopPreview();
});

test('stopPreview: is safe when nothing is playing', () => {
    setupMockCtx();
    stopPreview();
    stopPreview();
    assertFalse(isPreviewing());
});

test('stopPreview: fires onEnd with reason=stopped', () => {
    setupMockCtx();
    let endInfo = null;
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    previewBassline(pat, { bpm: 120, onEnd: info => { endInfo = info; } });
    stopPreview();
    assertTrue(endInfo !== null, 'onEnd fired');
    assertEq(endInfo.reason, 'stopped');
});

test('isPreviewing: true during preview, false after stop', () => {
    setupMockCtx();
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    assertFalse(isPreviewing());
    previewBassline(pat, { bpm: 120 });
    assertTrue(isPreviewing());
    stopPreview();
    assertFalse(isPreviewing());
});

// Gain
test('setPreviewGain: clamps to 0..1', () => {
    __resetForTests();
    setPreviewGain(2);
    assertEq(getPreviewGain(), 1);
    setPreviewGain(-0.5);
    assertEq(getPreviewGain(), 0);
    setPreviewGain(0.42);
    assertEq(getPreviewGain(), 0.42);
});
test('setPreviewGain: rejects non-finite', () => {
    __resetForTests();
    assertThrows(() => setPreviewGain(NaN), 'finite');
    assertThrows(() => setPreviewGain(Infinity), 'finite');
});
test('setPreviewGain: applies to context if already created', () => {
    const mock = setupMockCtx();
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    previewBassline(pat, { bpm: 120 });
    setPreviewGain(0.5);
    const master = mock.scheduledGains[0];
    const latest = master.gain._events[master.gain._events.length - 1];
    assertEq(latest.op, 'setValueAtTime');
    assertEq(latest.v, 0.5);
    stopPreview();
});

test('setPreviewGain: initial value applied to new context', () => {
    __resetForTests();
    setPreviewGain(0.8);
    const mock = makeMockAudioContext();
    __setAudioContextFactory(() => mock);
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    previewBassline(pat, { bpm: 120 });
    assertEq(mock.scheduledGains[0].gain.value, 0.8);
    stopPreview();
});

// Triplet timing
test('previewBassline: triplet flag overrides pattern.triplet', () => {
    setupMockCtx();
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    pat.triplet = false;
    const info = previewBassline(pat, { bpm: 120, triplet: true });
    assertClose(info.durationSec, 16 * (60 / 120 / 6), 1e-9);
    stopPreview();
});

test('previewBassline: defaults triplet from pattern.triplet', () => {
    setupMockCtx();
    const pat = makePattern(i => (i === 0 ? normalStep('D', true) : restStep()));
    pat.triplet = true;
    const info = previewBassline(pat, { bpm: 120 });
    assertClose(info.durationSec, 16 * (60 / 120 / 6), 1e-9);
    stopPreview();
});

// ---------- summary ----------

console.log('');
if (failed === 0) {
    console.log(`${passed} passed, 0 failed`);
    process.exit(0);
} else {
    console.log(`${passed} passed, ${failed} FAILED`);
    for (const f of failures) {
        console.log(`  - ${f.name}: ${f.err && f.err.message ? f.err.message : f.err}`);
    }
    process.exit(1);
}
