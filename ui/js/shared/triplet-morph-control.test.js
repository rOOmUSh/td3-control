// Usage: node ui/js/shared/triplet-morph-control.test.js

import {
    DEFAULT_TRIPLET_MORPH_PERCENT,
    MAX_TRIPLET_MORPH_PERCENT,
    MIN_TRIPLET_MORPH_PERCENT,
    createTripletMorphControl,
    normalizeTripletMorphPercent,
} from './triplet-morph-control.js';

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

function controlFixture({ initialValue = 0, visible = true } = {}) {
    let value = initialValue;
    let isVisible = visible;
    const changes = [];
    const root = fakeElement(['hidden']);
    const display = fakeElement();
    const knob = fakeElement();
    const indicator = fakeElement();
    const eventTarget = fakeEventTarget();
    const control = createTripletMorphControl({
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

console.log('triplet-morph-control tests:');

test('normalization accepts only integers zero through one hundred', () => {
    assert(MIN_TRIPLET_MORPH_PERCENT === 0, 'range starts at zero');
    assert(MAX_TRIPLET_MORPH_PERCENT === 100, 'range ends at one hundred');
    assert(DEFAULT_TRIPLET_MORPH_PERCENT === 0, 'default is zero');
    assert(normalizeTripletMorphPercent(-5) === 0, 'negative clamps to zero');
    assert(normalizeTripletMorphPercent(101) === 100, 'over-range clamps to 100');
    assert(normalizeTripletMorphPercent(49.6) === 50, 'fraction rounds');
    assert(normalizeTripletMorphPercent('37') === 37, 'numeric string accepted');
    assert(normalizeTripletMorphPercent('junk') === 0, 'garbage falls back to zero');
    assert(normalizeTripletMorphPercent(Number.NaN) === 0, 'NaN falls back to zero');
});

test('initial render shows the amount, ARIA range, and visibility', () => {
    const fixture = controlFixture({ initialValue: 0 });
    assert(fixture.display.textContent === '0%', 'display shows 0%');
    assert(fixture.knob.attributes['role'] === 'slider', 'role slider');
    assert(fixture.knob.attributes['aria-label'] === 'Triplet morph', 'aria label');
    assert(fixture.knob.attributes['aria-valuemin'] === '0', 'aria min 0');
    assert(fixture.knob.attributes['aria-valuemax'] === '100', 'aria max 100');
    assert(fixture.knob.attributes['aria-valuenow'] === '0', 'aria now 0');
    assert(!fixture.root.classList.contains('hidden'), 'visible root un-hidden');
});

test('wheel steps by one and clamps at both ends', () => {
    const fixture = controlFixture({ initialValue: 0 });
    fixture.knob.dispatch('wheel', { deltaY: 100 });
    assert(fixture.value() === 0, 'cannot go below zero');
    fixture.knob.dispatch('wheel', { deltaY: -100 });
    assert(fixture.value() === 1, 'wheel up adds one');
    for (let i = 0; i < 150; i += 1) fixture.knob.dispatch('wheel', { deltaY: -100 });
    assert(fixture.value() === 100, 'clamps at one hundred');
    assert(fixture.changes.at(-1) === 100, 'change events fired');
});

test('drag uses three pixels per unit against the drag start value', () => {
    const fixture = controlFixture({ initialValue: 40 });
    fixture.knob.dispatch('mousedown', { button: 0, clientY: 300 });
    fixture.eventTarget.dispatch('mousemove', { clientY: 270 });
    assert(fixture.value() === 50, '30 px up adds 10');
    fixture.eventTarget.dispatch('mousemove', { clientY: 330 });
    assert(fixture.value() === 30, '30 px down from start subtracts 10');
    fixture.eventTarget.dispatch('mouseup', {});
    fixture.eventTarget.dispatch('mousemove', { clientY: 0 });
    assert(fixture.value() === 30, 'movement after mouseup is ignored');
});

test('arrows, Home, End, PageUp, and PageDown update the value', () => {
    const fixture = controlFixture({ initialValue: 10 });
    fixture.knob.dispatch('keydown', { key: 'ArrowUp' });
    assert(fixture.value() === 11, 'ArrowUp adds one');
    fixture.knob.dispatch('keydown', { key: 'ArrowLeft' });
    assert(fixture.value() === 10, 'ArrowLeft subtracts one');
    fixture.knob.dispatch('keydown', { key: 'PageUp' });
    assert(fixture.value() === 20, 'PageUp adds ten');
    fixture.knob.dispatch('keydown', { key: 'PageDown' });
    assert(fixture.value() === 10, 'PageDown subtracts ten');
    fixture.knob.dispatch('keydown', { key: 'End' });
    assert(fixture.value() === 100, 'End jumps to one hundred');
    fixture.knob.dispatch('keydown', { key: 'Home' });
    assert(fixture.value() === 0, 'Home jumps to zero');
    fixture.knob.dispatch('keydown', { key: 'PageDown' });
    assert(fixture.value() === 0, 'PageDown clamps at zero');
});

test('visibility follows the isVisible callback without value loss', () => {
    const fixture = controlFixture({ initialValue: 42, visible: true });
    assert(!fixture.root.classList.contains('hidden'), 'starts visible');
    fixture.setVisible(false);
    fixture.control.render();
    assert(fixture.root.classList.contains('hidden'), 'hidden while LIVE is on');
    assert(fixture.value() === 42, 'value survives hiding');
    fixture.setVisible(true);
    fixture.control.render();
    assert(!fixture.root.classList.contains('hidden'), 'visible again');
    assert(fixture.display.textContent === '42%', 'display restored');
});

test('a rejecting state setter wins over the requested value', () => {
    let value = 0;
    const root = fakeElement();
    const display = fakeElement();
    const knob = fakeElement();
    const indicator = fakeElement();
    const eventTarget = fakeEventTarget();
    const changes = [];
    createTripletMorphControl({
        root,
        display,
        knob,
        indicator,
        eventTarget,
        getValue: () => value,
        setValue: () => { /* state refuses the change */ },
        onValueChange: (next) => changes.push(next),
    });
    knob.dispatch('wheel', { deltaY: -100 });
    assert(display.textContent === '0%', 'display reflects the refused state');
    assert(changes.length === 0, 'no change event for a refused set');
});

test('missing DOM handles yield an inert control', () => {
    const control = createTripletMorphControl({});
    control.render();
    control.destroy();
    assert(typeof control.render === 'function', 'inert control API intact');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
