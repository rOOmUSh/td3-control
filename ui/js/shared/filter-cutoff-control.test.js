// Usage: node ui/js/shared/filter-cutoff-control.test.js

import {
    DEFAULT_FILTER_CUTOFF,
    FILTER_CUTOFF_STORAGE_KEY,
    createFilterCutoffControl,
    formatFilterCutoff,
    normalizeFilterCutoff,
    readFilterCutoff,
    writeFilterCutoff,
} from './filter-cutoff-control.js';
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

function controlFixture({ initialValue = 64, visible = true } = {}) {
    let value = initialValue;
    let isVisible = visible;
    const changes = [];
    const root = fakeElement(['hidden']);
    const display = fakeElement();
    const knob = fakeElement();
    const indicator = fakeElement();
    const eventTarget = fakeEventTarget();
    const control = createFilterCutoffControl({
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

console.log('filter-cutoff-control tests:');

test('normalization rounds and clamps to 0..127', () => {
    assert(normalizeFilterCutoff(-1) === 0, 'negative clamps to zero');
    assert(normalizeFilterCutoff(128) === 127, 'over-range clamps to 127');
    assert(normalizeFilterCutoff(63.6) === 64, 'fraction rounds');
    assert(normalizeFilterCutoff('junk') === DEFAULT_FILTER_CUTOFF, 'invalid uses default');
});

test('readout is the raw CC value with no percent sign', () => {
    assert(formatFilterCutoff(0) === '0', 'zero');
    assert(formatFilterCutoff(127) === '127', 'maximum');
    assert(formatFilterCutoff(200) === '127', 'clamped before formatting');
});

test('storage defaults on missing or corrupt values and shares valid values', () => {
    const storage = fakeStorage();
    assert(readFilterCutoff(storage) === DEFAULT_FILTER_CUTOFF, 'missing uses default');
    for (const corrupt of ['bad', '-1', '128', '25.5']) {
        storage.setItem(FILTER_CUTOFF_STORAGE_KEY, corrupt);
        assert(readFilterCutoff(storage) === DEFAULT_FILTER_CUTOFF, `${corrupt} uses default`);
    }
    assert(writeFilterCutoff(0, storage) === 0, 'zero is a valid stored value');
    assert(readFilterCutoff(storage) === 0, 'zero reads back');
    assert(writeFilterCutoff(100, storage) === 100, 'write returns normalized value');
    assert(readFilterCutoff(storage) === 100, 'second reader sees shared value');
});

test('unavailable storage cannot crash reads or writes', () => {
    const storage = {
        getItem() { throw new Error('blocked'); },
        setItem() { throw new Error('blocked'); },
    };
    assert(readFilterCutoff(storage) === DEFAULT_FILTER_CUTOFF, 'blocked read uses default');
    assert(writeFilterCutoff(12, storage) === 12, 'blocked write returns normalized value');
});

test('initial render exposes value and slider metadata', () => {
    const fixture = controlFixture();
    assert(fixture.display.textContent === '64', 'digital readout');
    assert(fixture.knob.attributes['aria-label'] === 'Filter cutoff', 'accessible label');
    assert(fixture.knob.attributes['aria-valuemin'] === '0', 'minimum metadata');
    assert(fixture.knob.attributes['aria-valuemax'] === '127', 'maximum metadata');
    assert(fixture.knob.attributes['aria-valuenow'] === '64', 'current value metadata');
    assert(fixture.knob.attributes['aria-valuetext'] === '64', 'value text has no unit');
    assert(!fixture.root.classList.contains('hidden'), 'visible when supported');
});

test('wheel, arrows, and drag move one unit and clamp', () => {
    const fixture = controlFixture({ initialValue: 126 });
    fixture.knob.dispatch('wheel', { deltaY: -1 });
    fixture.knob.dispatch('wheel', { deltaY: -1 });
    assert(fixture.value() === 127, 'wheel up clamps at 127');
    fixture.knob.dispatch('keydown', { key: 'Home' });
    assert(fixture.value() === 0, 'Home selects zero');
    fixture.knob.dispatch('keydown', { key: 'ArrowDown' });
    assert(fixture.value() === 0, 'lower arrow clamps at zero');
    fixture.knob.dispatch('mousedown', { button: 0, clientY: 100 });
    fixture.eventTarget.dispatch('mousemove', { clientY: 70 });
    assert(fixture.value() === 10, 'thirty pixels upward adds ten');
    fixture.eventTarget.dispatch('mouseup');
    assert(fixture.changes.join(',') === '127,0,10', 'only real changes notify');
});

test('hidden when the device does not support the control, value retained', () => {
    const fixture = controlFixture({ initialValue: 30 });
    fixture.setVisible(false);
    fixture.control.render();
    assert(fixture.root.classList.contains('hidden'), 'hidden when unsupported');
    assert(fixture.value() === 30, 'value retained while hidden');
    fixture.setVisible(true);
    fixture.control.render();
    assert(!fixture.root.classList.contains('hidden'), 'visible again when supported');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
