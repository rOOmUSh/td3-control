// Tests for progression-package-export.js - runs with Node.js
// Usage: node ui/js/progression/progression-package-export.test.js
//
// Covers both the pure helpers and the DOM wiring via a tiny hand-rolled
// browser shim.

import {
    DEFAULT_FORMATS,
    PER_PATTERN_KEYS,
    COMBINED_KEYS,
    loadFormatsFromStorage,
    saveFormatsToStorage,
    anyFormatSelected,
    buildExportPayload,
    init,
    setPackage,
    clearPackage,
    __private,
} from './progression-package-export.js';

// --- Tiny in-memory localStorage shim ---------------------------------------

function makeStorage(initial) {
    const map = new Map(initial ? Object.entries(initial) : []);
    return {
        getItem(key) { return map.has(key) ? map.get(key) : null; },
        setItem(key, value) { map.set(key, String(value)); },
        removeItem(key) { map.delete(key); },
        _dump() { return Object.fromEntries(map.entries()); },
    };
}

// --- Pattern fixtures -------------------------------------------------------

function defaultStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' };
}

function defaultPattern() {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, defaultStep),
    };
}

function four() { return [defaultPattern(), defaultPattern(), defaultPattern(), defaultPattern()]; }
function twenty() { return Array.from({ length: 20 }, defaultPattern); }

// --- Tiny DOM shim ----------------------------------------------------------

class FakeElement {
    constructor(id) {
        this.id = id;
        this.checked = false;
        this.disabled = false;
        this.title = '';
        this.textContent = '';
        this.listeners = new Map();
    }
    addEventListener(type, fn) {
        if (!this.listeners.has(type)) this.listeners.set(type, []);
        this.listeners.get(type).push(fn);
    }
    dispatch(type) {
        const handlers = this.listeners.get(type) || [];
        for (const fn of handlers) fn({ target: this, type });
    }
    click() {
        if (this.disabled) return;
        this.dispatch('click');
    }
    change() {
        this.dispatch('change');
    }
}

function makeDom() {
    const ids = [
        'btn-save-package',
        'pkg-status',
        'pkg-last-saved',
        ...Object.keys(DEFAULT_FORMATS).map(k => `pkg-fmt-${k}`),
    ];
    const els = new Map(ids.map(id => [id, new FakeElement(id)]));
    return {
        elements: els,
        document: {
            getElementById(id) { return els.get(id) || null; },
        },
    };
}

function installDom(initialStorage) {
    const { elements, document } = makeDom();
    globalThis.document = document;
    globalThis.localStorage = makeStorage(initialStorage);
    return {
        btnSave: elements.get('btn-save-package'),
        pkgStatus: elements.get('pkg-status'),
        pkgLastSaved: elements.get('pkg-last-saved'),
        checkbox(key) { return elements.get(`pkg-fmt-${key}`); },
    };
}

// --- Test runner ------------------------------------------------------------

let passed = 0, failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; }
    else passed++;
}

async function test(name, fn) {
    try {
        await fn();
        console.log(`  ok: ${name}`);
    } catch (e) {
        console.error(`  FAIL: ${name}: ${e.stack || e.message}`);
        failed++;
    }
}

// --- Tests ------------------------------------------------------------------

await test('DEFAULT_FORMATS', () => {
    assert(DEFAULT_FORMATS.mid === true, 'mid default on');
    assert(DEFAULT_FORMATS.steps_txt === true, 'steps_txt default on');
    assert(DEFAULT_FORMATS.seq === true, 'seq default on');
    assert(DEFAULT_FORMATS.pat === false, 'pat default off');
    assert(DEFAULT_FORMATS.rbs === false, 'rbs default off');
    assert(DEFAULT_FORMATS.json === false, 'json default off');
    assert(DEFAULT_FORMATS.toml === false, 'toml default off');
    assert(DEFAULT_FORMATS.combined_rbs === false, 'combined_rbs default off');
    assert(DEFAULT_FORMATS.combined_sqs === false, 'combined_sqs default off');
});

