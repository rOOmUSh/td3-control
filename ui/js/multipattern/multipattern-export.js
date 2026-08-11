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

export const PATTERN_SET_NAME_INPUT_MAX_LENGTH = 120;
export const PATTERN_SET_NAME_MAX_BYTES = 120;

function utf8Length(character) {
    const codePoint = character.codePointAt(0);
    if (codePoint <= 0x7f) return 1;
    if (codePoint <= 0x7ff) return 2;
    if (codePoint <= 0xffff) return 3;
    return 4;
}

function truncateUtf8(value, maxBytes) {
    let bytes = 0;
    let result = '';
    for (const character of value) {
        const characterBytes = utf8Length(character);
        if (bytes + characterBytes > maxBytes) break;
        result += character;
        bytes += characterBytes;
    }
    return result;
}

export function sanitizePatternSetName(value) {
    let sanitized = String(value ?? '')
        .trim()
        .replace(/[\u0000-\u001f\u007f<>:"/\\|?*]/g, '_')
        .replace(/[ .]+$/g, '');

    if (/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(sanitized)) {
        sanitized = `_${sanitized}`;
    }
    return truncateUtf8(sanitized, PATTERN_SET_NAME_MAX_BYTES)
        .replace(/[ .]+$/g, '');
}

export function formatPatternExportTimestamp(date = new Date()) {
    const pad = value => String(value).padStart(2, '0');
    return [
        date.getFullYear(),
        pad(date.getMonth() + 1),
        pad(date.getDate()),
        pad(date.getHours()),
        pad(date.getMinutes()),
        pad(date.getSeconds()),
    ].join('-');
}

export function buildRbsExportFilename(patternSetName, ext = 'rbs') {
    const safeName = sanitizePatternSetName(patternSetName);
    if (!safeName) return null;
    return `${safeName}.${ext}`;
}

export function buildSingleFileExportPlan(patterns, checkedIndexes, ext, patternSetName) {
    const selection = selectPatternExportItems(patterns, checkedIndexes);
    if (selection.error) {
        return { files: null, count: 0, error: selection.error };
    }

    const safeName = sanitizePatternSetName(patternSetName);
    if (!safeName) {
        return { files: null, count: 0, error: 'empty-name' };
    }

    const files = selection.items.map((item) => ({
        index: item.index,
        pattern: item.pattern,
        filename: `P${String(item.index + 1).padStart(3, '0')}_${safeName}.${ext}`,
    }));

    return { files, count: files.length, error: null };
}
