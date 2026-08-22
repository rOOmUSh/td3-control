// Wires the CUTOFF and BEND knobs to a page state module and the API.
//
// Both knobs exist only for a device that reports device-control
// support; `render()` re-evaluates that flag. Sends are throttled so a
// drag yields at most one request per interval and ends on the final
// value. When support appears (a fresh connection) the bend knob and
// the device are both put at center so the readout matches what the
// device is doing.

import { api } from '../api.js';
import { initFilterCutoffControl } from './filter-cutoff-control.js';
import { initPitchBendControl, PITCH_BEND_CENTER } from './pitch-bend-control.js';
import { createTrailingThrottle } from './trailing-throttle.js';

export const DEVICE_CONTROL_SEND_INTERVAL_MS = 30;

export function initDeviceControls({
    state,
    setStatus = () => {},
    apiRef = api,
    documentRef = globalThis.document,
    intervalMs = DEVICE_CONTROL_SEND_INTERVAL_MS,
} = {}) {
    const isSupported = () => state.isDeviceControlsSupported();

    function report(label, promise) {
        promise.catch((err) => setStatus(`${label} send failed: ${err.message}`));
    }

    const sendCutoff = createTrailingThrottle((value) => {
        report('CUTOFF', apiRef.filterCutoff(value, state.getMidiChannel()));
    }, intervalMs);

    const sendBend = createTrailingThrottle((value) => {
        report('BEND', apiRef.pitchBend(value, state.getMidiChannel()));
    }, intervalMs);

    const cutoffControl = initFilterCutoffControl({
        getValue: () => state.getFilterCutoff(),
        setValue: (value) => state.setFilterCutoff(value),
        isVisible: isSupported,
        onValueChange: sendCutoff,
        documentRef,
    });

    const bendControl = initPitchBendControl({
        getValue: () => state.getPitchBend(),
        setValue: (value) => state.setPitchBend(value),
        isVisible: isSupported,
        onValueChange: sendBend,
        documentRef,
    });

    // Centring only makes sense when the BEND knob is actually in the
    // bar; with the markup absent nothing is sent to the device.
    const bendKnobPresent = Boolean(documentRef?.getElementById('pitch-bend-knob'));
    let wasSupported = false;

    function render() {
        const supported = isSupported();
        if (supported && !wasSupported && bendKnobPresent) {
            state.setPitchBend(PITCH_BEND_CENTER);
            sendBend(PITCH_BEND_CENTER);
        }
        wasSupported = supported;
        cutoffControl.render();
        bendControl.render();
    }

    render();
    return { render };
}