await test('DEFAULT_FORMATS is frozen (prevents accidental mutation)', () => {
    let threw = false;
    try { DEFAULT_FORMATS.mid = false; } catch (_) { threw = true; }
    // In non-strict mode the assignment silently fails; just verify the value
    // didn't change.
    assert(DEFAULT_FORMATS.mid === true, 'mid stays true');
});

await test('PER_PATTERN_KEYS covers all 7 per-pattern formats', () => {
    assert(PER_PATTERN_KEYS.length === 7, '7 per-pattern keys');
    for (const k of ['mid', 'steps_txt', 'seq', 'pat', 'rbs', 'json', 'toml']) {
        assert(PER_PATTERN_KEYS.includes(k), `includes ${k}`);
    }
});

await test('COMBINED_KEYS covers combined_rbs and combined_sqs', () => {
    assert(COMBINED_KEYS.length === 2, '2 combined keys');
    assert(COMBINED_KEYS.includes('combined_rbs'), 'includes combined_rbs');
    assert(COMBINED_KEYS.includes('combined_sqs'), 'includes combined_sqs');
});

await test('loadFormatsFromStorage returns defaults when empty', () => {
    const storage = makeStorage();
    const loaded = loadFormatsFromStorage(storage);
    for (const k of Object.keys(DEFAULT_FORMATS)) {
        assert(loaded[k] === DEFAULT_FORMATS[k], `${k} defaulted`);
    }
});

await test('loadFormatsFromStorage ignores corrupt JSON', () => {
    const storage = makeStorage({ [__private.FORMATS_STORAGE_KEY]: 'not-json{' });
    const loaded = loadFormatsFromStorage(storage);
    assert(loaded.mid === true, 'mid defaulted after corrupt JSON');
    assert(loaded.pat === false, 'pat defaulted after corrupt JSON');
});

await test('loadFormatsFromStorage merges stored booleans over defaults', () => {
    const storage = makeStorage();
    storage.setItem(__private.FORMATS_STORAGE_KEY, JSON.stringify({ mid: false, pat: true, rbs: true }));
    const loaded = loadFormatsFromStorage(storage);
    assert(loaded.mid === false, 'mid overridden to false');
    assert(loaded.pat === true, 'pat overridden to true');
    assert(loaded.rbs === true, 'rbs overridden to true');
    assert(loaded.steps_txt === true, 'steps_txt keeps default true');
    assert(loaded.combined_rbs === false, 'combined_rbs keeps default false');
});

await test('loadFormatsFromStorage rejects non-boolean stored values', () => {
    const storage = makeStorage();
    storage.setItem(__private.FORMATS_STORAGE_KEY, JSON.stringify({ mid: 'yes', pat: 1 }));
    const loaded = loadFormatsFromStorage(storage);
    assert(loaded.mid === true, 'string "yes" ignored, mid stays default');
    assert(loaded.pat === false, 'number 1 ignored, pat stays default');
});

await test('loadFormatsFromStorage ignores unknown keys in stored blob', () => {
    const storage = makeStorage();
    storage.setItem(__private.FORMATS_STORAGE_KEY, JSON.stringify({ mid: false, legacy_foo: true }));
    const loaded = loadFormatsFromStorage(storage);
    assert(!('legacy_foo' in loaded), 'unknown key not present in result');
    assert(loaded.mid === false, 'mid still applied');
});

await test('saveFormatsToStorage writes only known keys', () => {
    const storage = makeStorage();
    saveFormatsToStorage({ mid: false, pat: true, bogus: 'ignored' }, storage);
    const raw = storage.getItem(__private.FORMATS_STORAGE_KEY);
    const parsed = JSON.parse(raw);
    assert(Object.keys(parsed).length === Object.keys(DEFAULT_FORMATS).length, 'only known keys persisted');
    assert(parsed.mid === false, 'mid persisted as false');
    assert(parsed.pat === true, 'pat persisted as true');
    assert(!('bogus' in parsed), 'bogus key excluded');
});

