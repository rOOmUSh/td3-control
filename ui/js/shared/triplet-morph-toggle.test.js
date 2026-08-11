// Usage: node ui/js/shared/triplet-morph-toggle.test.js

import { createTripletMorphEndpointToggle } from './triplet-morph-toggle.js';

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
            for (const listener of listeners.get(type) || []) listener(event);
            return event;
        },
    };
}

function fakeButton() {
    return {
        ...fakeEventTarget(),
        attributes: {},
        dataset: {},
        setAttribute(name, value) { this.attributes[name] = String(value); },
    };
}

function fixture({ initialValue = 0, rejectSets = false } = {}) {
    let value = initialValue;
    const changes = [];
    const button = fakeButton();
    const thumb = { style: {} };
    const control = createTripletMorphEndpointToggle({
        button,
        thumb,
        getValue: () => value,
        setValue: (next) => { if (!rejectSets) value = next; },
        onValueChange: (next) => changes.push(next),
    });
    return {
        button,
        thumb,
        control,
        changes,
        value: () => value,
        setExternal(next) { value = next; },
    };
}

console.log('triplet-morph-toggle tests:');

test('initial render reflects the endpoint amounts', () => {
    const up = fixture({ initialValue: 0 });
    assert(up.button.attributes['role'] === 'switch', 'switch role');
    assert(up.button.attributes['aria-checked'] === 'false', 'unchecked at 0');
    assert(up.button.dataset.position === 'zero', 'position zero at 0');
    assert(up.thumb.style.top === '' && up.thumb.style.bottom === '', 'thumb up');

    const down = fixture({ initialValue: 100 });
    assert(down.button.attributes['aria-checked'] === 'true', 'checked at 100');
    assert(down.button.dataset.position === 'hundred', 'position hundred at 100');
    assert(down.thumb.style.bottom === '2px', 'thumb down');
});

test('clicking jumps to the opposite endpoint from anywhere', () => {
    const fixture0 = fixture({ initialValue: 0 });
    fixture0.button.dispatch('click');
    assert(fixture0.value() === 100, 'up position clicks to 100');
    assert(fixture0.button.attributes['aria-checked'] === 'true', 'now checked');
    assert(fixture0.changes.at(-1) === 100, 'change event fired with 100');
    fixture0.button.dispatch('click');
    assert(fixture0.value() === 0, 'down position clicks back to 0');
    assert(fixture0.changes.at(-1) === 0, 'change event fired with 0');

    const mid = fixture({ initialValue: 57 });
    assert(mid.button.dataset.position === 'zero', 'intermediate start defaults up');
    mid.button.dispatch('click');
    assert(mid.value() === 100, 'clicking from an intermediate amount lands on 100');
});

test('knob sweeps do not move the switch until an endpoint is reached', () => {
    const f = fixture({ initialValue: 0 });
    f.button.dispatch('click');
    assert(f.button.dataset.position === 'hundred', 'at 100 after click');

    // Knob pulls the amount back into the middle: switch holds its side.
    f.setExternal(63);
    f.control.render();
    assert(f.button.dataset.position === 'hundred', 'sweep leaves the switch down');
    f.setExternal(12);
    f.control.render();
    assert(f.button.dataset.position === 'hundred', 'still down mid-sweep');

    // Knob reaches an exact endpoint: switch mirrors it.
    f.setExternal(0);
    f.control.render();
    assert(f.button.dataset.position === 'zero', 'knob at 0 pulls the switch up');
    f.setExternal(100);
    f.control.render();
    assert(f.button.dataset.position === 'hundred', 'knob at 100 pulls it down');
});

test('clicking down from a mid-sweep switch position lands on zero', () => {
    const f = fixture({ initialValue: 100 });
    f.setExternal(40);
    f.control.render();
    assert(f.button.dataset.position === 'hundred', 'holds the 100 side at 40');
    f.button.dispatch('click');
    assert(f.value() === 0, 'opposite endpoint from the held side is 0');
    assert(f.button.dataset.position === 'zero', 'switch is up after the jump');
});

test('a rejecting state setter keeps the switch and fires no change', () => {
    const f = fixture({ initialValue: 0, rejectSets: true });
    f.button.dispatch('click');
    assert(f.value() === 0, 'value refused');
    assert(f.button.dataset.position === 'zero', 'switch stays up');
    assert(f.button.attributes['aria-checked'] === 'false', 'still unchecked');
    assert(f.changes.length === 0, 'no change event on refusal');
});

test('missing DOM handles yield an inert control', () => {
    const control = createTripletMorphEndpointToggle({});
    control.render();
    control.destroy();
    assert(typeof control.render === 'function', 'inert control API intact');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
