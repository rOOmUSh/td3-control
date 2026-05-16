// Cross-page handoff: ship Bank patterns into the Control page's
// multipattern canvas without overwriting what's already there.
//
// Flow:
//   1. Resolve item IDs (or snapshot slots) into decoded WebPattern payloads
//      via `bankApi.getItemPattern`.
//   2. POST the batch to `/api/control/queue/append`. The server queues the
//      patterns even when no Control tab is open, capping at MAX_QUEUE = 64.
//   3. Broadcast on `td3-control-queue` so a live Control tab drains the
//      queue immediately. The Control page also drains on boot, so the queue
//      is always consumed exactly once regardless of tab state.
//   4. Toast results: `queued`, `dropped` (overflow), and `failed` (per-id
//      fetch errors) are reported separately.

import { api } from '../api.js';
import { bankApi } from '../bank/bank-api.js';
import { toast } from '../bank/bank-toast.js';
import { confirmModal } from '../bank/bank-modal.js';

export const CONTROL_QUEUE_CHANNEL = 'td3-control-queue';
const CONFIRM_THRESHOLD = 10;

let channel = null;

function getChannel() {
    if (channel) return channel;
    if (typeof BroadcastChannel === 'undefined') return null;
    channel = new BroadcastChannel(CONTROL_QUEUE_CHANNEL);
    return channel;
}

/**
 * Subscribe to live queue notifications. The handler is fired when another
 * tab posts to the queue. Returns an unsubscribe function.
 */
export function subscribeControlQueue(handler) {
    const ch = getChannel();
    if (!ch) return () => {};
    const listener = (ev) => {
        if (ev && ev.data && ev.data.type === 'queued') handler(ev.data);
    };
    ch.addEventListener('message', listener);
    return () => ch.removeEventListener('message', listener);
}

function notifyControlQueue(count) {
    const ch = getChannel();
    if (!ch) return;
    try {
        ch.postMessage({ type: 'queued', count, ts: Date.now() });
    } catch (_) {}
}

/**
 * Add one or more LibraryItem IDs to the Control page. Fetches each item's
 * decoded pattern in parallel, then submits the batch as a single append.
 * `confirmThreshold` opens a confirm modal once `itemIds.length` exceeds it
 * (skip with `skipConfirm: true`).
 */
export async function addItemsToControl(itemIds, opts = {}) {
    const ids = Array.isArray(itemIds) ? itemIds.filter(Boolean) : [];
    if (ids.length === 0) {
        toast('Select one or more items first', 'info');
        return null;
    }
    const skipConfirm = !!opts.skipConfirm;
    if (!skipConfirm && ids.length > CONFIRM_THRESHOLD) {
        const ok = await confirmModal({
            title: 'Add to Control',
            message:
                `${ids.length} patterns will be appended to the Control page.\n\n` +
                'Control caps at 64 patterns; any overflow will be dropped.',
            okLabel: 'Add',
            cancelLabel: 'Cancel',
        });
        if (!ok) return null;
    }

    const results = await Promise.allSettled(ids.map((id) => bankApi.getItemPattern(id)));
    const patterns = [];
    const failedIds = [];
    for (let i = 0; i < results.length; i++) {
        const r = results[i];
        if (r.status === 'fulfilled' && r.value && r.value.pattern) {
            patterns.push(r.value.pattern);
        } else {
            failedIds.push(ids[i]);
        }
    }

    if (patterns.length === 0) {
        toast(`Add to Control failed: no patterns could be loaded (${failedIds.length} error${failedIds.length === 1 ? '' : 's'})`, 'error');
        return { queued: 0, dropped: 0, failed: failedIds.length };
    }

    return submit(patterns, { failed: failedIds.length });
}

/**
 * Add already-decoded WebPattern objects to the Control page. Used by the
 * snapshot-detail view where slot rows already carry their patterns - no
 * extra fetch needed.
 */
export async function addPatternsToControl(patterns, opts = {}) {
    const list = Array.isArray(patterns) ? patterns.filter(Boolean) : [];
    if (list.length === 0) {
        toast('No patterns to add', 'info');
        return null;
    }
    const skipConfirm = !!opts.skipConfirm;
    if (!skipConfirm && list.length > CONFIRM_THRESHOLD) {
        const ok = await confirmModal({
            title: 'Add to Control',
            message:
                `${list.length} patterns will be appended to the Control page.\n\n` +
                'Control caps at 64 patterns; any overflow will be dropped.',
            okLabel: 'Add',
            cancelLabel: 'Cancel',
        });
        if (!ok) return null;
    }
    return submit(list, { failed: 0 });
}

async function submit(patterns, { failed }) {
    let res;
    try {
        res = await api.appendControlQueue(patterns);
    } catch (e) {
        toast(`Add to Control failed: ${e.message || e}`, 'error');
        return null;
    }
    const queued = res && typeof res.queued === 'number' ? res.queued : 0;
    const dropped = res && typeof res.dropped === 'number' ? res.dropped : 0;

    if (queued > 0) notifyControlQueue(queued);

    const parts = [];
    if (queued > 0) parts.push(`Queued ${queued} pattern${queued === 1 ? '' : 's'} for Control`);
    if (dropped > 0) parts.push(`${dropped} dropped (Control is full)`);
    if (failed > 0) parts.push(`${failed} could not be loaded`);
    const kind = queued > 0 && dropped === 0 && failed === 0 ? 'success' : (queued > 0 ? 'info' : 'error');
    toast(parts.join(' - ') || 'Nothing to add', kind);

    return { queued, dropped, failed };
}
