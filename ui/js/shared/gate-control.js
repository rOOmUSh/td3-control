import { createPercentKnob, normalizePercentValue } from './percent-knob.js';

export const GATE_STORAGE_KEY = 'td3_gate_percent';
/**
 * The key this control used when it was called DECAY. A session written
 * before the rename still carries the user's setting under it, so the
 * read falls back to it once and the next write moves the value across.
 */
export const LEGACY_GATE_STORAGE_KEY = 'td3_note_decay_percent';
export const DEFAULT_GATE_PERCENT = 50;
export const MIN_GATE_PERCENT = 1;
export const MAX_GATE_PERCENT = 100;

const GATE_BOUNDS = {
    min: MIN_GATE_PERCENT,
    max: MAX_GATE_PERCENT,
    fallback: DEFAULT_GATE_PERCENT,
};

export function normalizeGatePercent(value) {
    return normalizePercentValue(value, GATE_BOUNDS);
}

export function readGatePercent(storage) {
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        let raw = target?.getItem(GATE_STORAGE_KEY);
        if (raw === null || raw === undefined || raw === '') {
            raw = target?.getItem(LEGACY_GATE_STORAGE_KEY);
        }
        if (raw === null || raw === undefined || raw === '') return DEFAULT_GATE_PERCENT;
        const value = Number(raw);
        if (!Number.isInteger(value)
            || value < MIN_GATE_PERCENT
            || value > MAX_GATE_PERCENT) {
            return DEFAULT_GATE_PERCENT;
        }
        return value;
    } catch (_) {
        return DEFAULT_GATE_PERCENT;
    }
}

export function writeGatePercent(value, storage) {
    const normalized = normalizeGatePercent(value);
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        target?.setItem(GATE_STORAGE_KEY, String(normalized));
    } catch (_) { /* unavailable or quota exceeded */ }
    return normalized;
}

export function createGateControl({
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
        ...GATE_BOUNDS,
        ariaLabel: 'Gate',
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

export function initGateControl({
    getValue,
    setValue,
    isVisible,
    onValueChange,
    documentRef = globalThis.document,
} = {}) {
    return createGateControl({
        root: documentRef?.getElementById('gate-controls'),
        display: documentRef?.getElementById('gate-display'),
        knob: documentRef?.getElementById('gate-knob'),
        indicator: documentRef?.getElementById('gate-indicator'),
        eventTarget: documentRef,
        getValue,
        setValue,
        isVisible,
        onValueChange,
    });
}
