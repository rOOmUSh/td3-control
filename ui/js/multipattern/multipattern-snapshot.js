// Overflow snapshot flow for PUSH TO TD-3 at N=64.
//
// When the user has 64 patterns on the main page, the canonical A/B ordering
// with scratch excluded can only hold 63 of them - the 64th has no device
// slot. The flow must then:
//
//   1. Create a bank snapshot named `main-overflow-YYYY-MM-DD` holding ALL 64
//      patterns in canonical A/B order (scratch *not* excluded - the snapshot
//      is a pure software backup so all 64 UI patterns survive).
//   2. After the snapshot succeeds, push the first 63 non-scratch slots to
//      the device as usual. The 64th UI pattern never hits the hardware -
//      it's preserved only in the snapshot, and the confirm modal tells the
//      user exactly this.
//   3. If snapshot creation fails, abort entirely - no device writes happen,
//      so the user can retry without a partial push.
//
// The snapshot → device order matters: snapshot first, device second. If
// we wrote the device first and the snapshot then failed, P64 would be
// irretrievably dropped from the workflow. Doing snapshot first makes the
// failure mode "nothing changed, try again".
//
// Slot-key format divergence: frontend `slotFor()` emits `G1P1A` (undashed,
// matches bank slot badges); backend strictly expects `G1-P1A` (dashed).
// `toDashedSlotKey()` bridges them for the snapshot request only - the
// device push path keeps using the `{group, pattern, side}` triple directly.

import { orderedSlots } from '../shared/slot-targets.js';
import { slotFor } from '../shared/slot-targets.js';
import { openModal } from '../bank/bank-modal.js';
import { openPushToTd3Modal } from '../shared/push-to-td3-modal.js';

/**
 * Convert a frontend slot label like "G1P1A" to the backend's dashed form
 * "G1-P1A". Pure; exported for tests.
 */
export function toDashedSlotKey(label) {
    if (typeof label !== 'string') return null;
    const m = /^G([1-4])P([1-8])([AB])$/.exec(label);
    if (!m) return null;
    return `G${m[1]}-P${m[2]}${m[3]}`;
}

/**
 * Build the 64-slot snapshot payload (`slots` array) for the overflow flow.
 *
 * Uses `orderedSlots(mode)` (scratch NOT excluded) so all 64 UI patterns
 * land in the snapshot. Returns `{ slots, error }`:
 *   - `{ slots: [{slot_key, pattern}], error: null }` on success (length 64);
 *   - `{ slots: null, error: 'bad-n' }` when patterns.length !== 64;
 *   - `{ slots: null, error: 'bad-mode' }` when mode isn't ALTERNATE/SERIAL.
 *
 * Kept pure so `multipattern-snapshot.test.js` can assert the exact slot
 * sequence without stubbing fetch or DOM.
 *
 * @param {Array<Pattern>} patterns          the 64 UI patterns (index 0..63)
 * @param {'ALTERNATE'|'SERIAL'} mode
 */
export function buildSnapshotSlots(patterns, mode) {
    if (!Array.isArray(patterns) || patterns.length !== 64) {
        return { slots: null, error: 'bad-n' };
    }
    let ordered;
    try {
        ordered = orderedSlots(mode);
    } catch {
        return { slots: null, error: 'bad-mode' };
    }
    const slots = [];
    for (let i = 0; i < 64; i++) {
        const label = ordered[i].label;
        const key = toDashedSlotKey(label);
        if (!key) return { slots: null, error: 'bad-mode' };
        slots.push({ slot_key: key, pattern: patterns[i] });
    }
    return { slots, error: null };
}

/** Default overflow snapshot name - `main-overflow-YYYY-MM-DD` in local time. */
export function defaultSnapshotName(date) {
    const d = date || new Date();
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `main-overflow-${y}-${m}-${day}`;
}

/**
 * Build the 63 device-write targets for the overflow push (same shape as
 * `buildPushTargets` in multipattern-push.js, but *only* for N=64 - the first
 * 63 UI patterns go to the 63 non-scratch slots, and P64 is skipped).
 *
 * Returns `{ targets, patternsToWrite, error }` where `patternsToWrite` is
 * the first 63 entries of `patterns` aligned with `targets` (same length),
 * so callers can hand them straight to `openPushToTd3Modal`.
 */
export function buildOverflowDeviceTargets(patterns, scratch, mode, startSlot) {
    if (!Array.isArray(patterns) || patterns.length !== 64) {
        return { targets: null, patternsToWrite: null, error: 'bad-n' };
    }
    if (!scratch) {
        // N=64 without scratch means no address to skip - there's no overflow
        // case yet (P64 would just write to the 64th slot). Callers should
        // not route here; we surface an explicit error so mis-routing is
        // noisy rather than silently dropping P64.
        return { targets: null, patternsToWrite: null, error: 'no-scratch' };
    }
    const targets = [];
    for (let i = 0; i < 63; i++) {
        const t = slotFor(i, scratch, mode, startSlot);
        if (!t) return { targets: null, patternsToWrite: null, error: 'unexpected-null-slot' };
        targets.push(t);
    }
    return { targets, patternsToWrite: patterns.slice(0, 63), error: null };
}

