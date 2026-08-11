// Hard endpoint switch for the TRIPLET morph amount.
//
// A two-position vertical switch inside the TRIPLET knob group: up is
// 0, down is 100. Clicking always jumps the amount to the opposite
// endpoint, even from an intermediate knob value. The switch mirrors
// the knob only at the exact endpoints: amount 0 pulls it up, amount
// 100 pulls it down, and any value in between leaves it at its last
// endpoint side, so knob sweeps never move it.

const THUMB_EDGE_PX = '2px';

export function createTripletMorphEndpointToggle({
    button,
    thumb,
    getValue,
    setValue,
    onValueChange = () => {},
}) {
    if (!button || !thumb
        || typeof getValue !== 'function' || typeof setValue !== 'function') {
        return { render() {}, destroy() {} };
    }

    // Last endpoint side reached; intermediate amounts leave it as is.
    let position = Number(getValue()) === 100 ? 'hundred' : 'zero';

    function render() {
        const value = Number(getValue());
        if (value === 0) position = 'zero';
        else if (value === 100) position = 'hundred';
        const down = position === 'hundred';
        button.setAttribute('role', 'switch');
        button.setAttribute('aria-label', 'Triplet endpoint toggle');
        button.setAttribute('aria-checked', down ? 'true' : 'false');
        button.dataset.position = down ? 'hundred' : 'zero';
        if (down) {
            thumb.style.top = 'auto';
            thumb.style.bottom = THUMB_EDGE_PX;
        } else {
            thumb.style.top = '';
            thumb.style.bottom = '';
        }
    }

    function onClick() {
        const target = position === 'hundred' ? 0 : 100;
        const previous = Number(getValue());
        setValue(target);
        const current = Number(getValue());
        // A rejecting state setter (ineligible source) keeps the switch
        // where it was; only a reached endpoint flips it.
        if (current === target) {
            position = target === 100 ? 'hundred' : 'zero';
        }
        render();
        if (current !== previous) onValueChange(current);
    }

    button.addEventListener('click', onClick);
    render();

    return {
        render,
        destroy() {
            button.removeEventListener('click', onClick);
        },
    };
}

export function initTripletMorphEndpointToggle({
    getValue,
    setValue,
    onValueChange,
    documentRef = globalThis.document,
} = {}) {
    return createTripletMorphEndpointToggle({
        button: documentRef?.getElementById('triplet-morph-endpoint-toggle'),
        thumb: documentRef?.getElementById('triplet-morph-endpoint-thumb'),
        getValue,
        setValue,
        onValueChange,
    });
}
