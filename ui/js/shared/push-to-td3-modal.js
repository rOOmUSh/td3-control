// Shared PUSH TO TD-3 confirmation modal.
//
// Extracted from `ui/js/progression/progression-push.js` so the main Control
// page and the progression page share one modal implementation. The
// progression page's call shape is unchanged
// (N=4, targets computed by `progression-push.js::computeTargets`, same
// modal chrome) so the existing progression regression tests still lock the
// behaviour.
//
// Responsibilities:
//   1. Render the confirmation body (intro, target line, scratch note,
//      warning).
//   2. On CONFIRM, write each `patterns[i]` to `targets[i]` via
//      `api.savePattern(group, pattern, side, pattern)`.
//   3. Surface per-write progress via `setStatus` and surface hard failures
//      by throwing (the shared `openModal` primary handler keeps the
//      dialog open on throw and toasts the error - matches progression
//      behaviour pre-extraction).
//
// The modal is parameterised by a label builder so the progression page
// keeps its `G1-P1A` (dashed) target line and the main page can opt into
// `G1P1A` (undashed, matches bank slot keys). Both pages share the scratch
// exclusion note format.

import { openModal } from '../bank/bank-modal.js';
import { toast } from '../bank/bank-toast.js';

/**
 * Open the push-to-TD-3 confirmation modal.
 *
 * @param {Object} opts
 * @param {Array<Pattern>} opts.patterns        patterns to write, in target order
 * @param {Array<{group:number, pattern:number, side:string, label:string}>} opts.targets
 *     targets[i] receives patterns[i]; must be same length as patterns
 * @param {Object} opts.api                     backend - needs savePattern(g,p,side,pat)
 * @param {string} [opts.scratchLabel]          scratch slot label to show in the note
 * @param {Function} opts.setStatus             status-line writer
 * @param {string} [opts.title]                 modal title
 * @param {string} [opts.introText]             first paragraph
 * @param {string} [opts.warnText]              red/warning paragraph
 * @param {Function} [opts.onComplete]          called after all writes succeed
 */
export function openPushToTd3Modal(opts) {
    const {
        patterns,
        targets,
        api,
        scratchLabel,
        setStatus,
        title,
        introText,
        warnText,
        onComplete,
    } = opts;

    if (!Array.isArray(patterns) || !Array.isArray(targets)) {
        toast('Push aborted: invalid patterns/targets.', 'error');
        return;
    }
    if (patterns.length === 0 || patterns.length !== targets.length) {
        toast('Push aborted: patterns/targets length mismatch.', 'error');
        return;
    }

    const n = patterns.length;
    const modalTitle = title || `Push ${n} pattern${n === 1 ? '' : 's'} to TD-3`;
    const intro = introText
        || `This will overwrite the following ${n} device slot${n === 1 ? '' : 's'} `
           + 'with the current pattern(s) (P1 → first target, P2 → second, etc.):';
    const warn = warnText
        || 'Current device content at these slots will be replaced and cannot '
           + 'be recovered unless you have a separate backup.';

    // --- Modal body ------------------------------------------------------

    const body = document.createElement('div');
    body.className = 'bank-confirm-body';

    const introEl = document.createElement('p');
    introEl.textContent = intro;
    body.appendChild(introEl);

    // Target line. For ≤8 targets we keep the single-line arrow form
    // (matches progression behaviour bit-identically). For >8 we fall
    // back to a compact multi-column grid so 63 targets stay legible
    // without overflowing the modal.
    if (n <= 8) {
        const addrLine = document.createElement('p');
        addrLine.style.color = '#dc143c';
        addrLine.style.fontWeight = '900';
        addrLine.style.fontSize = '1.5rem';
        addrLine.style.letterSpacing = '0.08em';
        addrLine.style.textAlign = 'center';
        addrLine.style.margin = '0.75rem 0';
        addrLine.textContent = targets.map((t) => t.label).join('  →  ');
        body.appendChild(addrLine);
    } else {
        const grid = document.createElement('div');
        grid.style.display = 'grid';
        grid.style.gridTemplateColumns = 'repeat(8, 1fr)';
        grid.style.gap = '0.25rem 0.5rem';
        grid.style.margin = '0.75rem 0';
        grid.style.color = '#dc143c';
        grid.style.fontWeight = '800';
        grid.style.fontSize = '0.85rem';
        grid.style.textAlign = 'center';
        for (let i = 0; i < targets.length; i += 1) {
            const cell = document.createElement('span');
            cell.textContent = `P${i + 1}→${targets[i].label}`;
            grid.appendChild(cell);
        }
        body.appendChild(grid);
    }

    if (scratchLabel) {
        const scratchNote = document.createElement('p');
        scratchNote.style.opacity = '0.75';
        scratchNote.style.fontSize = '0.85rem';
        scratchNote.textContent =
            `Scratch slot ${scratchLabel} is excluded from the write sequence.`;
        body.appendChild(scratchNote);
    }

    const warnEl = document.createElement('p');
    warnEl.style.opacity = '0.75';
    warnEl.style.fontSize = '0.85rem';
    warnEl.textContent = warn;
    body.appendChild(warnEl);

    // --- Confirm handler -------------------------------------------------

    openModal({
        title: modalTitle,
        body,
        primaryLabel: 'CONFIRM',
        secondaryLabel: 'CANCEL',
        danger: true,
        noScrim: false,
        onPrimary: async () => {
            setStatus(`Pushing ${n} pattern${n === 1 ? '' : 's'}...`);
            for (let i = 0; i < n; i += 1) {
                const t = targets[i];
                const pat = patterns[i];
                try {
                    await api.savePattern(t.group, t.pattern, t.side, pat);
                    setStatus(`Wrote P${i + 1} → ${t.label}`);
                } catch (err) {
                    // Throw so openModal keeps the dialog open and toasts
                    // the error - the user sees which slot failed.
                    throw new Error(`Write to ${t.label} failed: ${err.message}`);
                }
            }
            setStatus(
                `Pushed ${n} pattern${n === 1 ? '' : 's'} → `
                + targets.map((t) => t.label).join(', '),
            );
            if (typeof onComplete === 'function') {
                try { onComplete(); } catch (_err) { /* best-effort */ }
            }
        },
    });
}
