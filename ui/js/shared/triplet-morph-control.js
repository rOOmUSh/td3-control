// TRIPLET morph knob: Control-page NO-LIVE audition amount, an integer
// 0 through 100. The amount is transient session state owned by the
// multipattern state module; this module provides only the knob
// mechanics, label, display, and visibility wiring. PageUp and
// PageDown step by 10; all other interaction matches GATE.

import { createPercentKnob, normalizePercentValue } from './percent-knob.js';

export const MIN_TRIPLET_MORPH_PERCENT = 0;
export const MAX_TRIPLET_MORPH_PERCENT = 100;
export const DEFAULT_TRIPLET_MORPH_PERCENT = 0;
export const TRIPLET_MORPH_PAGE_STEP = 10;

const TRIPLET_MORPH_BOUNDS = {
    min: MIN_TRIPLET_MORPH_PERCENT,
    max: MAX_TRIPLET_MORPH_PERCENT,
    fallback: DEFAULT_TRIPLET_MORPH_PERCENT,
};

export function normalizeTripletMorphPercent(value) {
    return normalizePercentValue(value, TRIPLET_MORPH_BOUNDS);
}

export function createTripletMorphControl({
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
        ...TRIPLET_MORPH_BOUNDS,
        ariaLabel: 'Triplet morph',
        pageStep: TRIPLET_MORPH_PAGE_STEP,
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

export function initTripletMorphControl({
    getValue,
    setValue,
    isVisible,
    onValueChange,
    documentRef = globalThis.document,
} = {}) {
    return createTripletMorphControl({
        root: documentRef?.getElementById('triplet-morph-controls'),
        display: documentRef?.getElementById('triplet-morph-display'),
        knob: documentRef?.getElementById('triplet-morph-knob'),
        indicator: documentRef?.getElementById('triplet-morph-indicator'),
        eventTarget: documentRef,
        getValue,
        setValue,
        isVisible,
        onValueChange,
    });
}
