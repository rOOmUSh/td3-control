// Transport-bar selector for the MIDI channel the connected TD-3
// listens on.
//
// Host audition and the keyboard note preview address the device with
// channel-voice messages, which a TD-3 discards unless they carry its
// own channel. The channel is set on the device (SynthTribe) and the
// device does not report it back in a form this app can trust, so it
// has to be told. `MIDI_DEVICE_CHANNEL` supplies the startup value and
// this control overrides it for the session without a restart.

export const MIDI_CHANNEL_STORAGE_KEY = 'td3_midi_channel';
export const MIN_MIDI_CHANNEL = 1;
export const MAX_MIDI_CHANNEL = 16;
export const FALLBACK_MIDI_CHANNEL = 1;

function inRange(value) {
    return Number.isInteger(value)
        && value >= MIN_MIDI_CHANNEL
        && value <= MAX_MIDI_CHANNEL;
}

/**
 * Coerce any input to a channel in 1..16, falling back to `fallback`.
 * Non-integers, out-of-range values and junk all resolve rather than
 * throw: a stored value is untrusted input like any other.
 *
 * The return is always a valid channel. `fallback` is itself checked, so
 * a caller passing a bad default cannot push an out-of-range channel
 * through to the status nibble.
 */
export function normalizeMidiChannel(value, fallback = FALLBACK_MIDI_CHANNEL) {
    const numeric = typeof value === 'number' ? value : parseInt(value, 10);
    if (inRange(numeric)) return numeric;
    const numericFallback = typeof fallback === 'number'
        ? fallback
        : parseInt(fallback, 10);
    return inRange(numericFallback) ? numericFallback : FALLBACK_MIDI_CHANNEL;
}

export function readMidiChannel(fallback = FALLBACK_MIDI_CHANNEL, storage) {
    const safeFallback = normalizeMidiChannel(fallback, FALLBACK_MIDI_CHANNEL);
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        const raw = target?.getItem(MIDI_CHANNEL_STORAGE_KEY);
        if (raw === null || raw === undefined || raw === '') return safeFallback;
        return normalizeMidiChannel(raw, safeFallback);
    } catch (_) {
        return safeFallback;
    }
}

export function writeMidiChannel(value, storage) {
    const normalized = normalizeMidiChannel(value);
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        target?.setItem(MIDI_CHANNEL_STORAGE_KEY, String(normalized));
    } catch (_) { /* unavailable or quota exceeded */ }
    return normalized;
}

/**
 * Bind the CH selector.
 *
 * `render` reconciles the option list, the shown value and visibility
 * from the owning state, so callers refresh it the same way they
 * refresh the GATE knob. Returns `{ render }`; with no `select` element
 * present every call is a no-op so a page without the control in its
 * markup still loads.
 */
export function createMidiChannelControl({
    root,
    select,
    getValue,
    setValue,
    isVisible = () => true,
    onValueChange = () => {},
}) {
    function populate() {
        if (!select || select.options.length === MAX_MIDI_CHANNEL) return;
        select.textContent = '';
        for (let channel = MIN_MIDI_CHANNEL; channel <= MAX_MIDI_CHANNEL; channel += 1) {
            const option = select.ownerDocument.createElement('option');
            option.value = String(channel);
            option.textContent = String(channel);
            select.appendChild(option);
        }
    }

    function render() {
        if (root) {
            // `hidden` and `flex` are both display utilities, so the
            // container carries exactly one of them at a time. This is how
            // the neighbouring REMOTE block shows and hides itself.
            const visible = isVisible();
            root.classList.toggle('hidden', !visible);
            root.classList.toggle('flex', visible);
        }
        if (!select) return;
        populate();
        const current = normalizeMidiChannel(getValue());
        const asText = String(current);
        if (select.value !== asText) select.value = asText;
        select.setAttribute('aria-valuenow', asText);
    }

    if (select) {
        populate();
        select.addEventListener('change', () => {
            const before = normalizeMidiChannel(getValue());
            const next = normalizeMidiChannel(select.value, before);
            setValue(next);
            render();
            if (next !== before) onValueChange(next);
        });
    }

    render();
    return { render };
}

export function initMidiChannelControl({
    getValue,
    setValue,
    isVisible,
    onValueChange,
    documentRef = globalThis.document,
} = {}) {
    return createMidiChannelControl({
        root: documentRef?.getElementById('midi-channel-controls'),
        select: documentRef?.getElementById('midi-channel-select'),
        getValue,
        setValue,
        isVisible,
        onValueChange,
    });
}
