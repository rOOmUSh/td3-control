// Usage: node ui/js/shared/midi-channel-control.test.js
//
// The device only sounds channel-voice messages on its own channel, so a
// wrong value here is silent playback with no error. These cover the
// coercion of untrusted stored input, the startup fallback chain, and
// the control's visibility and change handling.

const storage = new Map();
globalThis.sessionStorage = {
    getItem: (k) => (storage.has(k) ? storage.get(k) : null),
    setItem: (k, v) => { storage.set(k, String(v)); },
    removeItem: (k) => { storage.delete(k); },
};

const {
    MIDI_CHANNEL_STORAGE_KEY,
    createMidiChannelControl,
    normalizeMidiChannel,
    readMidiChannel,
    writeMidiChannel,
} = await import('./midi-channel-control.js');

let passed = 0;
let failed = 0;

function check(condition, message) {
    if (condition) { passed += 1; return; }
    failed += 1;
    console.error(`  FAIL: ${message}`);
}

// --- normalization -------------------------------------------------

check(normalizeMidiChannel(1) === 1, 'channel 1 survives');
check(normalizeMidiChannel(16) === 16, 'channel 16 survives');
check(normalizeMidiChannel('3') === 3, 'numeric strings parse');
check(normalizeMidiChannel(0) === 1, 'channel 0 falls back');
check(normalizeMidiChannel(17) === 1, 'channel 17 falls back');
check(normalizeMidiChannel(-4) === 1, 'negative falls back');
check(normalizeMidiChannel(2.5) === 1, 'non-integer falls back');
check(normalizeMidiChannel('abc') === 1, 'junk falls back');
check(normalizeMidiChannel(undefined) === 1, 'undefined falls back');
check(normalizeMidiChannel(null) === 1, 'null falls back');
check(normalizeMidiChannel(99, 7) === 7, 'an explicit fallback is honoured');
check(normalizeMidiChannel(99, 42) === 1,
    'an out-of-range fallback cannot smuggle a bad channel through');

// --- storage --------------------------------------------------------

storage.clear();
check(readMidiChannel() === 1, 'empty storage yields the built-in default');
check(readMidiChannel(3) === 3,
    'empty storage yields the configured MIDI_DEVICE_CHANNEL');

writeMidiChannel(9);
check(storage.get(MIDI_CHANNEL_STORAGE_KEY) === '9', 'the value is persisted');
check(readMidiChannel(3) === 9, 'a stored value outranks the configured default');

storage.set(MIDI_CHANNEL_STORAGE_KEY, '77');
check(readMidiChannel(3) === 3, 'a corrupt stored value falls back, it is not clamped to 16');
storage.set(MIDI_CHANNEL_STORAGE_KEY, '');
check(readMidiChannel(5) === 5, 'an empty stored value falls back');

check(writeMidiChannel(0) === 1, 'writing an out-of-range value stores the fallback');
check(storage.get(MIDI_CHANNEL_STORAGE_KEY) === '1', 'the fallback is what lands in storage');

// A storage backend that throws must not take the page down.
check(readMidiChannel(4, {
    getItem() { throw new Error('denied'); },
}) === 4, 'a throwing storage read falls back');
writeMidiChannel(6, { setItem() { throw new Error('denied'); } });

// --- control --------------------------------------------------------

function fakeSelect() {
    const listeners = {};
    const node = {
        options: [],
        value: '',
        attributes: {},
        textContent: '',
        ownerDocument: {
            createElement: () => ({ value: '', textContent: '' }),
        },
        appendChild(option) { node.options.push(option); },
        setAttribute(name, value) { node.attributes[name] = value; },
        addEventListener(name, fn) { listeners[name] = fn; },
        fire(name) { listeners[name]?.(); },
    };
    return node;
}

function fakeRoot() {
    const classes = new Set(['hidden']);
    return {
        classList: {
            toggle: (name, on) => (on ? classes.add(name) : classes.delete(name)),
            contains: (name) => classes.has(name),
        },
    };
}

let current = 1;
let visible = true;
const changes = [];
const select = fakeSelect();
const root = fakeRoot();
const control = createMidiChannelControl({
    root,
    select,
    getValue: () => current,
    setValue: (value) => { current = value; },
    isVisible: () => visible,
    onValueChange: (value) => changes.push(value),
});

check(select.options.length === 16, 'all sixteen channels are offered');
check(select.options[0].value === '1' && select.options[15].value === '16',
    'the options span 1 through 16');
check(select.value === '1', 'the control shows the current channel');
check(root.classList.contains('hidden') === false, 'a visible control is not hidden');

select.value = '3';
select.fire('change');
check(current === 3, 'choosing a channel updates the owning state');
check(changes.length === 1 && changes[0] === 3, 'a real change notifies once');

select.fire('change');
check(changes.length === 1, 'reselecting the same channel does not notify again');

select.value = '99';
select.fire('change');
check(current === 3, 'an impossible selection leaves the channel alone');
check(select.value === '3', 'the control snaps back to the real value');

check(root.classList.contains('flex'), 'a visible control lays out as flex');

visible = false;
control.render();
check(root.classList.contains('hidden'), 'LIVE mode hides the control');
check(root.classList.contains('flex') === false,
    'the flex display utility is dropped so `hidden` actually hides it');
visible = true;
control.render();
check(root.classList.contains('hidden') === false, 'NO-LIVE mode shows it again');
check(root.classList.contains('flex'), 'and lays out as flex again');

// Missing markup must not throw: a page without the control still loads.
const headless = createMidiChannelControl({
    root: null,
    select: null,
    getValue: () => 1,
    setValue: () => {},
});
headless.render();
check(true, 'a page without the control markup still initialises');

console.log(`midi-channel-control tests: ${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
