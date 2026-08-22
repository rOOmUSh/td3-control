// Transport-bar BEND knob for a TD-3-MO.
//
// The value is the 14-bit Pitch Bend amount, 0 through 16383 with 8192
// as the no-offset center, and is sent through `/api/device/pitch-bend`.
// The readout shows the signed offset from center. The knob holds its
// position; a double-click or Enter returns it to center. Shown only
// while the connected device reports support for device controls.

import { createPercentKnob, normalizePercentValue } from './percent-knob.js';

export const PITCH_BEND_STORAGE_KEY = 'td3_pitch_bend';
export const MIN_PITCH_BEND = 0;
export const MAX_PITCH_BEND = 16383;
export const PITCH_BEND_CENTER = 8192;
/** Change per wheel notch, arrow key, or 3 pixels of drag. */
export const PITCH_BEND_STEP = 128;
export const PITCH_BEND_PAGE_STEP = 1024;

const BEND_BOUNDS = {
    min: MIN_PITCH_BEND,
    max: MAX_PITCH_BEND,
    fallback: PITCH_BEND_CENTER,
};

export function normalizePitchBend(value) {
    return normalizePercentValue(value, BEND_BOUNDS);
}

/** Signed offset from center: `0` at center, otherwise `+N` or `-N`. */
export function formatPitchBend(value) {
    const offset = normalizePitchBend(value) - PITCH_BEND_CENTER;
    if (offset === 0) return '0';
    return offset > 0 ? `+${offset}` : String(offset);
}

export function readPitchBend(storage) {
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        const raw = target?.getItem(PITCH_BEND_STORAGE_KEY);
        if (raw === null || raw === undefined || raw === '') return PITCH_BEND_CENTER;
        const value = Number(raw);
        if (!Number.isInteger(value)
            || value < MIN_PITCH_BEND
            || value > MAX_PITCH_BEND) {
            return PITCH_BEND_CENTER;
        }
        return value;
    } catch (_) {
        return PITCH_BEND_CENTER;
    }
}

export function writePitchBend(value, storage) {
    const normalized = normalizePitchBend(value);
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        target?.setItem(PITCH_BEND_STORAGE_KEY, String(normalized));
    } catch (_) { /* unavailable or quota exceeded */ }
    return normalized;
}

export function createPitchBendControl({
    root,
    display,
    knob,
    indicator,
    eventTarget,
    getValue,
    setValue,
    isVisible = () => true,
    onValueChange = () => {},
}) {
    return createPercentKnob({
        ...BEND_BOUNDS,
        ariaLabel: 'Pitch bend',
        step: PITCH_BEND_STEP,
        pageStep: PITCH_BEND_PAGE_STEP,
        resetValue: PITCH_BEND_CENTER,
        formatValue: formatPitchBend,
        root,
        display,
        knob,
        indicator,
        eventTarget,
        getValue,
        setValue,
        isVisible,
        onValueChange,
    });
}

export function initPitchBendControl({
    getValue,
    setValue,
    isVisible,
    onValueChange,
    documentRef = globalThis.document,
} = {}) {
    return createPitchBendControl({
        root: documentRef?.getElementById('pitch-bend-controls'),
        display: documentRef?.getElementById('pitch-bend-display'),
        knob: documentRef?.getElementById('pitch-bend-knob'),
        indicator: documentRef?.getElementById('pitch-bend-indicator'),
        eventTarget: documentRef,
        getValue,
        setValue,
        isVisible,
        onValueChange,
    });
}
