// Usage: node ui/js/shared/gate-control.test.js

import {
    DEFAULT_GATE_PERCENT,
    GATE_STORAGE_KEY,
    createGateControl,
    normalizeGatePercent,
    readGatePercent,
    LEGACY_GATE_STORAGE_KEY,
    writeGatePercent,
} from './gate-control.js';

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

function fakeStorage(initial = {}) {
    const values = new Map(Object.entries(initial));
    return {
        getItem(key) { return values.has(key) ? values.get(key) : null; },
        setItem(key, value) { values.set(key, String(value)); },
    };
}

function fakeClassList(initial = []) {
    const classes = new Set(initial);
    return {
        contains(name) { return classes.has(name); },
        toggle(name, force) {
            if (force === undefined ? !classes.has(name) : force) classes.add(name);
            else classes.delete(name);
        },
    };
}

function fakeEventTarget() {
    const listeners = new Map();
    return {
        addEventListener(type, listener) {
            if (!listeners.has(type)) listeners.set(type, new Set());
            listeners.get(type).add(listener);
        },
        removeEventListener(type, listener) {
            listeners.get(type)?.delete(listener);
        },
        dispatch(type, event = {}) {
            event.preventDefault ||= () => { event.defaultPrevented = true; };
            for (const listener of listeners.get(type) || []) listener(event);
            return event;
        },
    };
}

function fakeElement(initialClasses = []) {
    return {
        ...fakeEventTarget(),
        attributes: {},
        classList: fakeClassList(initialClasses),
        style: {},
        textContent: '',
        setAttribute(name, value) { this.attributes[name] = String(value); },
    };
}

function controlFixture({ initialValue = 50, visible = true } = {}) {
    let value = initialValue;
    let isVisible = visible;
    const changes = [];
    const root = fakeElement(['hidden']);
    const display = fakeElement();
    const knob = fakeElement();
    const indicator = fakeElement();
    const eventTarget = fakeEventTarget();
    const control = createGateControl({
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
        indicator,
        knob,
        root,
        setVisible(next) { isVisible = next; },
        value() { return value; },
    };
}

console.log('gate-control tests:');

test('normalization rounds and clamps to the integer range', () => {
    assert(normalizeGatePercent(0) === 1, 'zero clamps to one');
    assert(normalizeGatePercent(101) === 100, 'over-range clamps to 100');
    assert(normalizeGatePercent(49.6) === 50, 'fraction rounds to nearest integer');
    assert(normalizeGatePercent('bad') === 50, 'invalid value uses default');
});

test('storage defaults on missing or corrupt values and shares valid values', () => {
    const storage = fakeStorage();
    assert(readGatePercent(storage) === DEFAULT_GATE_PERCENT, 'missing uses default');
    for (const corrupt of ['bad', '0', '101', '25.5']) {
        storage.setItem(GATE_STORAGE_KEY, corrupt);
        assert(readGatePercent(storage) === DEFAULT_GATE_PERCENT,
            `${corrupt} uses default`);
    }
    assert(writeGatePercent(37, storage) === 37, 'write returns normalized value');
    assert(readGatePercent(storage) === 37, 'second reader sees shared value');
});

test('unavailable storage cannot crash reads or writes', () => {
    const storage = {
        getItem() { throw new Error('blocked'); },
        setItem() { throw new Error('blocked'); },
    };
    assert(readGatePercent(storage) === DEFAULT_GATE_PERCENT,
        'blocked read uses default');
    assert(writeGatePercent(64, storage) === 64, 'blocked write returns normalized value');
});

test('initial render exposes fixed value and slider accessibility metadata', () => {
    const fixture = controlFixture();
    assert(fixture.display.textContent === '50%', 'digital readout');
    assert(fixture.knob.attributes.role === 'slider', 'slider role');
    assert(fixture.knob.attributes.tabindex === '0', 'keyboard focusable');
    assert(fixture.knob.attributes['aria-label'] === 'Gate', 'accessible label');
    assert(fixture.knob.attributes['aria-valuemin'] === '1', 'minimum metadata');
    assert(fixture.knob.attributes['aria-valuemax'] === '100', 'maximum metadata');
    assert(fixture.knob.attributes['aria-valuenow'] === '50', 'current value metadata');
    assert(!fixture.root.classList.contains('hidden'), 'visible while LIVE is off');
});

test('wheel adjusts one percent and clamps at both ends', () => {
    const fixture = controlFixture({ initialValue: 99 });
    fixture.knob.dispatch('wheel', { deltaY: -1 });
    fixture.knob.dispatch('wheel', { deltaY: -1 });
    assert(fixture.value() === 100, 'wheel up clamps at 100');
    fixture.knob.dispatch('wheel', { deltaY: 1 });
    assert(fixture.value() === 99, 'wheel down subtracts one');
    assert(fixture.changes.join(',') === '100,99', 'only real changes notify');
});

test('vertical drag uses three pixels per percent', () => {
    const fixture = controlFixture({ initialValue: 50 });
    fixture.knob.dispatch('mousedown', { button: 0, clientY: 100 });
    fixture.eventTarget.dispatch('mousemove', { clientY: 91 });
    assert(fixture.value() === 53, 'nine pixels upward adds three');
    fixture.eventTarget.dispatch('mouseup');
    fixture.eventTarget.dispatch('mousemove', { clientY: 82 });
    assert(fixture.value() === 53, 'movement after mouseup is ignored');
});

test('keyboard arrows, Home, and End adjust and clamp', () => {
    const fixture = controlFixture({ initialValue: 50 });
    fixture.knob.dispatch('keydown', { key: 'ArrowRight' });
    fixture.knob.dispatch('keydown', { key: 'ArrowDown' });
    assert(fixture.value() === 50, 'opposite arrows cancel');
    fixture.knob.dispatch('keydown', { key: 'Home' });
    assert(fixture.value() === 1, 'Home selects one');
    fixture.knob.dispatch('keydown', { key: 'ArrowLeft' });
    assert(fixture.value() === 1, 'lower arrow clamps');
    fixture.knob.dispatch('keydown', { key: 'End' });
    assert(fixture.value() === 100, 'End selects 100');
});

test('LIVE visibility toggles without discarding the value', () => {
    const fixture = controlFixture({ initialValue: 63 });
    fixture.setVisible(false);
    fixture.control.render();
    assert(fixture.root.classList.contains('hidden'), 'hidden while LIVE is on');
    assert(fixture.value() === 63, 'value retained while hidden');
    fixture.setVisible(true);
    fixture.control.render();
    assert(!fixture.root.classList.contains('hidden'), 'visible when LIVE returns off');
    assert(fixture.display.textContent === '63%', 'retained value repainted');
});

test('a session written before the rename keeps its value', () => {
    // The control was called DECAY and stored under its own key. An
    // existing session must not silently fall back to the default.
    const storage = fakeStorage();
    storage.setItem(LEGACY_GATE_STORAGE_KEY, '37');
    assert(readGatePercent(storage) === 37, 'legacy key is read');

    // The next write moves the value onto the current key.
    writeGatePercent(64, storage);
    assert(storage.getItem(GATE_STORAGE_KEY) === '64', 'written under the current key');
    assert(readGatePercent(storage) === 64, 'current key wins over the legacy one');

    // A legacy value outside the range still falls back.
    const broken = fakeStorage();
    broken.setItem(LEGACY_GATE_STORAGE_KEY, '900');
    assert(readGatePercent(broken) === DEFAULT_GATE_PERCENT, 'invalid legacy value ignored');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
