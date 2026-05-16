// Fetch wrappers for the /api/bank/* backend. Mirrors the pattern in
// ui/js/api.js: every call throws an Error with the server's `error` field
// populated when the response is non-2xx, so callers can pipe exceptions
// straight to the toast helper.
//
// Global transport/MIDI endpoints (status, connect, disconnect, transport
// stop, transport bpm) live under /api/* rather than /api/bank/*, so we
// delegate to the shared `api` client instead of re-implementing the
// fetches. This keeps both pages (Bank + Control) on a single contract.

import { api } from '../api.js';

const BASE = '/api/bank';

async function request(method, path, body, query) {
    const qs = query ? buildQueryString(query) : '';
    const opts = { method, headers: { 'Content-Type': 'application/json' } };
    if (body !== undefined) opts.body = JSON.stringify(body);
    let res;
    try {
        res = await fetch(BASE + path + qs, opts);
    } catch (e) {
        // Network-level failure: surface with an explicit, non-empty message.
        throw new Error(`network error: ${e.message || e}`);
    }
    const text = await res.text();
    let json = {};
    if (text) {
        try { json = JSON.parse(text); }
        catch { throw new Error(`invalid JSON from ${path}: ${text.slice(0, 200)}`); }
    }
    if (!res.ok) {
        throw new Error(json.error || `HTTP ${res.status} ${path}`);
    }
    return json;
}