await test('saveFormatsToStorage coerces truthy/falsy to strict booleans', () => {
    const storage = makeStorage();
    saveFormatsToStorage({ mid: 1, pat: 0, rbs: 'x', json: null }, storage);
    const parsed = JSON.parse(storage.getItem(__private.FORMATS_STORAGE_KEY));
    assert(parsed.mid === true, 'truthy → true');
    assert(parsed.pat === false, 'falsy → false');
    assert(parsed.rbs === true, 'string → true');
    assert(parsed.json === false, 'null → false');
});

await test('save/load roundtrip is stable', () => {
    const storage = makeStorage();
    const mine = { ...DEFAULT_FORMATS, mid: false, pat: true, combined_sqs: true };
    saveFormatsToStorage(mine, storage);
    const loaded = loadFormatsFromStorage(storage);
    for (const k of Object.keys(DEFAULT_FORMATS)) {
        assert(loaded[k] === mine[k], `${k} roundtrips (${loaded[k]} === ${mine[k]})`);
    }
});

await test('anyFormatSelected false for undefined/null/all-false', () => {
    assert(anyFormatSelected(null) === false, 'null → false');
    assert(anyFormatSelected(undefined) === false, 'undefined → false');
    const allOff = {};
    for (const k of Object.keys(DEFAULT_FORMATS)) allOff[k] = false;
    assert(anyFormatSelected(allOff) === false, 'all-off → false');
});

await test('anyFormatSelected true when only a per-pattern format is ticked', () => {
    const only = {};
    for (const k of Object.keys(DEFAULT_FORMATS)) only[k] = false;
    only.mid = true;
    assert(anyFormatSelected(only) === true, 'mid alone → true');
});

await test('anyFormatSelected true when only combined format is ticked', () => {
    const only = {};
    for (const k of Object.keys(DEFAULT_FORMATS)) only[k] = false;
    only.combined_rbs = true;
    assert(anyFormatSelected(only) === true, 'combined_rbs alone → true');
});

await test('buildExportPayload constructs camelCase envelope', () => {
    const payload = buildExportPayload({
        packageId: 'pkg_test',
        label: 'Random Progression',
        scaleName: 'natural_minor',
        acidPatterns: four(),
        basslines: four(),
        formats: { ...DEFAULT_FORMATS },
    });
    assert(payload.packageId === 'pkg_test', 'packageId preserved');
    assert(payload.label === 'Random Progression', 'label preserved');
    assert(payload.scaleName === 'natural_minor', 'scaleName preserved');
    assert(Array.isArray(payload.formats), 'formats is array');
    assert(payload.formats.length === 3, 'only 3 default per-pattern formats');
    assert(payload.formats.includes('mid'), 'includes mid');
    assert(payload.formats.includes('steps_txt'), 'includes steps_txt');
    assert(payload.formats.includes('seq'), 'includes seq');
    assert(!payload.formats.includes('pat'), 'excludes pat');
    assert(payload.combinedFormats && typeof payload.combinedFormats === 'object', 'combinedFormats object');
    assert(payload.combinedFormats.rbs === false, 'combined rbs off by default');
    assert(payload.combinedFormats.sqs === false, 'combined sqs off by default');
    assert(Array.isArray(payload.acidPatterns) && payload.acidPatterns.length === 4, '4 acid patterns');
    assert(Array.isArray(payload.basslines) && payload.basslines.length === 4, '4 basslines');
});

await test('buildExportPayload accepts combined-only selection', () => {
    const formats = {};
    for (const k of Object.keys(DEFAULT_FORMATS)) formats[k] = false;
    formats.combined_rbs = true;
    const payload = buildExportPayload({
        packageId: 'pkg_combined',
        label: 'x',
        scaleName: 'x',
        acidPatterns: four(),
        basslines: four(),
        formats,
    });
    assert(payload.formats.length === 0, 'no per-pattern formats');
    assert(payload.combinedFormats.rbs === true, 'combined rbs on');
    assert(payload.combinedFormats.sqs === false, 'combined sqs off');
});

