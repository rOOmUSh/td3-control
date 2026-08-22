// Transport-bar CUTOFF knob for a TD-3-MO.
//
// The value is the CC 74 data byte, 0 through 127, and is sent to the
// device through `/api/device/filter-cutoff`. The control is shown only
// while the connected device reports support for device controls.

import { createPercentKnob, normalizePercentValue } from './percent-knob.js';

export const FILTER_CUTOFF_STORAGE_KEY = 'td3_filter_cutoff';
export const DEFAULT_FILTER_CUTOFF = 64;
export const MIN_FILTER_CUTOFF = 0;
export const MAX_FILTER_CUTOFF = 127;

const CUTOFF_BOUNDS = {
    min: MIN_FILTER_CUTOFF,
    max: MAX_FILTER_CUTOFF,
    fallback: DEFAULT_FILTER_CUTOFF,
};

export function normalizeFilterCutoff(value) {
    return normalizePercentValue(value, CUTOFF_BOUNDS);
}

export function formatFilterCutoff(value) {
    return String(normalizeFilterCutoff(value));
}

export function readFilterCutoff(storage) {
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        const raw = target?.getItem(FILTER_CUTOFF_STORAGE_KEY);
        if (raw === null || raw === undefined || raw === '') return DEFAULT_FILTER_CUTOFF;
        const value = Number(raw);
        if (!Number.isInteger(value)
            || value < MIN_FILTER_CUTOFF
            || value > MAX_FILTER_CUTOFF) {
            return DEFAULT_FILTER_CUTOFF;
        }
        return value;
    } catch (_) {
        return DEFAULT_FILTER_CUTOFF;
    }
}

export function writeFilterCutoff(value, storage) {
    const normalized = normalizeFilterCutoff(value);
    try {
        const target = storage === undefined ? globalThis.sessionStorage : storage;
        target?.setItem(FILTER_CUTOFF_STORAGE_KEY, String(normalized));
    } catch (_) { /* unavailable or quota exceeded */ }
    return normalized;
}

export function createFilterCutoffControl({
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
        ...CUTOFF_BOUNDS,
        ariaLabel: 'Filter cutoff',
        pageStep: 8,
        formatValue: formatFilterCutoff,
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

export function initFilterCutoffControl({
    getValue,
    setValue,
    isVisible,
    onValueChange,
    documentRef = globalThis.document,
} = {}) {
    return createFilterCutoffControl({
        root: documentRef?.getElementById('filter-cutoff-controls'),
        display: documentRef?.getElementById('filter-cutoff-display'),
        knob: documentRef?.getElementById('filter-cutoff-knob'),
        indicator: documentRef?.getElementById('filter-cutoff-indicator'),
        eventTarget: documentRef,
        getValue,
        setValue,
        isVisible,
        onValueChange,
    });
}