function buildQueryString(obj) {
    const parts = [];
    for (const [k, v] of Object.entries(obj)) {
        if (v === undefined || v === null || v === '') continue;
        if (v === false && k !== 'archived') continue;
        if (Array.isArray(v)) {
            for (const x of v) parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(x)}`);
        } else {
            parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
        }
    }
    return parts.length ? '?' + parts.join('&') : '';
}

// ItemFilter fields on the backend are all optional; we drop empty values
// so the server sees them as absent rather than empty-string.
function sanitizeFilter(filter) {
    const out = {};
    if (!filter) return out;
    const keys = [
        'search', 'format', 'source_kind', 'favorite', 'archived',
        'duplicate_only', 'related_only', 'failed_imports_only',
        'snapshot_id', 'slot_key', 'scale', 'root', 'tag',
        'date_from', 'date_to', 'needs_review',
    ];
    for (const k of keys) {
        const v = filter[k];
        if (v === undefined || v === null) continue;
        if (typeof v === 'string' && v.trim() === '') continue;
        // `archived=false` is a real constraint in the Bank UI: the default
        // library view hides archived items until the user explicitly opts in.
        if (typeof v === 'boolean' && !v && k !== 'archived') continue;
        out[k] = v;
    }
    return out;
}

export const bankApi = {
    // Items
    listItems:      (filter)        => request('GET', '/items', undefined, sanitizeFilter(filter)),
    getItem:        (id)            => request('GET', `/items/${encodeURIComponent(id)}`),
    getItemPattern: (id)            => request('GET', `/items/${encodeURIComponent(id)}/pattern`),
    deleteItem:     (id)            => request('DELETE', `/items/${encodeURIComponent(id)}/delete`),
    toggleFavorite: (id, favorite)  => request('POST', `/items/${encodeURIComponent(id)}/favorite`, { favorite }),
    setArchived:    (id, archived)  => request('POST', `/items/${encodeURIComponent(id)}/archive`, { archived }),

    // Tags
    listTags:       ()              => request('GET', '/tags'),
    addTag:         (itemId, label) => request('POST', `/items/${encodeURIComponent(itemId)}/tags`, { label }),
    removeTag:      (itemId, tag)   => request('DELETE', `/items/${encodeURIComponent(itemId)}/tags/${encodeURIComponent(tag)}`),
    bulkTag:        (payload)       => request('POST', '/items/bulk-tag', payload),

    // Snapshots
    listSnapshots:  ()              => request('GET', '/snapshots'),
    getSnapshot:    (id)            => request('GET', `/snapshots/${encodeURIComponent(id)}`),
    createSnapshot: (body)          => request('POST', '/snapshots', body),
    deleteSnapshot: (id)            => request('DELETE', `/snapshots/${encodeURIComponent(id)}`),
    addItemToSnapshot: (id, body)   => request('POST', `/items/${encodeURIComponent(id)}/add-to-snapshot`, body || {}),
    savePatternsToBank: (body)      => request('POST', '/patterns/save', body),
    // Main-page overflow: POST `{ name, description?, slots: [{slot_key, pattern}] }`.
    // Backend decodes every pattern up front (atomic) and resolves name
    // collisions by appending " (N)". Returns the full SnapshotDetailResponse.
    createSnapshotFromPatterns: (body) => request('POST', '/snapshots/from-patterns', body),
    updateSnapshot: (id, patch)     => request('PATCH', `/snapshots/${encodeURIComponent(id)}`, patch),
    // Omit `backup_dir` to use the server's default.
    syncBackups:    (backup_dir)    => request('POST', '/snapshots/sync-backups',
                                              backup_dir ? { backup_dir } : {}),
    /**
     * Export N selected slots from a snapshot as individual pattern files
     * into a `{source}_export` folder created inside `target_dir`. See
     * `web::snapshot_export` for naming + format rules.
     */
    exportSnapshotPatterns: (id, body) =>
        request('POST', `/snapshots/${encodeURIComponent(id)}/export-patterns`, body),
    /**
     * Delete the listed `slot_keys` from a snapshot. Underlying LibraryItems
     * are left alone - only the snapshot↔slot mapping is removed. The
     * snapshot's `slot_count` is refreshed server-side; callers should
     * re-fetch via `getSnapshot` after calling this.
     */
    deleteSnapshotSlots: (id, slot_keys) =>
        request('DELETE', `/snapshots/${encodeURIComponent(id)}/slots`, { slot_keys }),
    /**
     * Move (or swap) the slot stored at `from_key` to `to_key` inside the
     * snapshot. When `to_key` is empty the row is renamed in place; when
     * `to_key` is occupied the two rows trade places. Returns
     * `{ swapped, snapshot, slots }` so callers can re-render the grid in a
     * single round trip.
     */
    moveSnapshotSlot: (id, from_key, to_key) =>
        request('POST', `/snapshots/${encodeURIComponent(id)}/move-slot`, { from_key, to_key }),

    // Scan / import
    scan:           ({ path, recursive }) => request('POST', '/scan', { path, recursive }),
    scanJob:        (id)            => request('GET', `/scan/${encodeURIComponent(id)}`),
    scanProgress:   ()              => request('GET', '/scan/progress'),
    browseFolder:   ()              => request('GET', '/browse-folder'),
    importFiles:    (paths)         => request('POST', '/import', { paths }),
    listImportBatches: ()           => request('GET', '/import-batches'),
    getImportBatch: (id)            => request('GET', `/import-batches/${encodeURIComponent(id)}`),
    retryFailedBatch: (id)          => request('POST', `/import-batches/${encodeURIComponent(id)}/retry-failed`),
    deleteImportBatch: (id)         => request('DELETE', `/import-batches/${encodeURIComponent(id)}`),

    // Compare + merge
    compareItems:     (a, b)        => request('GET', '/compare/items', undefined, { a, b }),
    compareSnapshots: (src, dst)    => request('GET', '/compare/snapshots', undefined, { src, dst }),
    buildMergePlan:   (body)        => request('POST', '/merge-plan', body),
    previewMergePlan: (body)        => request('POST', '/merge-plan/preview', body),

    // Related + duplicates
    listRelated:    (kind)          => request('GET', '/related', undefined, kind ? { kind } : undefined),
    listDuplicates: ()              => request('GET', '/duplicates'),

    // Audition - play a single LibraryItem on the device by uploading it to
    // the scratch slot and starting the transport. Stop re-uses the global
    // /api/transport/stop endpoint which also clears `playing_item_id`.
    playItem:       (id, bpm)       => request(
        'POST',
        `/items/${encodeURIComponent(id)}/play`,
        undefined,
        bpm ? { centibpm: Math.round(bpm * 100) } : undefined,
    ),
    getPlaying:     ()              => request('GET', '/playing'),
    stopPlayback:   ()              => api.transportStop(),

    // Connection + BPM controls used by the Bank footer. These live under
    // /api/* (not /api/bank/*), so they delegate to the shared `api` client
    // to stay in lockstep with the Control page.
    midiStatus:     ()              => api.status(),
    midiConnect:    ()              => api.connect(),
    midiDisconnect: ()              => api.disconnect(),
    transportBpm:   (bpm)           => api.transportBpm(bpm),
};