await test('buildExportPayload rejects empty selection', () => {
    const formats = {};
    for (const k of Object.keys(DEFAULT_FORMATS)) formats[k] = false;
    let threw = false;
    try {
        buildExportPayload({
            packageId: 'pkg',
            label: 'x',
            scaleName: 'x',
            acidPatterns: four(),
            basslines: four(),
            formats,
        });
    } catch (e) {
        threw = true;
        assert(/at least one format/i.test(e.message), 'error message mentions at-least-one');
    }
    assert(threw, 'empty selection throws');
});

await test('buildExportPayload rejects missing packageId', () => {
    let threw = false;
    try {
        buildExportPayload({
            packageId: '',
            label: 'x',
            scaleName: 'x',
            acidPatterns: four(),
            basslines: four(),
            formats: { ...DEFAULT_FORMATS },
        });
    } catch (e) { threw = true; }
    assert(threw, 'empty packageId throws');
});

await test('buildExportPayload rejects wrong pattern count', () => {
    let threw1 = false, threw2 = false;
    try {
        buildExportPayload({
            packageId: 'pkg',
            label: 'x',
            scaleName: 'x',
            acidPatterns: [defaultPattern(), defaultPattern()],
            basslines: four(),
            formats: { ...DEFAULT_FORMATS },
        });
    } catch (e) { threw1 = true; }
    try {
        buildExportPayload({
            packageId: 'pkg',
            label: 'x',
            scaleName: 'x',
            acidPatterns: four(),
            basslines: [defaultPattern()],
            formats: { ...DEFAULT_FORMATS },
        });
    } catch (e) { threw2 = true; }
    assert(threw1, 'wrong acid length throws');
    assert(threw2, 'wrong bassline length throws');
});

await test('buildExportPayload passes basslinesFull (length 20) through envelope', () => {
    const full = twenty();
    const payload = buildExportPayload({
        packageId: 'pkg',
        scaleName: 'x',
        acidPatterns: four(),
        basslines: four(),
        basslinesFull: full,
        formats: { ...DEFAULT_FORMATS },
    });
    assert(Array.isArray(payload.basslinesFull), 'basslinesFull present');
    assert(payload.basslinesFull.length === 20, 'basslinesFull length 20');
    assert(payload.basslinesFull[0] === full[0], 'basslinesFull entries forwarded by reference');
});

await test('buildExportPayload omits basslinesFull key when not provided', () => {
    const payload = buildExportPayload({
        packageId: 'pkg',
        scaleName: 'x',
        acidPatterns: four(),
        basslines: four(),
        formats: { ...DEFAULT_FORMATS },
    });
    assert(!('basslinesFull' in payload), 'basslinesFull omitted when absent');
});

await test('buildExportPayload rejects basslinesFull with wrong length', () => {
    let threw = false;
    try {
        buildExportPayload({
            packageId: 'pkg',
            scaleName: 'x',
            acidPatterns: four(),
            basslines: four(),
            basslinesFull: Array.from({ length: 19 }, defaultPattern),
            formats: { ...DEFAULT_FORMATS },
        });
    } catch (e) {
        threw = true;
        assert(/length 20/.test(e.message), 'error mentions length 20');
    }
    assert(threw, 'wrong basslinesFull length throws');
});

await test('setPackage forwards basslinesFull into export payload on save', async () => {
    const ui = installDom();
    const payloads = [];
    init({
        setStatus: () => {},
        exportFn: async (payload) => { payloads.push(payload); return { ok: true, zipName: 'x.zip' }; },
    });
    clearPackage();
    const full = twenty();
    setPackage({
        packageId: 'pkg_full',
        label: 'with full matrix',
        scaleName: 'minor',
        acidPatterns: four(),
        basslines: four(),
        basslinesFull: full,
    });
    ui.btnSave.click();
    await Promise.resolve();
    await Promise.resolve();
    assert(payloads.length === 1, 'export fired once');
    assert(Array.isArray(payloads[0].basslinesFull), 'basslinesFull flowed through');
    assert(payloads[0].basslinesFull.length === 20, 'basslinesFull length 20 on the wire');
});