// --- DOM flow --------------------------------------------------------------

/**
 * Render the pre-snapshot confirmation modal that explains the two-step
 * flow (snapshot first, device second, P64 snapshot-only). On CONFIRM, POSTs
 * the snapshot then opens the standard push modal for the 63 device writes.
 *
 * @param {Object} opts
 * @param {Array<Pattern>} opts.patterns        all 64 UI patterns
 * @param {{group,pattern,side,label}} opts.scratch
 * @param {'ALTERNATE'|'SERIAL'} opts.mode
 * @param {{group,pattern,side,label}|null} [opts.startSlot] sidebar-anchored P1 target
 * @param {Object} opts.api                     backend API (savePattern)
 * @param {Object} opts.bankApi                 bank API (createSnapshotFromPatterns)
 * @param {Function} opts.setStatus             status-line writer
 * @param {string} [opts.suggestedName]         override for manual testing
 */
export function openOverflowPushFlow(opts) {
    const { patterns, scratch, mode, startSlot, api, bankApi, setStatus, suggestedName } = opts;

    const name = suggestedName || defaultSnapshotName();

    const { slots, error: snapErr } = buildSnapshotSlots(patterns, mode);
    if (snapErr || !slots) {
        setStatus(`Overflow aborted: snapshot prep failed (${snapErr})`);
        return;
    }

    const deviceResult = buildOverflowDeviceTargets(patterns, scratch, mode, startSlot);
    if (deviceResult.error) {
        setStatus(`Overflow aborted: device target prep failed (${deviceResult.error})`);
        return;
    }
    const { targets, patternsToWrite } = deviceResult;

    // --- Confirmation modal body --------------------------------------

    const body = document.createElement('div');
    body.className = 'bank-confirm-body';

    const p1 = document.createElement('p');
    p1.textContent =
        `You have 64 patterns; the TD-3 can only hold 63 (one slot is scratch). `
        + `Before pushing, all 64 patterns will be saved to a bank snapshot so `
        + `P64 isn't lost.`;
    body.appendChild(p1);

    const p2 = document.createElement('p');
    p2.innerHTML =
        `<strong>Step 1.</strong> Save a bank snapshot named `
        + `<code class="tactile-code">${escapeHtml(name)}</code> with all 64 patterns in `
        + `${mode === 'ALTERNATE' ? 'A/B alternating' : 'As-then-Bs serial'} order.`;
    body.appendChild(p2);

    const p3 = document.createElement('p');
    p3.innerHTML =
        `<strong>Step 2.</strong> Push patterns 1-63 to the TD-3 (P64 stays in the snapshot only).`;
    body.appendChild(p3);

    const warn = document.createElement('p');
    warn.className = 'bank-warn';
    warn.textContent =
        'Device slot writes cannot be undone once the snapshot step completes. '
        + 'If snapshot creation fails, no device writes happen.';
    body.appendChild(warn);

    openModal({
        title: 'Overflow push (N=64) - snapshot + push',
        body,
        primaryLabel: 'Continue',
        onPrimary: async () => {
            setStatus('Creating overflow snapshot…');
            let created;
            try {
                created = await bankApi.createSnapshotFromPatterns({
                    name,
                    description: 'Auto-created by main-page PUSH at N=64.',
                    slots,
                });
            } catch (err) {
                setStatus(`Overflow aborted: snapshot create failed - ${err.message || err}`);
                throw err; // keeps modal open; shared openModal toasts on throw
            }
            const finalName = created && created.snapshot ? created.snapshot.name : name;
            setStatus(`Snapshot '${finalName}' created - opening push confirm…`);

            // Chain straight into the standard push modal for the 63 device
            // writes. Its CONFIRM drives savePattern for each target.
            openPushToTd3Modal({
                title: `Push 63 of 64 patterns to TD-3 (P64 snapshot-only)`,
                introText:
                    `Snapshot '${finalName}' saved. Now overwrite the following 63 device slot(s) `
                    + `with P1-P63. P64 is preserved in the snapshot only and will NOT be written:`,
                warnText:
                    `Current device content at these 63 slots will be replaced and cannot be recovered `
                    + `except via the snapshot just created (or a separate backup).`,
                patterns: patternsToWrite,
                targets,
                api,
                scratchLabel: scratch.label,
                setStatus,
            });
        },
    });
}

function escapeHtml(s) {
    return String(s)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}
