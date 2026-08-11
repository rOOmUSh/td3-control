// Shared percent-knob interaction mechanics: clamp, wheel, vertical
// drag, keyboard, ARIA, and visibility. Value storage and labels stay
// with each concrete control module.

const DRAG_PIXELS_PER_UNIT = 3;

export function normalizePercentValue(value, { min, max, fallback }) {
    const numeric = typeof value === 'number' ? value : Number(value);
    const safe = Number.isFinite(numeric) ? Math.round(numeric) : fallback;
    return Math.max(min, Math.min(max, safe));
}

// Builds a knob controller over injected DOM handles. Any missing
// element or accessor yields an inert controller so pages without the
// markup can share the module. `pageStep` of 0 disables PageUp and
// PageDown handling.
export function createPercentKnob({
    min,
    max,
    fallback,
    ariaLabel,
    pageStep = 0,
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
    if (!root || !display || !knob || !indicator || !eventTarget
        || typeof getValue !== 'function' || typeof setValue !== 'function') {
        return { render() {}, destroy() {} };
    }

    const bounds = { min, max, fallback };
    let dragStartY = null;
    let dragStartValue = null;

    function indicatorAngle(value) {
        return ((value - min) / (max - min)) * 300 - 150;
    }

    function render() {
        const value = normalizePercentValue(getValue(), bounds);
        display.textContent = `${value}%`;
        indicator.style.transform = `rotate(${indicatorAngle(value)}deg)`;
        knob.setAttribute('tabindex', '0');
        knob.setAttribute('role', 'slider');
        knob.setAttribute('aria-label', ariaLabel);
        knob.setAttribute('aria-valuemin', String(min));
        knob.setAttribute('aria-valuemax', String(max));
        knob.setAttribute('aria-valuenow', String(value));
        knob.setAttribute('aria-valuetext', `${value}%`);
        root.classList.toggle('hidden', !isVisible());
    }

    function commit(value) {
        const previous = normalizePercentValue(getValue(), bounds);
        setValue(normalizePercentValue(value, bounds));
        const current = normalizePercentValue(getValue(), bounds);
        render();
        if (current !== previous) onValueChange(current);
    }

    function onWheel(event) {
        if (!event.deltaY) return;
        event.preventDefault();
        commit(getValue() + (event.deltaY < 0 ? 1 : -1));
    }

    function onMouseDown(event) {
        if (event.button !== undefined && event.button !== 0) return;
        dragStartY = event.clientY;
        dragStartValue = normalizePercentValue(getValue(), bounds);
        event.preventDefault();
    }

    function onMouseMove(event) {
        if (dragStartY === null || dragStartValue === null) return;
        const delta = Math.round((dragStartY - event.clientY) / DRAG_PIXELS_PER_UNIT);
        commit(dragStartValue + delta);
    }

    function onMouseUp() {
        dragStartY = null;
        dragStartValue = null;
    }

    function onKeyDown(event) {
        let next = null;
        if (event.key === 'ArrowUp' || event.key === 'ArrowRight') next = getValue() + 1;
        else if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') next = getValue() - 1;
        else if (event.key === 'Home') next = min;
        else if (event.key === 'End') next = max;
        else if (pageStep && event.key === 'PageUp') next = getValue() + pageStep;
        else if (pageStep && event.key === 'PageDown') next = getValue() - pageStep;
        if (next === null) return;
        event.preventDefault();
        commit(next);
    }

    knob.addEventListener('wheel', onWheel, { passive: false });
    knob.addEventListener('mousedown', onMouseDown);
    knob.addEventListener('keydown', onKeyDown);
    eventTarget.addEventListener('mousemove', onMouseMove);
    eventTarget.addEventListener('mouseup', onMouseUp);
    render();

    return {
        render,
        destroy() {
            knob.removeEventListener('wheel', onWheel);
            knob.removeEventListener('mousedown', onMouseDown);
            knob.removeEventListener('keydown', onKeyDown);
            eventTarget.removeEventListener('mousemove', onMouseMove);
            eventTarget.removeEventListener('mouseup', onMouseUp);
        },
    };
}