await test('setPackage drops malformed basslinesFull (wrong length)', async () => {
    const ui = installDom();
    const payloads = [];
    init({
        setStatus: () => {},
        exportFn: async (payload) => { payloads.push(payload); return { ok: true, zipName: 'x.zip' }; },
    });
    clearPackage();
    setPackage({
        packageId: 'pkg_bad',
        label: 'malformed',
        scaleName: 'minor',
        acidPatterns: four(),
        basslines: four(),
        basslinesFull: Array.from({ length: 18 }, defaultPattern),
    });
    ui.btnSave.click();
    await Promise.resolve();
    await Promise.resolve();
    assert(payloads.length === 1, 'export still fired');
    assert(!('basslinesFull' in payloads[0]), 'malformed basslinesFull dropped silently');
});

await test('buildExportPayload tolerates missing scaleName/label', () => {
    const payload = buildExportPayload({
        packageId: 'pkg',
        acidPatterns: four(),
        basslines: four(),
        formats: { ...DEFAULT_FORMATS },
    });
    assert(payload.scaleName === '', 'missing scaleName becomes empty string');
    assert(payload.label === '', 'missing label becomes empty string');
});

await test('init renders cold-load defaults and disabled save button state', () => {
    const ui = installDom();
    init({
        setStatus: () => {},
        exportFn: async () => ({ ok: true, zipName: 'unused.zip' }),
    });
    clearPackage();

    assert(ui.pkgStatus.textContent === 'No package', 'cold load shows No package');
    assert(ui.pkgLastSaved.textContent === '', 'pkg-last-saved starts empty');
    assert(ui.btnSave.disabled === true, 'save button disabled with no package');
    assert(ui.btnSave.title === 'Generate a progression first', 'no-package tooltip has priority');
    assert(ui.checkbox('mid').checked === true, 'mid checked by default');
    assert(ui.checkbox('steps_txt').checked === true, 'steps_txt checked by default');
    assert(ui.checkbox('seq').checked === true, 'seq checked by default');
    assert(ui.checkbox('pat').checked === false, 'pat unchecked by default');
    assert(ui.checkbox('combined_sqs').checked === false, 'combined_sqs unchecked by default');
});

await test('init restores checkbox state from localStorage', () => {
    const ui = installDom({
        [__private.FORMATS_STORAGE_KEY]: JSON.stringify({
            ...DEFAULT_FORMATS,
            seq: false,
            pat: true,
            combined_sqs: true,
        }),
    });
    init({ setStatus: () => {}, exportFn: async () => ({ ok: true, zipName: 'unused.zip' }) });
    clearPackage();

    assert(ui.checkbox('mid').checked === true, 'mid restored on');
    assert(ui.checkbox('steps_txt').checked === true, 'steps_txt restored on');
    assert(ui.checkbox('seq').checked === false, 'seq restored off');
    assert(ui.checkbox('pat').checked === true, 'pat restored on');
    assert(ui.checkbox('combined_sqs').checked === true, 'combined_sqs restored on');
});

await test('setPackage enables save when any format is selected and label renders', () => {
    const ui = installDom();
    init({ setStatus: () => {}, exportFn: async () => ({ ok: true, zipName: 'unused.zip' }) });
    clearPackage();

    setPackage({
        packageId: 'pkg_1',
        label: 'A minor progression',
        scaleName: 'natural_minor',
        acidPatterns: four(),
        basslines: four(),
    });

    assert(ui.pkgStatus.textContent === 'A minor progression', 'package label rendered');
    assert(ui.btnSave.disabled === false, 'save enabled with package + default formats');
    assert(ui.btnSave.title === 'Save package as ZIP', 'enabled tooltip');
});

