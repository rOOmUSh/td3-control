// Side effects of a per-step lane edit on the Control page.
//
// The card renderer builds each drawer but owns no transport or API
// access. `main.js` registers the hooks once; the renderer calls them
// through this module so a lane edit can reach the device (immediate
// CC for audible feedback), the running host audition (schedule
// resync), and the clock thread (LIVE lane) without the row module
// importing any of those.

let hooks = {
    onValue: () => {},
    onToggle: () => {},
};

export function setStepLaneHooks(next) {
    hooks = {
        onValue: typeof next?.onValue === 'function' ? next.onValue : () => {},
        onToggle: typeof next?.onToggle === 'function' ? next.onToggle : () => {},
    };
}

export function stepLaneValueChanged(patternIdx, lane, step, value) {
    hooks.onValue(patternIdx, lane, step, value);
}

export function stepLaneToggled(patternIdx, lane, on) {
    hooks.onToggle(patternIdx, lane, on);
}
