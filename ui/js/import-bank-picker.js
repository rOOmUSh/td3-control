// IMPORT flow for `.sqs` / `.rbs` bank files.
//
// The IMPORT button on the main toolbar already handles the single-pattern
// text/binary formats (toml/json/steps/pat/seq/mid). Bank files are
// different - each file holds up to 64 patterns and the user has to decide
// which one to edit. This module handles that picker step:
//
//   1. POST the raw file bytes to `/api/pattern/parse-bank`.
//   2. Render the returned 64-slot grid inside a modal (shared renderer
//      from `shared/snapshot-grid.js`, same DOM shape the Bank snapshot
//      view uses).
//   3. Let the user preview any populated slot via the per-cell play button
//      (→ `/api/pattern/play-preview`, scratch-slot audition).
//   4. Selection + commit:
//        - `multi: false` (default) - single-click selects a cell; Import
//          commits. Double-click is the fast path - commit immediately.
//          `onImport(pattern)` receives the single WebPattern.
//        - `multi: true` - Ctrl+click toggles membership, Shift+click picks
//          an inclusive range (populated slots only) from the last anchor,
//          bare click resets the selection to that one slot. Double-click
//          commits the current selection. Primary button shows the count.
//          `onImport(patterns[])` receives an array of WebPattern in grid
//          order.
//
// Playback state is local to the picker (a single `_playingSlotKey`
// variable plus a tiny listener set) rather than the bank-play global bus.
// These patterns have no library item_id, so the bank surfaces would get
// confused if we routed them through the shared tracker. On modal close we
// send `/transport/stop` if anything is playing so the scratch slot goes
// quiet with the dialog.

import { openModal } from './bank/bank-modal.js';
import { toast } from './bank/bank-toast.js';
import { api } from './api.js';
import { buildSnapshotGrid } from './shared/snapshot-grid.js';
import { envInt } from './td3-env.js';
import { applyClick, patternsFromSelection } from './import-bank-selection.js';

// Default BPM resolved synchronously from window.TD3_CONFIG_ENV (server
// inlines it into every HTML page; see src/web/static_html.rs).
const ENV_DEFAULT_BPM = envInt('uiDefaultBpm');

function getDefaultBpm() {
    return ENV_DEFAULT_BPM;
}

/**
 * Open the bank-import picker for a parsed sqs/rbs file.
 *
 * @param {Object} opts
 * @param {Array<object>} opts.slots      64 slots from /api/pattern/parse-bank.
 * @param {string}        opts.title      Modal title, e.g. "Import from idea.sqs".
 * @param {Function}      opts.onImport   Called with the selected WebPattern
 *                                        (multi=false) or an array of them
 *                                        in grid order (multi=true).
 * @param {boolean}       [opts.multi=false]
 *                                        Enable multi-cell selection
 *                                        (Ctrl+click toggle, Shift+click
 *                                        range).
 * @returns {Promise<void>} Resolves when the modal closes (commit or cancel).
 */