await test('no-format tooltip appears only when a package exists', () => {
    const ui = installDom();
    init({ setStatus: () => {}, exportFn: async () => ({ ok: true, zipName: 'unused.zip' }) });
    clearPackage();

    for (const k of Object.keys(DEFAULT_FORMATS)) {
        const cb = ui.checkbox(k);
        cb.checked = false;
        cb.change();
    }
    assert(ui.btnSave.title === 'Generate a progression first', 'no-package tooltip still wins');

    setPackage({
        packageId: 'pkg_2',
        label: 'Combined only',
        scaleName: 'minor',
        acidPatterns: four(),
        basslines: four(),
    });
    assert(ui.btnSave.disabled === true, 'save disabled when package exists but no formats selected');
    assert(ui.btnSave.title === 'Select at least one format', 'no-format tooltip after package set');

    ui.checkbox('combined_rbs').checked = true;
    ui.checkbox('combined_rbs').change();
    assert(ui.btnSave.disabled === false, 'combined-only selection enables save');
});

await test('save click disables button in flight and records last-saved name on success', async () => {
    const ui = installDom();
    const statuses = [];
    const payloads = [];
    let release;
    const exportPromise = new Promise((resolve) => { release = resolve; });

    init({
        setStatus: (msg) => { statuses.push(msg); },
        exportFn: async (payload) => {
            payloads.push(payload);
            await exportPromise;
            return { ok: true, zipName: 'PG_test-natural_minor-Random_Progression_Package.zip' };
        },
    });
    clearPackage();
    setPackage({
        packageId: 'pkg_3',
        label: 'Fresh package',
        scaleName: 'natural_minor',
        acidPatterns: four(),
        basslines: four(),
    });

    ui.btnSave.click();
    await Promise.resolve();

    assert(ui.btnSave.disabled === true, 'save disabled while export is running');
    assert(ui.btnSave.title === 'Exporting…', 'in-flight tooltip');
    assert(statuses[0] === 'Exporting...', 'exporting status shown immediately');
    assert(payloads.length === 1, 'export function called once');
    assert(payloads[0].packageId === 'pkg_3', 'payload contains packageId');
    assert(payloads[0].formats.join(',') === 'mid,steps_txt,seq', 'payload uses default formats');

    release();
    await Promise.resolve();
    await Promise.resolve();

    assert(ui.btnSave.disabled === false, 'save re-enabled after success');
    assert(ui.btnSave.title === 'Save package as ZIP', 'tooltip restored after success');
    assert(statuses[1] === 'Saved: PG_test-natural_minor-Random_Progression_Package.zip', 'saved status shown');
    assert(ui.pkgLastSaved.textContent === 'PG_test-natural_minor-Random_Progression_Package.zip', 'last-saved updated');
});

await test('save click reports export failures and does not fire while disabled', async () => {
    const ui = installDom();
    const statuses = [];
    let calls = 0;

    init({
        setStatus: (msg) => { statuses.push(msg); },
        exportFn: async () => {
            calls++;
            throw new Error('disk full');
        },
    });
    clearPackage();

    ui.btnSave.click();
    assert(calls === 0, 'disabled click with no package does not call exportFn');

    setPackage({
        packageId: 'pkg_4',
        label: 'Broken export',
        scaleName: 'minor',
        acidPatterns: four(),
        basslines: four(),
    });

    ui.btnSave.click();
    await Promise.resolve();
    await Promise.resolve();

    assert(calls === 1, 'enabled click calls exportFn once');
    assert(statuses[0] === 'Exporting...', 'exporting status emitted before failure');
    assert(statuses[1] === 'Export failed: disk full', 'failure status surfaced');
    assert(ui.btnSave.disabled === false, 'button re-enabled after failure');
    assert(ui.pkgLastSaved.textContent === '', 'last-saved remains empty after failure');
});

// --- Summary ---

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
