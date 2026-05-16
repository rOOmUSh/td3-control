// Usage: node ui/js/magic-randomizer/magic-state.test.js
//
// Verifies the localStorage-backed flag that toggles the MAGIC randomizer
// pipeline. Polyfills window/localStorage for Node.

if (typeof globalThis.window === 'undefined') {
    const store = new Map();
    globalThis.window = {
        localStorage: {
            getItem: (k) => (store.has(k) ? store.get(k) : null),
            setItem: (k, v) => { store.set(k, String(v)); },
            removeItem: (k) => { store.delete(k); },
            clear: () => { store.clear(); },
        },
    };
}

const { isMagicEnabled, setMagicEnabled, _resetMagicCache } = await import('./magic-state.js');

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; return; }
    passed++;
}
function test(name, fn) {
    try { fn(); console.log(`  ok: ${name}`); }
    catch (e) { console.error(`  FAIL: ${name}: ${e.stack || e.message}`); failed++; }
}

function reset() {
    window.localStorage.clear();
    _resetMagicCache();
}

test('defaults to disabled when nothing stored', () => {
    reset();
    assert(isMagicEnabled() === false, 'isMagicEnabled() should default to false');
});

test('enabling persists to localStorage', () => {
    reset();
    setMagicEnabled(true);
    _resetMagicCache();
    assert(isMagicEnabled() === true, 'enabled state should survive cache reset');
    assert(window.localStorage.getItem('td3_magic_randomizer') === '1', 'storage should hold "1"');
});

test('disabling persists to localStorage', () => {
    reset();
    setMagicEnabled(true);
    setMagicEnabled(false);
    _resetMagicCache();
    assert(isMagicEnabled() === false, 'disabled state should survive cache reset');
});

test('coerces truthy values to true', () => {
    reset();
    setMagicEnabled('yes');
    assert(isMagicEnabled() === true, 'truthy value should coerce to true');
});

test('coerces falsy values to false', () => {
    reset();
    setMagicEnabled(true);
    setMagicEnabled(0);
    assert(isMagicEnabled() === false, 'falsy value should coerce to false');
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
