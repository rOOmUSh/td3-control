// Usage: node ui/js/shared/pitch-bend-control.test.js

import {
    PITCH_BEND_CENTER,
    PITCH_BEND_STEP,
    PITCH_BEND_STORAGE_KEY,
    createPitchBendControl,
    formatPitchBend,
    normalizePitchBend,
    readPitchBend,
    writePitchBend,
} from './pitch-bend-control.js';
import { fakeElement, fakeEventTarget, fakeStorage } from './knob-test-fakes.js';

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

function controlFixture({ initialValue = PITCH_BEND_CENTER, visible = true } = {}) {
    let value = initialValue;
    let isVisible = visible;
    const changes = [];
    const root = fakeElement(['hidden']);
    const display = fakeElement();
    const knob = fakeElement();
    const indicator = fakeElement();
    const eventTarget = fakeEventTarget();
    const control = createPitchBendControl({
        root,
        display,
        knob,
        indicator,
        eventTarget,
        getValue: () => value,
        setValue: (next) => { value = next; },
        isVisible: () => isVisible,
        onValueChange: (next) => changes.push(next),
    });
    return {
        changes,
        control,
        display,
        eventTarget,
        knob,
        root,
        setVisible(next) { isVisible = next; },
        value() { return value; },
    };
}

console.log('pitch-bend-control tests:');

test('normalization clamps to 0..16383 and defaults to center', () => {
    assert(normalizePitchBend(-5) === 0, 'negative clamps to zero');
    assert(normalizePitchBend(16384) === 16383, 'over-range clamps to 16383');
    assert(normalizePitchBend('junk') === PITCH_BEND_CENTER, 'invalid uses center');
    assert(normalizePitchBend(undefined) === PITCH_BEND_CENTER, 'missing uses center');
});

test('readout is the signed offset from center', () => {
    assert(formatPitchBend(8192) === '0', 'center shows 0');
    assert(formatPitchBend(0) === '-8192', 'minimum');
    assert(formatPitchBend(16383) === '+8191', 'maximum');
    assert(formatPitchBend(4096) === '-4096', 'below center');
    assert(formatPitchBend(8193) === '+1', 'one above center');
});

test('storage defaults to center on missing or corrupt values', () => {
    const storage = fakeStorage();
    assert(readPitchBend(storage) === PITCH_BEND_CENTER, 'missing uses center');
    for (const corrupt of ['bad', '-1', '16384', '100.5']) {
        storage.setItem(PITCH_BEND_STORAGE_KEY, corrupt);
        assert(readPitchBend(storage) === PITCH_BEND_CENTER, `${corrupt} uses center`);
    }
    assert(writePitchBend(0, storage) === 0, 'zero is a valid stored value');
    assert(readPitchBend(storage) === 0, 'zero reads back');
    assert(writePitchBend(12000, storage) === 12000, 'write returns normalized value');
    assert(readPitchBend(storage) === 12000, 'second reader sees shared value');
});

test('unavailable storage cannot crash reads or writes', () => {
    const storage = {
        getItem() { throw new Error('blocked'); },
        setItem() { throw new Error('blocked'); },
    };
    assert(readPitchBend(storage) === PITCH_BEND_CENTER, 'blocked read uses center');
    assert(writePitchBend(9000, storage) === 9000, 'blocked write returns normalized value');
});

test('initial render exposes center and slider metadata', () => {
    const fixture = controlFixture();
    assert(fixture.display.textContent === '0', 'center readout');
    assert(fixture.knob.attributes['aria-label'] === 'Pitch bend', 'accessible label');
    assert(fixture.knob.attributes['aria-valuemin'] === '0', 'minimum metadata');
    assert(fixture.knob.attributes['aria-valuemax'] === '16383', 'maximum metadata');
    assert(fixture.knob.attributes['aria-valuenow'] === '8192', 'raw value metadata');
    assert(fixture.knob.attributes['aria-valuetext'] === '0', 'signed value text');
});

test('wheel and arrows move one bend step and clamp at the ends', () => {
    const fixture = controlFixture();
    fixture.knob.dispatch('wheel', { deltaY: -1 });
    assert(fixture.value() === PITCH_BEND_CENTER + PITCH_BEND_STEP, 'wheel up adds one step');
    assert(fixture.display.textContent === `+${PITCH_BEND_STEP}`, 'readout follows');
    fixture.knob.dispatch('keydown', { key: 'ArrowDown' });
    assert(fixture.value() === PITCH_BEND_CENTER, 'arrow down returns to center');
    fixture.knob.dispatch('keydown', { key: 'End' });
    assert(fixture.value() === 16383, 'End selects maximum');
    fixture.knob.dispatch('wheel', { deltaY: -1 });
    assert(fixture.value() === 16383, 'wheel up clamps at maximum');
});

test('drag scales by the bend step so a full sweep fits on screen', () => {
    const fixture = controlFixture();
    fixture.knob.dispatch('mousedown', { button: 0, clientY: 100 });
    fixture.eventTarget.dispatch('mousemove', { clientY: 70 });
    assert(fixture.value() === PITCH_BEND_CENTER + 10 * PITCH_BEND_STEP,
        'thirty pixels upward adds ten steps');
    fixture.eventTarget.dispatch('mousemove', { clientY: 130 });
    assert(fixture.value() === PITCH_BEND_CENTER - 10 * PITCH_BEND_STEP,
        'thirty pixels below the start subtracts ten steps');
    fixture.eventTarget.dispatch('mouseup');
});

test('the knob holds its position after release', () => {
    const fixture = controlFixture();
    fixture.knob.dispatch('mousedown', { button: 0, clientY: 100 });
    fixture.eventTarget.dispatch('mousemove', { clientY: 88 });
    fixture.eventTarget.dispatch('mouseup');
    assert(fixture.value() === PITCH_BEND_CENTER + 4 * PITCH_BEND_STEP,
        'value stays where the drag ended');
});

test('double-click and Enter reset to center and notify once each', () => {
    const fixture = controlFixture({ initialValue: 1000 });
    fixture.knob.dispatch('dblclick');
    assert(fixture.value() === PITCH_BEND_CENTER, 'double-click centers');
    fixture.knob.dispatch('keydown', { key: 'ArrowUp' });
    fixture.knob.dispatch('keydown', { key: 'Enter' });
    assert(fixture.value() === PITCH_BEND_CENTER, 'Enter centers');
    assert(fixture.changes.join(',') === [
        PITCH_BEND_CENTER,
        PITCH_BEND_CENTER + PITCH_BEND_STEP,
        PITCH_BEND_CENTER,
    ].join(','), 'each reset notifies with the center value');
});

test('hidden when unsupported, value retained', () => {
    const fixture = controlFixture({ initialValue: 3000 });
    fixture.setVisible(false);
    fixture.control.render();
    assert(fixture.root.classList.contains('hidden'), 'hidden when unsupported');
    assert(fixture.value() === 3000, 'value retained while hidden');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
