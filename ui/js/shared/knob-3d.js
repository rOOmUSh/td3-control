// Draws the 3D GLB knob over an existing DOM knob.
//
// The canvas is a skin, never a control. The host element keeps its id, its
// box and every listener it already had, and a plain 2D canvas is appended
// inside it. Two things follow: selectors and event paths that drive the host
// keep working untouched, and the failure path needs no branching, because a
// failed mount simply leaves the flat knob on screen.
//
// The canvas registers no input at all. Every wheel, drag and key press
// passes through to the host's existing handlers exactly once, so no control
// changes behaviour by gaining a 3D face.
//
// All knobs share one WebGL context through `knob-renderer.js` and receive
// their pixels by blit. A context per knob would not survive the page: the
// pattern lane drawers alone can ask for thousands, and browsers silently
// lose every context past sixteen.

// The renderer is imported dynamically inside the mount, never at module
// scope. A static import would put the knob package in the module graph of
// every control that draws a face, so a missing or blocked asset would fail
// `percent-knob.js` and `step-lane-drawer.js`, and through them the page
// entry module. The flat knobs would go down with it, which is the opposite
// of a fallback.
const KNOB_START_DEGREES = -145;
const KNOB_END_DEGREES = 145;

/** Class on every mounted canvas. */
export const CANVAS_CLASS = 'knob-3d-canvas';

/** Class added to the host element while a canvas is mounted. */
export const MOUNTED_CLASS = 'has-knob-3d';

/**
 * Backing-store scale. The shared renderer draws at 256px, so even the
 * largest knob here blits down rather than up.
 */
const PIXEL_SCALE = 3;

/**
 * Degrees for `value` over `[min, max]`, clockwise-positive from the top.
 * Mirrors `knobDegrees` in the package, kept here so this module needs no
 * static import of it.
 */
export function knobDegrees(value, min, max) {
    const span = max - min;
    const clamped = Math.min(max, Math.max(min, Number(value)));
    const ratio = span === 0 ? 0 : (clamped - min) / span;
    return KNOB_START_DEGREES + (KNOB_END_DEGREES - KNOB_START_DEGREES) * ratio;
}

function clamp(value, min, max) {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) return min;
    return Math.min(max, Math.max(min, numeric));
}

/**
 * Draw the 3D knob inside `host`.
 *
 * Returns `{ canvas, setValue, dispose }` on success and `null` on any
 * failure, so callers never need an error branch. `size` is the drawn square
 * in CSS pixels; positioning over the host is left to CSS.
 *
 * `loadRenderer` exists so tests can supply a stub: the real module imports
 * three.js and cannot be loaded outside a browser.
 */
export async function mountKnob3d({
    host,
    min,
    max,
    value,
    size,
    loadRenderer = null,
} = {}) {
    if (!host || typeof host.appendChild !== 'function') return null;
    if (!(Number.isFinite(min) && Number.isFinite(max) && max > min)) return null;
    if (!(Number.isFinite(size) && size > 0)) return null;

    let canvas = null;
    try {
        const renderer = loadRenderer
            ? await loadRenderer()
            : await (await import('../../partials/knob/src/knob-renderer.js')).getKnobRenderer();
        canvas = host.ownerDocument.createElement('canvas');
        canvas.className = CANVAS_CLASS;
        canvas.width = Math.round(size * PIXEL_SCALE);
        canvas.height = Math.round(size * PIXEL_SCALE);
        canvas.style.width = `${size}px`;
        canvas.style.height = `${size}px`;
        // The host already carries whatever ARIA its control needs.
        canvas.setAttribute('aria-hidden', 'true');
        host.appendChild(canvas);

        const paint = (next) => {
            const safe = clamp(next, min, max);
            renderer.paint(canvas, knobDegrees(safe, min, max));
            // The drawn angle lives in a canvas bitmap, so the value it shows
            // is mirrored onto the element for tests to read.
            canvas.dataset.knobValue = String(safe);
        };

        paint(value);
        host.classList.add(MOUNTED_CLASS);

        return {
            canvas,
            setValue: paint,
            dispose() {
                host.classList.remove(MOUNTED_CLASS);
                canvas.remove();
            },
        };
    } catch (error) {
        if (canvas) canvas.remove();
        host.classList.remove(MOUNTED_CLASS);
        console.warn('3D knob unavailable, keeping the flat knob:', error);
        return null;
    }
}
