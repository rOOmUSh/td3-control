// Usage: node ui/js/shared/knob-3d.test.js
//
// `knob-renderer.js` imports three.js and needs a WebGL context, so it cannot
// be loaded here. `mountKnob3d` takes a `loadRenderer` injection point for
// exactly that reason, which lets the mount and failure paths be exercised
// against a stub.

import { CANVAS_CLASS, MOUNTED_CLASS, mountKnob3d } from './knob-3d.js';

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

async function test(name, fn) {
    try {
        await fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

// --- DOM stand-ins --------------------------------------------------------

function fakeHost() {
    const set = new Set();
    const created = [];
    return {
        children: [],
        created,
        classList: {
            add: (n) => set.add(n),
            remove: (n) => set.delete(n),
            contains: (n) => set.has(n),
        },
        ownerDocument: {
            createElement() {
                const attributes = new Map();
                const el = {
                    className: '',
                    dataset: {},
                    style: {},
                    width: 0,
                    height: 0,
                    removed: false,
                    setAttribute: (n, v) => attributes.set(n, v),
                    getAttribute: (n) => (attributes.has(n) ? attributes.get(n) : null),
                    remove() { this.removed = true; },
                };
                created.push(el);
                return el;
            },
        },
        appendChild(child) { this.children.push(child); },
    };
}

/** A shared-renderer stub that records every paint. */
function stubRenderer({ fail = false } = {}) {
    const paints = [];
    return {
        paints,
        loadRenderer: async () => {
            if (fail) throw new Error('no WebGL context');
            return { paint: (target, degrees) => { paints.push([target, degrees]); return true; } };
        },
    };
}

const base = { min: 0, max: 100, value: 50, size: 25 };

// --- mount ----------------------------------------------------------------

await test('a mount sizes the canvas and marks the host', async () => {
    const host = fakeHost();
    const stub = stubRenderer();
    const handle = await mountKnob3d({ ...base, host, loadRenderer: stub.loadRenderer });

    assert(handle !== null, 'returns a handle');
    assert(host.children.length === 1, 'canvas appended to the host');
    assert(handle.canvas.className === CANVAS_CLASS, 'canvas carries the shared class');
    assert(host.classList.contains(MOUNTED_CLASS), 'host marked as mounted');
    assert(handle.canvas.style.width === '25px', 'CSS width is the requested size');
    assert(handle.canvas.style.height === '25px', 'CSS height is the requested size');
    assert(handle.canvas.width > 25 && handle.canvas.height > 25,
        'backing store is larger than the CSS box, so the blit scales down');
    assert(handle.canvas.getAttribute('aria-hidden') === 'true',
        'hidden from assistive tech, the host carries the ARIA');
});

await test('the initial value is painted at its angle', async () => {
    const host = fakeHost();
    const stub = stubRenderer();
    await mountKnob3d({ ...base, host, value: 0, loadRenderer: stub.loadRenderer });

    assert(stub.paints.length === 1, 'painted once on mount');
    assert(stub.paints[0][1] === -145, 'the minimum sits at -145 degrees');
});

await test('the sweep runs minimum to maximum across the full arc', async () => {
    const host = fakeHost();
    const stub = stubRenderer();
    const handle = await mountKnob3d({ ...base, host, loadRenderer: stub.loadRenderer });

    handle.setValue(0);
    handle.setValue(50);
    handle.setValue(100);
    const degrees = stub.paints.slice(-3).map(([, d]) => d);
    assert(degrees[0] === -145, 'minimum at -145');
    assert(degrees[1] === 0, 'midpoint straight up');
    assert(degrees[2] === 145, 'maximum at +145');
});

await test('setValue clamps and publishes the drawn value', async () => {
    const host = fakeHost();
    const stub = stubRenderer();
    const handle = await mountKnob3d({ ...base, host, loadRenderer: stub.loadRenderer });

    handle.setValue(9000);
    assert(handle.canvas.dataset.knobValue === '100', 'clamped to the maximum');
    handle.setValue(-5);
    assert(handle.canvas.dataset.knobValue === '0', 'clamped to the minimum');
    handle.setValue(63.5);
    assert(handle.canvas.dataset.knobValue === '63.5', 'fractional values survive');
    handle.setValue(NaN);
    assert(handle.canvas.dataset.knobValue === '0', 'unusable input resolves to the minimum');
});

await test('every mount paints through the one shared renderer', async () => {
    const stub = stubRenderer();
    const hosts = [fakeHost(), fakeHost(), fakeHost()];
    for (const host of hosts) {
        await mountKnob3d({ ...base, host, loadRenderer: stub.loadRenderer });
    }
    assert(stub.paints.length === 3, 'three mounts, three paints, one renderer');
    const targets = new Set(stub.paints.map(([target]) => target));
    assert(targets.size === 3, 'each mount painted into its own canvas');
});

// --- failure paths --------------------------------------------------------

await test('a failed renderer leaves the host untouched', async () => {
    const host = fakeHost();
    const stub = stubRenderer({ fail: true });
    const handle = await mountKnob3d({ ...base, host, loadRenderer: stub.loadRenderer });

    assert(handle === null, 'returns null so the caller needs no error branch');
    assert(!host.classList.contains(MOUNTED_CLASS), 'host not marked as mounted');
});

await test('a missing host returns null before loading anything', async () => {
    let loaded = false;
    const handle = await mountKnob3d({
        ...base,
        host: null,
        loadRenderer: async () => { loaded = true; return { paint() {} }; },
    });
    assert(handle === null, 'returns null');
    assert(loaded === false, 'the renderer is never built without a host');
});

await test('an unusable domain or size returns null before loading anything', async () => {
    let loaded = false;
    const load = async () => { loaded = true; return { paint() {} }; };
    const bad = [
        { min: 10, max: 10 },
        { min: 10, max: 5 },
        { min: NaN, max: 100 },
        { min: 0, max: Infinity },
        { size: 0 },
        { size: -4 },
        { size: NaN },
    ];
    for (const override of bad) {
        const handle = await mountKnob3d({
            ...base, ...override, host: fakeHost(), loadRenderer: load,
        });
        assert(handle === null, `rejected ${JSON.stringify(override)}`);
    }
    assert(loaded === false, 'the renderer is never built for unusable input');
});

await test('dispose unmarks the host and removes the canvas', async () => {
    const host = fakeHost();
    const stub = stubRenderer();
    const handle = await mountKnob3d({ ...base, host, loadRenderer: stub.loadRenderer });

    handle.dispose();
    assert(!host.classList.contains(MOUNTED_CLASS), 'host unmarked');
    assert(host.created[0].removed === true, 'canvas removed');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
