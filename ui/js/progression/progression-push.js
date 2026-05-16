// Push the 4 progression patterns to the TD-3 in consecutive slots starting
// at the currently selected slot, skipping the scratch slot so the user's
// live-play buffer isn't clobbered mid-session.
//
// Slot walk rules (progression-specific, NOT the canonical A/B ordering
// that the main Control page uses - see `ui/js/shared/slot-targets.js`):
//   - start at `selected` (group, pattern, side)
//   - step through pattern numbers 1..8 cyclically WITHIN the selected
//     group + side (we don't cross side or group boundaries - patterns per
//     TD-3 are addressed P1..P8 within each side/group)
//   - skip any slot that equals the scratch address
//   - collect exactly 4 target addresses
//
// Example: scratch=G1P2A, selected=G1P1A → P1, P3, P4, P5
//          scratch=G1P2A, selected=G1P3A → P3, P4, P5, P6
//          scratch=G1P2A, selected=G1P8A → P8, P1, P3, P4
//
// The shared PUSH TO TD-3 modal (`ui/js/shared/push-to-td3-modal.js`)
// renders the computed targets and drives the write loop. Progression's
// behaviour pre-extraction is locked by `progression-push.test.js`
// (computeTargets) and manual browser verification of the modal.

import { toast } from '../bank/bank-toast.js';
import { openPushToTd3Modal } from '../shared/push-to-td3-modal.js';

/**
 * Compute the 4 target slots for the push operation.
 *
 * @param {{group:number, pattern:number, side:string}} selected
 * @param {{group:number, pattern:number, side:string}} scratch
 * @returns {Array<{group:number, pattern:number, side:string, label:string}>}
 */
export function computeTargets(selected, scratch) {
    const group = selected.group;
    const side = selected.side;
    const targets = [];
    let p = selected.pattern;
    // Hard-capped loop to avoid any chance of spinning: we visit at most 8
    // distinct slots within the side/group; if after a full cycle we still
    // haven't filled 4 targets something is wrong.
    for (let i = 0; i < 8 && targets.length < 4; i += 1) {
        const isScratch =
            scratch &&
            scratch.group === group &&
            scratch.side === side &&
            scratch.pattern === p;
        if (!isScratch) {
            targets.push({
                group,
                pattern: p,
                side,
                label: `G${group}-P${p}${side}`,
            });
        }
        p += 1;
        if (p > 8) p = 1;
    }
    return targets;
}

/**
 * Open the push-to-TD-3 confirmation modal. On CONFIRM, writes the 4
 * progression patterns to the computed target addresses in order.
 *
 * @param {Object} opts
 * @param {Object} opts.state       Progression state module
 *   (getGroup / getPatternNum / getSide / getPattern(idx) / isConnected).
 * @param {Object} opts.scratch     { group, pattern, side, label }
 * @param {Object} opts.api         Backend API (savePattern).
 * @param {Function} opts.setStatus Status-line writer.
 */
export function openPushModal({ state, scratch, api, setStatus }) {
    if (!state.isConnected()) {
        setStatus('Connect MIDI first');
        return;
    }

    const selected = {
        group: state.getGroup(),
        pattern: state.getPatternNum(),
        side: state.getSide(),
    };
    const targets = computeTargets(selected, scratch);

    if (targets.length < 4) {
        // Shouldn't happen (8 slots - 1 scratch = 7 available), but fail
        // loud rather than writing fewer than 4 patterns.
        toast('Could not compute 4 target slots - aborting.', 'error');
        return;
    }

    // Collect the 4 progression patterns in target order. Same order the
    // inlined loop used before extraction: `state.getPattern(i)` for i=0..3.
    const patterns = [0, 1, 2, 3].map((i) => state.getPattern(i));

    openPushToTd3Modal({
        title: 'Push 4 patterns to TD-3',
        introText:
            'This will overwrite the following 4 device slots with the current '
            + 'progression patterns (P1 → first target, P2 → second, etc.):',
        patterns,
        targets,
        api,
        scratchLabel: scratch && scratch.label,
        setStatus,
    });
}
