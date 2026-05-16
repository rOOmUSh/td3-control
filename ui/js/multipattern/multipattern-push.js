// PUSH TO TD-3 on the main Control page.
//
// The scratch-excluded canonical ordering covers every UI
// pattern 1..N. For N ∈ [1, 63] the write targets are `slotFor(i, scratch,
// mode)` for i=0..N-1 (slot-targets.js owns the ALTERNATE/SERIAL walk). At
// N=64 the last pattern has no device slot (overflow) and the flow hands
// off to a bank snapshot so the full pattern set is preserved.
//
// Selection is NOT consulted here. SAVE is the selection-scoped write;
// PUSH is "send the whole thing." Mixing the two caused an
// early-draft regression where checking a single pattern accidentally
// narrowed the push scope to one slot. The button label and N both refer to the full list.
//
// The modal + write loop live in `shared/push-to-td3-modal.js`; this module
// is pure glue: state → targets → shared modal.

import { slotFor } from '../shared/slot-targets.js';
import { openPushToTd3Modal } from '../shared/push-to-td3-modal.js';
import { openOverflowPushFlow } from './multipattern-snapshot.js';

/**
 * Pure: build the target list for pushing N patterns, using the canonical
 * A/B ordering anchored at `startSlot` (sidebar selector) with the
 * scratch slot removed. Returns `{ targets, error }` - one of:
 *   - `{ targets: [...], error: null }` on success (length === n);
 *   - `{ targets: null, error: 'overflow' }` when any idx lacks a slot
 *     (the caller is responsible for routing to the snapshot flow).
 *
 * Kept pure so `multipattern-push.test.js` can assert the exact target
 * sequence for representative (N, scratch, mode, startSlot) triples without
 * stubbing DOM or the modal primitive.
 *
 * @param {number} n                            pattern count (1..64)
 * @param {{group:number, pattern:number, side:'A'|'B'}|null} scratch
 * @param {'ALTERNATE'|'SERIAL'} mode
 * @param {{group:number, pattern:number, side:'A'|'B'}|null} [startSlot]
 */
export function buildPushTargets(n, scratch, mode, startSlot) {
    if (!Number.isInteger(n) || n < 0) return { targets: null, error: 'bad-n' };
    const out = [];
    for (let i = 0; i < n; i++) {
        const t = slotFor(i, scratch, mode, startSlot);
        if (!t) return { targets: null, error: 'overflow' };
        out.push(t);
    }
    return { targets: out, error: null };
}

/**
 * @param {Object} opts
 * @param {Object} opts.state      Multipattern state module
 *   (isConnected, getPatterns, getAbMode, getScratchSlot, onChange).
 * @param {Object} opts.api        Backend API - needs savePattern(g,p,side,pat).
 * @param {Function} opts.setStatus Status-line writer.
 */
export function init({ state, api, bankApi, setStatus }) {
    const btn = document.getElementById('btn-push-td3');
    if (!btn) return;

    function updateChrome() {
        const n = state.getPatterns().length;
        const connected = state.isConnected();
        const scratch = state.getScratchSlot();

        let disabled = false;
        let title = 'Push every UI pattern to TD-3 in the canonical A/B order';

        if (!connected) {
            disabled = true;
            title = 'Connect MIDI first';
        } else if (n === 0) {
            disabled = true;
            title = 'No patterns to push';
        } else if (!scratch) {
            disabled = true;
            title = 'Waiting for scratch slot - retry in a moment';
        } else if (n > 64) {
            // Defence in depth: the state module caps N at 64, but if
            // something ever slips past that we want a noisy, disabled
            // button rather than truncating or looping.
            disabled = true;
            title = 'Too many patterns - trim to 64 or fewer';
        } else if (n === 64) {
            // Hands off to the overflow snapshot flow
            // (bank snapshot all 64 + device push of 63 non-scratch slots).
            // Kept enabled; the Continue modal explains the two-step
            // behaviour and lets the user cancel before anything writes.
            title = 'Push 63 to TD-3 and save all 64 to an overflow snapshot';
        }

        btn.disabled = disabled;
        btn.title = title;
        btn.classList.toggle('opacity-50', disabled);
        btn.classList.toggle('cursor-not-allowed', disabled);
    }

    btn.addEventListener('click', () => {
        if (btn.disabled) return;

        const scratch = state.getScratchSlot();
        const mode = state.getAbMode();
        const startSlot = state.getSelectedSlot();
        const patterns = state.getPatterns();
        const n = patterns.length;

        // Re-validate at click time - state may have changed between the
        // last chrome update and the user's click (e.g. MIDI disconnect).
        if (!state.isConnected()) { setStatus('Connect MIDI first'); return; }
        if (n === 0)              { setStatus('No patterns to push');  return; }
        if (!scratch)             { setStatus('Scratch slot not yet known');   return; }
        if (n > 64)               { setStatus('Too many patterns - trim to 64 or fewer'); return; }

        // N=64 → overflow flow: snapshot all 64, then device-push 63.
        if (n === 64) {
            if (!bankApi) {
                setStatus('Overflow aborted: bank API not wired');
                return;
            }
            openOverflowPushFlow({ patterns, scratch, mode, startSlot, api, bankApi, setStatus });
            return;
        }

        // N<64 → plain push. `buildPushTargets` returns `{ error: 'overflow' }`
        // only when a slot is missing (idx=63 with scratch present) - we
        // already gated n<64 above, so any error here is a bug and should
        // fail loud rather than quietly skip a pattern.
        const { targets, error } = buildPushTargets(n, scratch, mode, startSlot);
        if (error || !targets) {
            setStatus(`Push aborted: ${error || 'no targets'}`);
            return;
        }

        openPushToTd3Modal({
            title: `Push ${n} pattern${n === 1 ? '' : 's'} to TD-3`,
            introText:
                `This will overwrite the following ${n} device slot${n === 1 ? '' : 's'} `
                + 'with every UI pattern (P1 → first target, P2 → second, etc.):',
            patterns,
            targets,
            api,
            scratchLabel: scratch.label,
            setStatus,
        });
    });

    state.onChange(updateChrome);
    updateChrome();
}
