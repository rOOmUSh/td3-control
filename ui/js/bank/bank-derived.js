// Helpers that derive UI-facing item decorations from the related/duplicate
// endpoints so the main item list does not depend on stale persisted fields.

export function decorateItems(items, { related, duplicates } = {}) {
    const relatedCounts = new Map();
    for (const group of related?.groups || []) {
        for (const itemId of group?.item_ids || []) {
            relatedCounts.set(itemId, (relatedCounts.get(itemId) || 0) + 1);
        }
    }

    const duplicateStatuses = new Map();
    for (const cluster of duplicates?.clusters || []) {
        const status = cluster?.kind === 'exact' ? 'exactduplicate'
            : cluster?.kind === 'near' ? 'nearduplicate'
            : null;
        if (!status) continue;
        for (const itemId of cluster?.item_ids || []) {
            duplicateStatuses.set(itemId, status);
        }
    }

    return (items || []).map((item) => ({
        ...item,
        related_group_count: relatedCounts.get(item.item_id) || 0,
        duplicate_status: duplicateStatuses.get(item.item_id) || item.duplicate_status || 'unknown',
    }));
}