export function openImportBankPicker({ slots, title, onImport, multi = false }) {
    return new Promise((resolve) => {
        // Picker-local playback tracker. `null` when nothing is playing,
        // otherwise the slot_key of the currently auditioning slot.
        let playingSlotKey = null;
        const playListeners = new Set();

        const notifyPlay = () => {
            for (const fn of [...playListeners]) {
                try { fn(playingSlotKey); }
                catch (e) { console.error('import-picker play listener:', e); }
            }
        };

        let selectedKeys = new Set();
        let anchorKey = null;
        let primaryBtn = null;      // populated after openModal; used to toggle enabled state + count label

        const body = document.createElement('div');
        body.className = 'import-bank-picker-body';
        body.style.display = 'flex';
        body.style.flexDirection = 'column';
        body.style.gap = '0.5rem';
        body.style.maxHeight = '70vh';
        body.style.overflow = 'auto';

        const hint = document.createElement('div');
        hint.className = 'text-xs font-mono opacity-70';
        hint.textContent = multi
            ? 'Click a populated slot to select. Ctrl+click to toggle, Shift+click for a range. Double-click to import the current selection. Use the play button for preview.'
            : 'Click a populated slot to select it. Double-click or press Import to replace the current pattern. Use the play button for preview.';
        body.appendChild(hint);

        let commit; // forward declaration so onDblClick can call it

        const updatePrimary = () => {
            if (!primaryBtn) return;
            primaryBtn.disabled = selectedKeys.size === 0;
            if (multi) {
                primaryBtn.textContent = selectedKeys.size > 0
                    ? `Import (${selectedKeys.size})`
                    : 'Import';
            }
        };

        const rerender = () => {
            // Rebuild the grid in-place so selection highlight + playing icon
            // refresh together. The grid is small (64 cells) so a full swap
            // is simpler and cheaper than a targeted class toggle.
            const next = buildGrid();
            gridHost.replaceChildren(next);
        };

        const gridHost = document.createElement('div');
        body.appendChild(gridHost);

        const buildGrid = () => {
            return buildSnapshotGrid(slots, {
                isSelected: (s) => selectedKeys.has(s.slot_key),
                makePlayButton: (s) => makePreviewPlayButton(s, {
                    isPlaying: () => playingSlotKey === s.slot_key,
                    onStart: async () => {
                        try {
                            await api.playPatternPreview(s.pattern, getDefaultBpm());
                            playingSlotKey = s.slot_key;
                            notifyPlay();
                            toast(`Previewing ${s.slot_key}`, 'info');
                        } catch (e) {
                            toast(`Preview failed: ${e.message}`, 'error');
                        }
                    },
                    onStop: async () => {
                        try {
                            await api.transportStop();
                            playingSlotKey = null;
                            notifyPlay();
                        } catch (e) {
                            toast(`Stop failed: ${e.message}`, 'error');
                        }
                    },
                    subscribe: (fn) => { playListeners.add(fn); return () => playListeners.delete(fn); },
                }),
                onClick: (s, ev) => {
                    if (s.empty) return;     // empty slots are not selectable
                    const res = applyClick({
                        slots,
                        currentKeys: selectedKeys,
                        anchorKey,
                        clickedKey: s.slot_key,
                        shiftKey: !!(ev && ev.shiftKey),
                        ctrlKey:  !!(ev && (ev.ctrlKey || ev.metaKey)),
                        multi,
                    });
                    selectedKeys = res.keys;
                    anchorKey = res.anchorKey;
                    updatePrimary();
                    rerender();
                },
                onDblClick: (s, ev) => {
                    if (s.empty) return;
                    ev.preventDefault();
                    // In single mode, dblclick forces the clicked cell as
                    // selection. In multi mode, the click that precedes the
                    // dblclick has already updated the selection - just
                    // commit what's there.
                    if (!multi) {
                        selectedKeys = new Set([s.slot_key]);
                        anchorKey = s.slot_key;
                    }
                    commit();
                },
            });
        };

        gridHost.appendChild(buildGrid());

        let committed = false;
        const close = openModal({
            title,
            body,
            size: 'wide',
            primaryLabel: 'Import',
            secondaryLabel: 'Cancel',
            onPrimary: async () => { commit(); },
        });

        // The shared grid is 16 columns wide; at ×1.5 cell sizing the
        // default 560px `size: 'wide'` modal squeezes cells below a
        // readable width. Widen the host modal directly - the bank-modal
        // max-width (88vw) still caps it on small screens.
        const hostModal = body.closest('.bank-modal');
        if (hostModal) hostModal.style.minWidth = '1100px';

        // openModal grabs the first focusable element - find the primary
        // button (the "Import" <button> inside .bank-modal-actions) so we
        // can gate it on selection.
        primaryBtn = findPrimaryButton(body);
        updatePrimary();

        commit = () => {
            if (committed) return;
            if (selectedKeys.size === 0) {
                toast('Pick a populated slot first', 'info');
                return;
            }
            const ordered = patternsFromSelection(slots, selectedKeys);
            if (ordered.length === 0) {
                toast('Selected slot has no pattern payload', 'error');
                return;
            }
            committed = true;
            // Stop any picker audition before handing off. Failure is
            // non-fatal - the next transport start will reset the clock.
            const finish = () => {
                try {
                    if (multi) onImport(ordered);
                    else       onImport(ordered[0]);
                }
                catch (e) { toast(`Import failed: ${e.message}`, 'error'); }
                close();
                resolve();
            };
            if (playingSlotKey) {
                api.transportStop()
                    .catch((e) => console.warn('import-picker stop on commit:', e))
                    .finally(finish);
            } else {
                finish();
            }
        };

        // When the modal goes away for any reason (Esc, backdrop click,
        // Cancel) we need to halt an in-flight audition so the scratch
        // slot doesn't keep ticking. The close() returned by openModal is
        // synchronous - wrap it so we fire stop first, *then* tear down.
        const obs = new MutationObserver(() => {
            if (document.body.contains(body)) return;
            obs.disconnect();
            if (playingSlotKey) {
                api.transportStop().catch((e) =>
                    console.warn('import-picker stop on close:', e)
                );
            }
            if (!committed) resolve();
        });
        obs.observe(document.body, { childList: true, subtree: true });
    });
}

// ---------------------------------------------------------------------------
// Per-slot play button (picker-local state)
// ---------------------------------------------------------------------------
//
// Mirrors the visual shape of bank-play's `makePlayButton` (same classes so
// the shared CSS applies) but talks to our picker-local playback tracker
// instead of the global bank-play bus. The callbacks live at the call site
// in `openImportBankPicker` - this function just wires them to a button.

function makePreviewPlayButton(slot, hooks) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'bank-play-btn bank-play-sm';

    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    btn.appendChild(icon);

    const paint = () => {
        const playing = hooks.isPlaying();
        icon.textContent = playing ? 'stop' : 'play_arrow';
        if (playing) btn.classList.add('is-playing');
        else btn.classList.remove('is-playing');
        btn.title = playing
            ? 'Stop preview'
            : `Preview ${slot.slot_key} on the TD-3`;
        btn.setAttribute('aria-label', btn.title);
    };
    paint();

    const unsub = hooks.subscribe(() => {
        if (!btn.isConnected) { unsub(); return; }
        paint();
    });

    btn.addEventListener('click', async (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        if (btn.disabled) return;
        btn.disabled = true;
        try {
            if (hooks.isPlaying()) await hooks.onStop();
            else                  await hooks.onStart();
        } finally {
            btn.disabled = false;
        }
    });

    return btn;
}

function findPrimaryButton(bodyEl) {
    // openModal adds `.bank-modal-actions` as a sibling of the body
    // container inside `.bank-modal`. The primary button is the one with
    // the `active` class.
    const modal = bodyEl.closest('.bank-modal');
    if (!modal) return null;
    const actions = modal.querySelector('.bank-modal-actions');
    if (!actions) return null;
    return actions.querySelector('button.active') || null;
}
