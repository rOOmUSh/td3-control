export function selectPatternExportItems(patterns, checkedIndexes) {
    if (!Array.isArray(patterns) || patterns.length === 0) {
        return { items: null, error: 'no-patterns' };
    }

    const indexes = Array.isArray(checkedIndexes) && checkedIndexes.length > 0
        ? [...checkedIndexes].sort((a, b) => a - b)
        : patterns.map((_pattern, index) => index);

    const items = [];
    for (const index of indexes) {
        if (!Number.isInteger(index) || index < 0 || index >= patterns.length) {
            return { items: null, error: 'index-out-of-range' };
        }
        items.push({ index, pattern: patterns[index] });
    }

    if (items.length === 0) {
        return { items: null, error: 'no-patterns' };
    }

    return { items, error: null };
}

export function buildRbsExportPayload(patterns, checkedIndexes, mode) {
    if (mode !== 'ALTERNATE' && mode !== 'SERIAL') {
        return { payload: null, count: 0, error: 'bad-mode' };
    }

    const selection = selectPatternExportItems(patterns, checkedIndexes);
    if (selection.error) {
        return { payload: null, count: 0, error: selection.error };
    }
    const selected = selection.items.map(item => item.pattern);

    return {
        payload: {
            pattern: selected[0],
            patterns: selected,
            rbs_mode: mode,
        },
        count: selected.length,
        error: null,
    };
}

export function buildSingleFileExportPlan(patterns, checkedIndexes, ext, selectedSlot) {
    const selection = selectPatternExportItems(patterns, checkedIndexes);
    if (selection.error) {
        return { files: null, count: 0, error: selection.error };
    }

    const multi = selection.items.length > 1;
    const files = selection.items.map((item) => ({
        index: item.index,
        pattern: item.pattern,
        filename: multi
            ? `pattern_P${String(item.index + 1).padStart(3, '0')}.${ext}`
            : singlePatternFilename(selectedSlot, ext),
    }));

    return { files, count: files.length, error: null };
}

function singlePatternFilename(slot, ext) {
    const group = slot && Number.isInteger(slot.group) ? slot.group : 1;
    const pattern = slot && Number.isInteger(slot.pattern) ? slot.pattern : 1;
    const side = slot && typeof slot.side === 'string' ? slot.side : 'A';
    return `pattern_G${group}P${pattern}${side}.${ext}`;
}
